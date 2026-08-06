use approx::assert_abs_diff_eq;
use serde_json::json;
use vkit_core::fit::{
    AssistedAlignmentRejection, AssistedFitOptions, AssistedIcpSkipReason,
    AssistedInitializationMode, AutomaticConstraintRejection, AutomaticSurfaceConstraintOptions,
    GeometryInitializationMode, GeometryInitializationRejection, GeometryInitializationRequest,
    GuardedWeightedFallbackRejection, MIN_ASSISTED_FIT_PAIRS, RECOMMENDED_ASSISTED_FIT_PAIRS,
    RobustInitializationFallback, SparseInitializationFallback,
    build_automatic_surface_correspondences, estimate_assisted_similarity,
    estimate_assisted_similarity_with_prior, initialize_geometry_assisted_similarity,
    refine_assisted_similarity_icp,
};
use vkit_core::formats::{DazGeometry, GroupTable, Mesh, SurfaceAttachment};
use vkit_core::math::{Mat3, SimilarityTransform, Vec3};
use vkit_core::pipeline::{
    AnatomyPreservation, GeometryAssistedFitOptions, ManualFitRequest, NumericSurfacePin,
    PipelineError, TopologyGuardOptions, run_assisted_fit, run_geometry_assisted_fit,
    run_manual_fit, run_prior_assisted_fit,
};
use vkit_core::symmetry::SymmetryOptions;
use vkit_core::{MIN_ALIGNMENT_PAIRS, MIN_FIT_PAIRS};

fn expected_transform() -> SimilarityTransform {
    let angle = 0.21_f64;
    SimilarityTransform {
        scale: 1.35,
        rotation: Mat3::from_rows([
            [angle.cos(), 0.0, angle.sin()],
            [0.0, 1.0, 0.0],
            [-angle.sin(), 0.0, angle.cos()],
        ]),
        translation: Vec3::new(2.0, -1.25, 0.4),
    }
}

fn six_spread_points() -> Vec<Vec3> {
    vec![
        Vec3::new(-2.0, -1.0, 0.0),
        Vec3::new(2.0, -0.8, 0.3),
        Vec3::new(-1.4, 2.2, 0.7),
        Vec3::new(1.7, 2.5, -0.5),
        Vec3::new(0.0, 0.4, 1.8),
        Vec3::new(0.3, -0.2, -1.5),
    ]
}

fn maximum_error(transform: SimilarityTransform, source: &[Vec3], target: &[Vec3]) -> f64 {
    source
        .iter()
        .copied()
        .zip(target.iter().copied())
        .map(|(source, target)| (transform.apply(source) - target).norm())
        .fold(0.0, f64::max)
}

#[test]
fn six_noisy_pairs_reject_one_large_outlier_deterministically() {
    assert_eq!(RECOMMENDED_ASSISTED_FIT_PAIRS, 6);
    let source = six_spread_points();
    let expected = expected_transform();
    let mut target = expected.apply_slice(&source);
    let noise = [
        [0.01, -0.01, 0.00],
        [-0.01, 0.00, 0.01],
        [0.00, 0.01, -0.01],
        [0.01, 0.00, 0.01],
        [-0.01, -0.01, 0.00],
    ];
    for (point, noise) in target.iter_mut().take(5).zip(noise) {
        *point += Vec3::from(noise);
    }
    target[5] = Vec3::new(45.0, -35.0, 28.0);

    let first =
        estimate_assisted_similarity(&source, &target, &[1.0; 6], &AssistedFitOptions::default())
            .unwrap();
    let second =
        estimate_assisted_similarity(&source, &target, &[1.0; 6], &AssistedFitOptions::default())
            .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.receipt.inlier_count, 5);
    assert_eq!(first.receipt.outlier_count, 1);
    assert!(first.pin_weight_multipliers[5] <= 1.0e-4);
    assert!(maximum_error(first.transform, &source[..5], &target[..5]) < 0.04);
}

#[test]
fn four_well_spread_pairs_recover_similarity() {
    assert_eq!(MIN_ASSISTED_FIT_PAIRS, 4);
    let source = &six_spread_points()[..4];
    let expected = expected_transform();
    let target = expected.apply_slice(source);
    let result =
        estimate_assisted_similarity(source, &target, &[1.0; 4], &AssistedFitOptions::default())
            .unwrap();
    assert_eq!(result.receipt.inlier_count, 4);
    assert_eq!(result.receipt.outlier_count, 0);
    assert_abs_diff_eq!(result.transform.scale, expected.scale, epsilon = 1.0e-12);
    assert!(maximum_error(result.transform, source, &target) < 1.0e-11);
}

fn eleven_face_points() -> Vec<Vec3> {
    vec![
        Vec3::new(-4.8, 1.2, 0.0),
        Vec3::new(4.8, 1.2, 0.1),
        Vec3::new(-2.8, 3.2, 0.8),
        Vec3::new(2.8, 3.2, 0.9),
        Vec3::new(-1.6, 0.8, 2.3),
        Vec3::new(1.6, 0.8, 2.4),
        Vec3::new(0.0, 0.0, 3.2),
        Vec3::new(-2.4, -2.0, 1.4),
        Vec3::new(2.4, -2.0, 1.5),
        Vec3::new(0.0, -3.8, 0.8),
        Vec3::new(0.0, 4.8, -0.2),
    ]
}

#[test]
fn eleven_moderately_nonrigid_pairs_use_guarded_weighted_prior_fallback() {
    let source = eleven_face_points();
    let expected = expected_transform();
    let mut target = expected.apply_slice(&source);
    let nonrigid_offsets = [
        [-0.55, 0.18, 0.45],
        [0.58, 0.12, -0.44],
        [-0.48, 0.42, -0.38],
        [0.52, 0.38, 0.41],
        [-0.42, -0.32, 0.50],
        [0.46, -0.28, -0.52],
        [0.05, 0.04, 0.02],
        [-0.62, -0.18, -0.35],
        [0.66, -0.16, 0.38],
        [0.02, -0.08, 0.05],
        [0.00, 0.06, -0.04],
    ];
    for (point, offset) in target.iter_mut().zip(nonrigid_offsets) {
        *point += Vec3::from(offset);
    }
    let strict =
        estimate_assisted_similarity(&source, &target, &[1.0; 11], &AssistedFitOptions::default())
            .unwrap_err();
    assert_eq!(
        strict.receipt.rejection_reason,
        Some(AssistedAlignmentRejection::InsufficientRobustInliers)
    );
    assert_eq!(strict.receipt.inlier_count, 5);

    let prior = SimilarityTransform {
        translation: expected.translation + Vec3::new(0.45, -0.20, 0.10),
        ..expected
    };
    let recovered = estimate_assisted_similarity_with_prior(
        &source,
        &target,
        &[1.0; 11],
        prior,
        &AssistedFitOptions::default(),
    )
    .unwrap();
    assert_eq!(
        recovered.receipt.initialization_mode,
        AssistedInitializationMode::GuardedWeightedPrior
    );
    assert_eq!(recovered.receipt.fallback_rejection_reason, None);
    assert!(
        recovered.receipt.fallback_candidate_rms_cm.unwrap()
            < recovered.receipt.fallback_prior_rms_cm.unwrap()
    );
    assert!(
        recovered.receipt.fallback_candidate_rms_cm.unwrap()
            <= recovered.receipt.fallback_maximum_rms_cm.unwrap()
    );
    assert!(
        recovered
            .pin_weight_multipliers
            .iter()
            .all(|weight| *weight > 0.0)
    );
}

#[test]
fn incoherent_eleven_pair_layout_remains_fail_closed() {
    let source = eleven_face_points();
    let mut target = expected_transform().apply_slice(&source);
    target.rotate_left(4);
    let prior = expected_transform();
    let error = estimate_assisted_similarity_with_prior(
        &source,
        &target,
        &[1.0; 11],
        prior,
        &AssistedFitOptions::default(),
    )
    .unwrap_err();
    assert_eq!(
        error.receipt.rejection_reason,
        Some(AssistedAlignmentRejection::InsufficientRobustInliers)
    );
    assert!(matches!(
        error.receipt.fallback_rejection_reason,
        Some(
            GuardedWeightedFallbackRejection::ExcessivePriorStep
                | GuardedWeightedFallbackRejection::ExcessiveResidual
                | GuardedWeightedFallbackRejection::NoResidualImprovement
        )
    ));
}

#[test]
fn collinear_layout_is_rejected_with_a_receipt() {
    let source = (0..6)
        .map(|index| Vec3::new(index as f64, 0.0, 0.0))
        .collect::<Vec<_>>();
    let target = expected_transform().apply_slice(&source);
    let error =
        estimate_assisted_similarity(&source, &target, &[1.0; 6], &AssistedFitOptions::default())
            .unwrap_err();
    assert_eq!(
        error.receipt.rejection_reason,
        Some(AssistedAlignmentRejection::DegeneratePinLayout)
    );
    assert_eq!(error.receipt.pair_count, 6);
    assert_eq!(error.receipt.inlier_count, 0);
    assert!(error.receipt.source_spread <= 1.0e-12);
}

#[test]
fn similarity_icp_uses_real_surface_correspondences_and_clamped_steps() {
    let template_vertices = octahedron_vertices()
        .into_iter()
        .map(Vec3::from)
        .collect::<Vec<_>>();
    let template_triangles = octahedron_faces()
        .into_iter()
        .map(|face| [face[0], face[1], face[2]])
        .collect::<Vec<_>>();
    let scan_shift = Vec3::new(1.0, -0.45, 0.25);
    let scan = Mesh::new(
        template_vertices
            .iter()
            .map(|vertex| (*vertex + scan_shift).to_array())
            .collect(),
        template_triangles.clone(),
    )
    .unwrap();
    let initial = SimilarityTransform {
        translation: Vec3::new(-0.82, 0.36, -0.18),
        ..SimilarityTransform::IDENTITY
    };
    let options = AssistedFitOptions {
        icp_iterations: 3,
        icp_sample_count: 6,
        icp_max_distance_cm: 1.0,
        icp_min_normal_dot: -1.0,
        icp_trim_fraction: 0.0,
        icp_minimum_correspondences: 4,
        icp_minimum_coverage: 0.5,
        icp_max_translation_step_cm: 0.15,
        icp_max_rotation_step_radians: 0.15,
        icp_max_scale_step: 0.10,
        icp_minimum_rms_improvement_cm: 0.0,
        ..Default::default()
    };
    let (refined, iterations, skip) = refine_assisted_similarity_icp(
        initial,
        &scan,
        &template_vertices,
        &template_triangles,
        &options,
    );
    let expected_translation = scan_shift * -1.0;
    assert!(!iterations.is_empty());
    assert!(iterations.iter().any(|iteration| iteration.accepted));
    assert!(
        (refined.translation - expected_translation).norm()
            < (initial.translation - expected_translation).norm()
    );
    assert!(matches!(
        skip,
        None | Some(AssistedIcpSkipReason::NoImprovement)
    ));
}

struct SparseFixture {
    scan: Mesh,
    template_vertices: Vec<Vec3>,
    template_triangles: Vec<[u32; 3]>,
    prior: SimilarityTransform,
    scan_points: Vec<Vec3>,
    template_points: Vec<Vec3>,
}

fn sparse_initialization_fixture(pair_count: usize) -> SparseFixture {
    let template_vertices = octahedron_vertices()
        .into_iter()
        .map(Vec3::from)
        .collect::<Vec<_>>();
    let template_triangles = octahedron_faces()
        .into_iter()
        .map(|face| [face[0], face[1], face[2]])
        .collect::<Vec<_>>();
    let shift = Vec3::new(0.55, -0.30, 0.18);
    let scan = Mesh::new(
        template_vertices
            .iter()
            .map(|vertex| (*vertex + shift).to_array())
            .collect(),
        template_triangles.clone(),
    )
    .unwrap();
    let prior = SimilarityTransform {
        translation: Vec3::new(-0.43, 0.24, -0.13),
        ..SimilarityTransform::IDENTITY
    };
    let indices = [0_usize, 2, 4, 1];
    let scan_points = indices[..pair_count]
        .iter()
        .map(|&index| template_vertices[index] + shift)
        .collect::<Vec<_>>();
    let template_points = indices[..pair_count]
        .iter()
        .map(|&index| template_vertices[index])
        .collect::<Vec<_>>();
    SparseFixture {
        scan,
        template_vertices,
        template_triangles,
        prior,
        scan_points,
        template_points,
    }
}

fn sparse_options() -> AssistedFitOptions {
    AssistedFitOptions {
        icp_iterations: 3,
        icp_sample_count: 6,
        icp_max_distance_cm: 1.0,
        icp_min_normal_dot: -1.0,
        icp_trim_fraction: 0.0,
        icp_minimum_correspondences: 4,
        icp_minimum_coverage: 0.5,
        icp_max_translation_step_cm: 0.25,
        icp_max_rotation_step_radians: 0.15,
        icp_max_scale_step: 0.10,
        icp_minimum_rms_improvement_cm: 0.0,
        ..Default::default()
    }
}

#[test]
fn zero_through_three_pairs_refine_the_manual_prior_deterministically() {
    let modes = [
        GeometryInitializationMode::ManualPrior,
        GeometryInitializationMode::PriorWithOnePair,
        GeometryInitializationMode::PriorWithTwoPairs,
        GeometryInitializationMode::PriorWithThreePairs,
    ];
    for (pair_count, expected_mode) in modes.into_iter().enumerate() {
        let fixture = sparse_initialization_fixture(pair_count);
        let weights = vec![1.0; pair_count];
        let run = || {
            initialize_geometry_assisted_similarity(
                GeometryInitializationRequest {
                    prior: fixture.prior,
                    scan: &fixture.scan,
                    template_vertices: &fixture.template_vertices,
                    template_triangles: &fixture.template_triangles,
                    scan_points: &fixture.scan_points,
                    template_points: &fixture.template_points,
                    weights: &weights,
                },
                &sparse_options(),
            )
            .unwrap()
        };
        let first = run();
        let second = run();
        assert_eq!(first, second, "pair count {pair_count}");
        assert_eq!(first.receipt.pair_count, pair_count);
        assert_eq!(first.receipt.mode, expected_mode);
        assert_eq!(first.receipt.rejection_reason, None);
        assert_eq!(first.pin_weight_multipliers, vec![1.0; pair_count]);
        let before = fixture
            .template_vertices
            .iter()
            .zip(&fixture.scan.vertices)
            .map(|(template, scan)| (fixture.prior.apply(Vec3::from(*scan)) - *template).norm())
            .sum::<f64>();
        let after = fixture
            .template_vertices
            .iter()
            .zip(&fixture.scan.vertices)
            .map(|(template, scan)| (first.transform.apply(Vec3::from(*scan)) - *template).norm())
            .sum::<f64>();
        assert!(after <= before + 1.0e-12, "pair count {pair_count}");
    }
}

#[test]
fn degenerate_sparse_layout_falls_back_and_invalid_prior_is_rejected_with_receipts() {
    let fixture = sparse_initialization_fixture(0);
    let duplicate_source = [Vec3::new(0.5, 0.0, 0.0); 2];
    let duplicate_target = [Vec3::new(0.4, 0.0, 0.0); 2];
    let result = initialize_geometry_assisted_similarity(
        GeometryInitializationRequest {
            prior: fixture.prior,
            scan: &fixture.scan,
            template_vertices: &fixture.template_vertices,
            template_triangles: &fixture.template_triangles,
            scan_points: &duplicate_source,
            template_points: &duplicate_target,
            weights: &[1.0; 2],
        },
        &sparse_options(),
    )
    .unwrap();
    assert_eq!(
        result.receipt.sparse_fallback,
        Some(SparseInitializationFallback::DegeneratePairToTranslation)
    );
    assert_eq!(
        result.receipt.mode,
        GeometryInitializationMode::PriorWithOnePair
    );

    let error = initialize_geometry_assisted_similarity(
        GeometryInitializationRequest {
            prior: SimilarityTransform {
                scale: 0.0,
                ..SimilarityTransform::IDENTITY
            },
            scan: &fixture.scan,
            template_vertices: &fixture.template_vertices,
            template_triangles: &fixture.template_triangles,
            scan_points: &[],
            template_points: &[],
            weights: &[],
        },
        &sparse_options(),
    )
    .unwrap_err();
    assert_eq!(
        error.receipt.rejection_reason,
        Some(GeometryInitializationRejection::InvalidPrior)
    );
}

#[test]
fn four_pair_robust_delta_is_continuous_with_three_pair_prior_refinement() {
    let options = AssistedFitOptions {
        icp_minimum_correspondences: 7,
        ..sparse_options()
    };
    let run = |pair_count| {
        let fixture = sparse_initialization_fixture(pair_count);
        let weights = vec![1.0; pair_count];
        let result = initialize_geometry_assisted_similarity(
            GeometryInitializationRequest {
                prior: fixture.prior,
                scan: &fixture.scan,
                template_vertices: &fixture.template_vertices,
                template_triangles: &fixture.template_triangles,
                scan_points: &fixture.scan_points,
                template_points: &fixture.template_points,
                weights: &weights,
            },
            &options,
        )
        .unwrap();
        (fixture, result)
    };
    let (three_fixture, three) = run(3);
    let (_, four) = run(4);
    assert_eq!(
        three.receipt.mode,
        GeometryInitializationMode::PriorWithThreePairs
    );
    assert_eq!(
        four.receipt.mode,
        GeometryInitializationMode::RobustLandmarkPairs
    );
    assert_eq!(four.receipt.robust_fallback, None);
    let three_target = three
        .transform
        .apply_slice(&three_fixture.template_vertices);
    assert!(
        maximum_error(
            four.transform,
            &three_fixture.template_vertices,
            &three_target,
        ) < 1.0e-10
    );
}

#[test]
fn degenerate_four_pair_layout_keeps_prior_and_records_robust_fallback() {
    let fixture = sparse_initialization_fixture(0);
    let source = [Vec3::new(0.25, -0.5, 0.1); 4];
    let target = [Vec3::new(-0.1, 0.2, 0.3); 4];
    let result = initialize_geometry_assisted_similarity(
        GeometryInitializationRequest {
            prior: fixture.prior,
            scan: &fixture.scan,
            template_vertices: &fixture.template_vertices,
            template_triangles: &fixture.template_triangles,
            scan_points: &source,
            template_points: &target,
            weights: &[1.0; 4],
        },
        &AssistedFitOptions {
            icp_minimum_correspondences: 7,
            ..sparse_options()
        },
    )
    .unwrap();
    assert_eq!(result.transform, fixture.prior);
    assert_eq!(
        result.receipt.robust_fallback,
        Some(RobustInitializationFallback::Rejected(
            AssistedAlignmentRejection::DegeneratePinLayout
        ))
    );
    assert_eq!(result.receipt.rejection_reason, None);
    assert_eq!(result.pin_weight_multipliers, vec![1.0; 4]);
}

#[test]
fn one_outlier_in_four_optional_pairs_cannot_worsen_valid_prior() {
    let fixture = sparse_initialization_fixture(4);
    let shift = Vec3::from(fixture.scan.vertices[0]) - fixture.template_vertices[0];
    let prior = SimilarityTransform {
        translation: shift * -1.0,
        ..SimilarityTransform::IDENTITY
    };
    let mut target = fixture.template_points.clone();
    target[3] = Vec3::new(80.0, -65.0, 42.0);
    let result = initialize_geometry_assisted_similarity(
        GeometryInitializationRequest {
            prior,
            scan: &fixture.scan,
            template_vertices: &fixture.template_vertices,
            template_triangles: &fixture.template_triangles,
            scan_points: &fixture.scan_points,
            template_points: &target,
            weights: &[1.0; 4],
        },
        &AssistedFitOptions {
            icp_minimum_correspondences: 7,
            ..sparse_options()
        },
    )
    .unwrap();
    assert!(
        result.receipt.pin_rms_after_cm.unwrap()
            <= result.receipt.pin_rms_before_cm.unwrap() + 1.0e-10
    );
    assert_eq!(result.receipt.rejection_reason, None);
}

#[test]
fn determinant_one_shear_is_rejected_as_an_invalid_geometry_prior() {
    let fixture = sparse_initialization_fixture(0);
    let error = initialize_geometry_assisted_similarity(
        GeometryInitializationRequest {
            prior: SimilarityTransform {
                rotation: Mat3::from_rows([[1.0, 0.2, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
                ..SimilarityTransform::IDENTITY
            },
            scan: &fixture.scan,
            template_vertices: &fixture.template_vertices,
            template_triangles: &fixture.template_triangles,
            scan_points: &[],
            template_points: &[],
            weights: &[],
        },
        &sparse_options(),
    )
    .unwrap_err();
    assert_eq!(
        error.receipt.rejection_reason,
        Some(GeometryInitializationRejection::InvalidPrior)
    );
}

#[test]
fn automatic_surface_constraints_are_deterministic_and_fail_closed_on_low_coverage() {
    let fixture = sparse_initialization_fixture(0);
    let template = Mesh::new(
        fixture
            .template_vertices
            .iter()
            .map(|vertex| vertex.to_array())
            .collect(),
        fixture.template_triangles.clone(),
    )
    .unwrap();
    let candidate_ids = (0..fixture.template_triangles.len() as u32).collect::<Vec<_>>();
    let options = AutomaticSurfaceConstraintOptions {
        sample_count: 8,
        maximum_distance_cm: 1.0,
        minimum_normal_dot: -1.0,
        trim_fraction: 0.0,
        minimum_constraints: 4,
        minimum_coverage: 0.5,
        ..Default::default()
    };
    let first = build_automatic_surface_correspondences(
        &fixture.scan,
        &template,
        &candidate_ids,
        fixture.prior,
        &options,
    )
    .unwrap();
    let second = build_automatic_surface_correspondences(
        &fixture.scan,
        &template,
        &candidate_ids,
        fixture.prior,
        &options,
    )
    .unwrap();
    assert_eq!(first, second);
    assert!(first.correspondences.len() >= 4);
    assert_eq!(first.receipt.rejection_reason, None);

    let rejected = build_automatic_surface_correspondences(
        &fixture.scan,
        &template,
        &candidate_ids,
        fixture.prior,
        &AutomaticSurfaceConstraintOptions {
            maximum_distance_cm: 1.0e-6,
            ..options
        },
    )
    .unwrap_err();
    assert_eq!(
        rejected.receipt.rejection_reason,
        Some(AutomaticConstraintRejection::InsufficientCorrespondences)
    );
}

#[test]
fn automatic_surface_constraints_do_not_count_one_scan_primitive_repeatedly() {
    let scan = Mesh::new(
        vec![[-2.0, -2.0, 0.0], [2.0, -2.0, 0.0], [0.0, 2.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .unwrap();
    let template = Mesh::new(
        vec![
            [-0.8, -0.4, 0.0],
            [-0.3, -0.4, 0.0],
            [-0.55, 0.1, 0.0],
            [0.3, -0.4, 0.0],
            [0.8, -0.4, 0.0],
            [0.55, 0.1, 0.0],
            [-0.2, 0.3, 0.0],
            [0.2, 0.3, 0.0],
            [0.0, 0.8, 0.0],
        ],
        vec![[0, 1, 2], [3, 4, 5], [6, 7, 8]],
    )
    .unwrap();
    let error = build_automatic_surface_correspondences(
        &scan,
        &template,
        &[0, 1, 2],
        SimilarityTransform::IDENTITY,
        &AutomaticSurfaceConstraintOptions {
            sample_count: 3,
            maximum_distance_cm: 0.1,
            minimum_normal_dot: -1.0,
            trim_fraction: 0.0,
            minimum_constraints: 3,
            minimum_coverage: 0.5,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.receipt.accepted_constraints, 1);
    assert_eq!(error.receipt.duplicate_scan_primitives_removed, 2);
    assert_abs_diff_eq!(error.receipt.coverage, 1.0 / 3.0, epsilon = 1.0e-12);
    assert_eq!(
        error.receipt.rejection_reason,
        Some(AutomaticConstraintRejection::InsufficientCorrespondences)
    );
}

fn octahedron_vertices() -> Vec<[f64; 3]> {
    vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ]
}

fn octahedron_faces() -> Vec<Vec<u32>> {
    vec![
        vec![0, 2, 4],
        vec![2, 1, 4],
        vec![1, 3, 4],
        vec![3, 0, 4],
        vec![2, 0, 5],
        vec![1, 2, 5],
        vec![3, 1, 5],
        vec![0, 3, 5],
    ]
}

fn generic_request_with_pins(pin_count: usize) -> ManualFitRequest {
    let faces = octahedron_faces();
    let template = DazGeometry::new(
        "generic".into(),
        octahedron_vertices(),
        faces.clone(),
        GroupTable {
            indices: vec![0; faces.len()],
            names: vec!["Head".into()],
        },
        GroupTable {
            indices: vec![0; faces.len()],
            names: vec!["Face".into()],
        },
        json!({}),
    )
    .unwrap();
    let shift = Vec3::new(3.0, -2.0, 0.75);
    let scan = Mesh::new(
        octahedron_vertices()
            .into_iter()
            .map(Vec3::from)
            .map(|point| (point + shift).to_array())
            .collect(),
        faces
            .iter()
            .map(|face| [face[0], face[1], face[2]])
            .collect(),
    )
    .unwrap();
    let selected_faces = [0_usize, 2, 4, 6, 1, 3, 5, 7];
    let barycentrics = [[0.2, 0.3, 0.5], [0.3, 0.4, 0.3], [0.45, 0.2, 0.35]];
    let pins = (0..pin_count)
        .enumerate()
        .map(|(pair_index, source_index)| {
            let primitive = selected_faces[source_index % selected_faces.len()];
            let face = &faces[primitive];
            let attachment = SurfaceAttachment {
                triangle_vertex_ids: [face[0], face[1], face[2]],
                barycentric: barycentrics[(source_index / selected_faces.len()) % 3],
                primitive_id: Some(primitive as u32),
            };
            NumericSurfacePin {
                pair_index: pair_index as u32,
                scan: attachment.clone(),
                template: attachment,
                alignment_weight: 1.0,
                fit_weight: 1.0,
                confidence: 1.0,
            }
        })
        .collect();
    ManualFitRequest {
        scan,
        template,
        pins,
        symmetry: SymmetryOptions::default(),
        seam_vertices: Vec::new(),
        anchor_weights: Vec::new(),
        warp: Default::default(),
        topology_guard: TopologyGuardOptions::default(),
        anatomy: AnatomyPreservation::default(),
        scan_fidelity: 1.0,
    }
}

fn generic_request_with_four_pins() -> ManualFitRequest {
    generic_request_with_pins(4)
}

fn semantic_surface_request() -> ManualFitRequest {
    let rings = 7_usize;
    let segments = 16_usize;
    let mut vertices = vec![[0.0, 1.0, 0.0]];
    for latitude in 1..rings {
        let phi = std::f64::consts::PI * latitude as f64 / rings as f64;
        for longitude in 0..segments {
            let theta = std::f64::consts::TAU * longitude as f64 / segments as f64;
            vertices.push([phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()]);
        }
    }
    let bottom = vertices.len() as u32;
    vertices.push([0.0, -1.0, 0.0]);

    let mut faces = Vec::new();
    for longitude in 0..segments {
        faces.push(vec![
            0,
            1 + longitude as u32,
            1 + ((longitude + 1) % segments) as u32,
        ]);
    }
    for latitude in 0..rings - 2 {
        let first = 1 + latitude * segments;
        let second = first + segments;
        for longitude in 0..segments {
            let next = (longitude + 1) % segments;
            faces.push(vec![
                (first + longitude) as u32,
                (second + longitude) as u32,
                (second + next) as u32,
            ]);
            faces.push(vec![
                (first + longitude) as u32,
                (second + next) as u32,
                (first + next) as u32,
            ]);
        }
    }
    let last_ring = 1 + (rings - 2) * segments;
    for longitude in 0..segments {
        faces.push(vec![
            (last_ring + longitude) as u32,
            bottom,
            (last_ring + (longitude + 1) % segments) as u32,
        ]);
    }

    let template = DazGeometry::new(
        "semantic-surface".into(),
        vertices.clone(),
        faces.clone(),
        GroupTable {
            indices: vec![0; faces.len()],
            names: vec!["Head".into()],
        },
        GroupTable {
            indices: vec![0; faces.len()],
            names: vec!["Face".into()],
        },
        json!({}),
    )
    .unwrap();
    let shift = Vec3::new(3.0, -2.0, 0.75);
    let scan = Mesh::new(
        vertices
            .into_iter()
            .map(Vec3::from)
            .map(|point| (point + shift).to_array())
            .collect(),
        faces
            .iter()
            .map(|face| [face[0], face[1], face[2]])
            .collect(),
    )
    .unwrap();
    let selected = [20_usize, 30, 48, 62, 80, 94, 112, 126];
    let pins = selected
        .into_iter()
        .enumerate()
        .map(|(pair_index, template_primitive)| {
            let scan_primitive = if pair_index % 2 == 0 {
                template_primitive + 2
            } else {
                template_primitive - 2
            };
            let scan_face = &faces[scan_primitive];
            let template_face = &faces[template_primitive];
            NumericSurfacePin {
                pair_index: pair_index as u32,
                scan: SurfaceAttachment {
                    triangle_vertex_ids: [scan_face[0], scan_face[1], scan_face[2]],
                    barycentric: [0.2, 0.3, 0.5],
                    primitive_id: Some(scan_primitive as u32),
                },
                template: SurfaceAttachment {
                    triangle_vertex_ids: [template_face[0], template_face[1], template_face[2]],
                    barycentric: [0.2, 0.3, 0.5],
                    primitive_id: Some(template_primitive as u32),
                },
                alignment_weight: 1.0,
                fit_weight: 1.0,
                confidence: 1.0,
            }
        })
        .collect();
    ManualFitRequest {
        scan,
        template,
        pins,
        symmetry: SymmetryOptions::default(),
        seam_vertices: Vec::new(),
        anchor_weights: Vec::new(),
        warp: Default::default(),
        topology_guard: TopologyGuardOptions::default(),
        anatomy: AnatomyPreservation::default(),
        scan_fidelity: 1.0,
    }
}

fn semantic_geometry_options() -> GeometryAssistedFitOptions {
    GeometryAssistedFitOptions {
        prior: SimilarityTransform {
            translation: Vec3::new(-3.0, 2.0, -0.75),
            ..SimilarityTransform::IDENTITY
        },
        initialization: AssistedFitOptions {
            icp_iterations: 2,
            icp_sample_count: 64,
            icp_max_distance_cm: 1.5,
            icp_min_normal_dot: -1.0,
            icp_trim_fraction: 0.0,
            icp_minimum_correspondences: 12,
            icp_minimum_coverage: 0.2,
            icp_minimum_rms_improvement_cm: 0.0,
            ..Default::default()
        },
        automatic_constraints: AutomaticSurfaceConstraintOptions {
            sample_count: 96,
            maximum_distance_cm: 1.5,
            minimum_normal_dot: -1.0,
            trim_fraction: 0.0,
            minimum_constraints: 24,
            minimum_coverage: 0.2,
            ..Default::default()
        },
        user_constraint_weight_multiplier: 4.0,
    }
}

fn pin_residual_rms(
    result: &vkit_core::pipeline::ManualFitResult,
    pins: &[NumericSurfacePin],
) -> f64 {
    let squared = pins
        .iter()
        .map(|pin| {
            let scan = pin
                .scan
                .triangle_vertex_ids
                .iter()
                .zip(pin.scan.barycentric)
                .map(|(&index, weight)| {
                    Vec3::from(result.prepared_scan.vertices[index as usize]) * weight
                })
                .fold(Vec3::ZERO, |sum, point| sum + point);
            let target = result.alignment.apply(scan);
            let output = pin
                .template
                .triangle_vertex_ids
                .iter()
                .zip(pin.template.barycentric)
                .map(|(&index, weight)| Vec3::from(result.output.vertices[index as usize]) * weight)
                .fold(Vec3::ZERO, |sum, point| sum + point);
            (output - target).norm_squared()
        })
        .sum::<f64>();
    (squared / pins.len() as f64).sqrt()
}

fn generic_request_with_incoherent_pins(pin_count: usize) -> ManualFitRequest {
    let mut request = generic_request_with_pins(0);
    let faces = octahedron_faces();
    let scan_face = &faces[0];
    let scan = SurfaceAttachment {
        triangle_vertex_ids: [scan_face[0], scan_face[1], scan_face[2]],
        barycentric: [0.2, 0.3, 0.5],
        primitive_id: Some(0),
    };
    request.pins = (0..pin_count)
        .map(|pair_index| {
            let primitive = pair_index % faces.len();
            let template_face = &faces[primitive];
            NumericSurfacePin {
                pair_index: pair_index as u32,
                scan: scan.clone(),
                template: SurfaceAttachment {
                    triangle_vertex_ids: [template_face[0], template_face[1], template_face[2]],
                    barycentric: [0.2, 0.3, 0.5],
                    primitive_id: Some(primitive as u32),
                },
                alignment_weight: 1.0,
                fit_weight: 1.0,
                confidence: 1.0,
            }
        })
        .collect();
    request
}

#[test]
fn assisted_entry_accepts_four_but_manual_contract_and_guards_stay_frozen() {
    assert_eq!(MIN_ALIGNMENT_PAIRS, 8);
    assert_eq!(MIN_FIT_PAIRS, 12);
    assert_eq!(
        TopologyGuardOptions::canonical_g2(),
        TopologyGuardOptions {
            minimum_orientation_cosine: 0.03,
            minimum_area_ratio: 0.08,
            maximum_area_ratio: 4.0,
        }
    );
    let request = generic_request_with_four_pins();
    assert!(matches!(
        run_manual_fit(&request, |_| {}, || false),
        Err(PipelineError::TooFewAlignmentPins {
            required: 8,
            actual: 4
        })
    ));

    let assisted =
        run_assisted_fit(&request, &AssistedFitOptions::default(), |_| {}, || false).unwrap();
    assert!(assisted.fit.topology.valid);
    assert_eq!(assisted.assisted.inlier_count, 4);
    assert_eq!(
        assisted.assisted.icp_skip_reason,
        Some(AssistedIcpSkipReason::NonCanonicalTemplate)
    );
    let maximum_output_error = assisted
        .fit
        .output
        .vertices
        .iter()
        .zip(&request.template.vertices)
        .map(|(actual, expected)| {
            (0..3)
                .map(|axis| (actual[axis] - expected[axis]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .fold(0.0, f64::max);
    assert!(maximum_output_error < 1.0e-12);
}

#[test]
fn prior_assisted_pipeline_completes_moderately_nonrigid_eleven_pin_layout() {
    let mut request = semantic_surface_request();
    let template_primitives = [20_usize, 30, 48, 62, 80, 94, 112, 126, 140, 154, 168];
    let scan_offsets = [4_isize, -4, 5, -5, 4, -4, 5, -5, 4, -4, 5];
    request.pins = template_primitives
        .into_iter()
        .zip(scan_offsets)
        .enumerate()
        .map(|(pair_index, (template_primitive, offset))| {
            let scan_primitive = (template_primitive as isize + offset) as usize;
            let scan_face = &request.template.faces[scan_primitive];
            let template_face = &request.template.faces[template_primitive];
            NumericSurfacePin {
                pair_index: pair_index as u32,
                scan: SurfaceAttachment {
                    triangle_vertex_ids: [scan_face[0], scan_face[1], scan_face[2]],
                    barycentric: [0.2, 0.3, 0.5],
                    primitive_id: Some(scan_primitive as u32),
                },
                template: SurfaceAttachment {
                    triangle_vertex_ids: [template_face[0], template_face[1], template_face[2]],
                    barycentric: [0.2, 0.3, 0.5],
                    primitive_id: Some(template_primitive as u32),
                },
                alignment_weight: 1.0,
                fit_weight: 1.0,
                confidence: 1.0,
            }
        })
        .collect();
    let strict =
        run_assisted_fit(&request, &AssistedFitOptions::default(), |_| {}, || false).unwrap_err();
    let PipelineError::AssistedAlignment(strict) = strict else {
        panic!("expected strict robust initialization rejection: {strict:?}");
    };
    assert_eq!(
        strict.receipt.rejection_reason,
        Some(AssistedAlignmentRejection::InsufficientRobustInliers)
    );
    assert_eq!(strict.receipt.inlier_count, 5);

    let prior = SimilarityTransform {
        translation: Vec3::new(-2.70, 1.85, -0.65),
        ..SimilarityTransform::IDENTITY
    };
    let recovered = run_prior_assisted_fit(
        &request,
        &AssistedFitOptions::default(),
        prior,
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(
        recovered.assisted.initialization_mode,
        AssistedInitializationMode::GuardedWeightedPrior
    );
    assert_eq!(
        recovered.assisted.icp_skip_reason,
        Some(AssistedIcpSkipReason::NonCanonicalTemplate),
        "the public guarded-prior entry keeps its established ICP policy"
    );
    assert!(recovered.fit.topology.valid);
    assert_eq!(recovered.pin_weight_multipliers.len(), 11);
}

#[test]
fn geometry_assisted_entry_covers_every_routing_pin_band_deterministically() {
    for pin_count in [0, 1, 2, 3, 4, 11, MIN_FIT_PAIRS + 1] {
        let request = generic_request_with_pins(pin_count);
        let options = GeometryAssistedFitOptions {
            prior: SimilarityTransform {
                translation: Vec3::new(-3.0, 2.0, -0.75),
                ..SimilarityTransform::IDENTITY
            },
            initialization: AssistedFitOptions {
                icp_iterations: 2,
                icp_sample_count: 6,
                icp_max_distance_cm: 1.0,
                icp_min_normal_dot: -1.0,
                icp_trim_fraction: 0.0,
                icp_minimum_correspondences: 4,
                icp_minimum_coverage: 0.5,
                icp_minimum_rms_improvement_cm: 0.0,
                ..Default::default()
            },
            automatic_constraints: AutomaticSurfaceConstraintOptions {
                sample_count: 8,
                maximum_distance_cm: 1.0,
                minimum_normal_dot: -1.0,
                trim_fraction: 0.0,
                minimum_constraints: 4,
                minimum_coverage: 0.5,
                ..Default::default()
            },
            user_constraint_weight_multiplier: 4.0,
        };
        let first = run_geometry_assisted_fit(&request, &options, |_| {}, || false).unwrap();
        let second = run_geometry_assisted_fit(&request, &options, |_| {}, || false).unwrap();
        assert_eq!(first, second, "pin count {pin_count}");
        assert!(first.fit.topology.valid);
        assert_eq!(first.receipt.user_pair_count, pin_count);
        assert!(first.receipt.automatic_pair_count >= 4);
        assert_eq!(first.user_pin_weight_multipliers.len(), pin_count);
        assert_eq!(
            first.receipt.automatic_constraints_applied_to_guarded_fit,
            pin_count < MIN_ASSISTED_FIT_PAIRS,
            "pin count {pin_count}"
        );
        if pin_count >= MIN_ASSISTED_FIT_PAIRS {
            assert_eq!(first.receipt.effective_automatic_alignment_weight, 0.0);
            assert_eq!(first.receipt.effective_automatic_fit_weight, 0.0);
            assert_eq!(first.receipt.guarded_fit.pair_count, pin_count);
        }
        assert_eq!(
            first.receipt.initialization.mode,
            if pin_count >= MIN_ASSISTED_FIT_PAIRS {
                GeometryInitializationMode::RobustLandmarkPairs
            } else {
                [
                    GeometryInitializationMode::ManualPrior,
                    GeometryInitializationMode::PriorWithOnePair,
                    GeometryInitializationMode::PriorWithTwoPairs,
                    GeometryInitializationMode::PriorWithThreePairs,
                ][pin_count]
            }
        );
        if pin_count >= MIN_ASSISTED_FIT_PAIRS {
            assert_eq!(
                first.receipt.initialization.icp_skip_reason,
                Some(AssistedIcpSkipReason::AuthoritativeUserPins),
                "pin-anchored initialization must not run all-surface ICP"
            );
            assert_eq!(
                first.receipt.guarded_fit.icp_skip_reason,
                Some(AssistedIcpSkipReason::AuthoritativeUserPins),
                "the private guarded geometry route must preserve user-owned scale"
            );
            assert!(first.receipt.guarded_fit.icp_iterations.is_empty());
        }
        assert_eq!(
            first.receipt.automatic_constraints.provenance,
            vkit_core::fit::AutomaticConstraintProvenance::ClosestSurfaceProjection
        );
        let maximum_output_error = first
            .fit
            .output
            .vertices
            .iter()
            .zip(&request.template.vertices)
            .map(|(actual, expected)| {
                (0..3)
                    .map(|axis| (actual[axis] - expected[axis]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .fold(0.0, f64::max);
        assert!(maximum_output_error < 1.0e-8, "pin count {pin_count}");
    }
}

#[test]
fn geometry_assistance_preserves_strong_manual_pin_shape() {
    let request = semantic_surface_request();
    let options = semantic_geometry_options();
    let manual = run_assisted_fit(&request, &options.initialization, |_| {}, || false).unwrap();
    let geometry = run_geometry_assisted_fit(&request, &options, |_| {}, || false).unwrap();
    let manual_rms = pin_residual_rms(&manual.fit, &request.pins);
    let geometry_rms = pin_residual_rms(&geometry.fit, &request.pins);
    assert!(geometry.receipt.automatic_pair_count >= 24);
    assert_eq!(geometry.receipt.effective_automatic_alignment_weight, 0.0);
    assert_eq!(geometry.receipt.effective_automatic_fit_weight, 0.0);
    assert!(
        !geometry
            .receipt
            .automatic_constraints_applied_to_guarded_fit
    );
    assert_eq!(
        geometry.receipt.guarded_fit.pair_count,
        request.pins.len(),
        "automatic pairs must not vote in robust alignment once user coverage is sufficient"
    );
    assert_abs_diff_eq!(manual_rms, 0.008_208_749_481, epsilon = 1.0e-9);
    assert!(
        geometry_rms <= 0.005,
        "automatic surface constraints diluted semantic pin shape: {geometry_rms} cm RMS"
    );
    assert!(
        geometry_rms <= manual_rms * 0.35,
        "geometry assistance must improve rather than dilute user-pin residuals: manual={manual_rms} geometry={geometry_rms}"
    );
}

#[test]
fn strong_user_fit_survives_unavailable_automatic_correspondences_without_auto_fallback() {
    let request = semantic_surface_request();
    let mut options = semantic_geometry_options();
    options.automatic_constraints.minimum_constraints =
        options.automatic_constraints.sample_count + 1;

    let result = run_geometry_assisted_fit(&request, &options, |_| {}, || false).unwrap();
    assert!(result.fit.topology.valid);
    assert_eq!(result.receipt.automatic_pair_count, 0);
    assert_eq!(
        result.receipt.automatic_constraints.rejection_reason,
        Some(AutomaticConstraintRejection::InvalidOptions)
    );
    assert!(!result.receipt.automatic_constraints_applied_to_guarded_fit);
    assert_eq!(result.receipt.guarded_fit.pair_count, request.pins.len());
}

#[test]
fn one_user_pin_remains_a_local_hint_without_disabling_automatic_fit() {
    let mut one_pin = semantic_surface_request();
    one_pin.pins.truncate(1);
    let mut zero_pin = one_pin.clone();
    zero_pin.pins.clear();
    let options = semantic_geometry_options();

    let automatic = run_geometry_assisted_fit(&zero_pin, &options, |_| {}, || false).unwrap();
    let hinted = run_geometry_assisted_fit(&one_pin, &options, |_| {}, || false).unwrap();
    let automatic_rms = pin_residual_rms(&automatic.fit, &one_pin.pins);
    let hinted_rms = pin_residual_rms(&hinted.fit, &one_pin.pins);

    assert!(hinted.receipt.automatic_pair_count >= 24);
    assert!(hinted.receipt.effective_automatic_alignment_weight > 0.0);
    assert!(hinted.receipt.automatic_constraints_applied_to_guarded_fit);
    assert!(hinted.receipt.guarded_fit.pair_count > one_pin.pins.len());
    assert!(hinted_rms < automatic_rms);
}

#[test]
fn four_or_more_incoherent_user_pins_never_silently_fall_back_to_automatic_only() {
    let request = generic_request_with_incoherent_pins(32);
    let options = GeometryAssistedFitOptions {
        prior: SimilarityTransform {
            translation: Vec3::new(-3.0, 2.0, -0.75),
            ..SimilarityTransform::IDENTITY
        },
        initialization: AssistedFitOptions {
            icp_iterations: 2,
            icp_sample_count: 6,
            icp_max_distance_cm: 1.0,
            icp_min_normal_dot: -1.0,
            icp_trim_fraction: 0.0,
            icp_minimum_correspondences: 4,
            icp_minimum_coverage: 0.5,
            icp_minimum_rms_improvement_cm: 0.0,
            ..Default::default()
        },
        automatic_constraints: AutomaticSurfaceConstraintOptions {
            sample_count: 8,
            maximum_distance_cm: 1.0,
            minimum_normal_dot: -1.0,
            trim_fraction: 0.0,
            minimum_constraints: 4,
            minimum_coverage: 0.5,
            ..Default::default()
        },
        user_constraint_weight_multiplier: 4.0,
    };

    let error = run_geometry_assisted_fit(&request, &options, |_| {}, || false).unwrap_err();
    let geometry_only =
        run_geometry_assisted_fit(&generic_request_with_pins(0), &options, |_| {}, || false)
            .unwrap();
    let PipelineError::AssistedAlignment(error) = error else {
        panic!("expected the incoherent user layout to fail explicitly: {error:?}");
    };
    assert_eq!(
        error.receipt.rejection_reason,
        Some(AssistedAlignmentRejection::DegeneratePinLayout)
    );
    assert!(geometry_only.fit.topology.valid);
}
