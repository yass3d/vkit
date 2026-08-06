use std::cell::RefCell;

use approx::assert_abs_diff_eq;
use vkit_core::fit::{
    AnchorOperator, BarycentricConstraint, BarycentricLandmarkOperator, CsrMatrix,
    LandmarkWarpError, LandmarkWarpOptions, LinearOperator, OperatorError, StackedOperator,
    UniformLaplacianOperator, landmark_laplacian_warp, topology_safe_step,
};
use vkit_core::math::Vec3;

fn grid_vertices() -> Vec<Vec3> {
    vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(2.0, 1.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(1.0, 2.0, 0.0),
        Vec3::new(2.0, 2.0, 0.0),
    ]
}

fn grid_triangles() -> Vec<[usize; 3]> {
    vec![
        [0, 1, 4],
        [0, 4, 3],
        [1, 2, 5],
        [1, 5, 4],
        [3, 4, 7],
        [3, 7, 6],
        [4, 5, 8],
        [4, 8, 7],
    ]
}

#[test]
fn uniform_laplacian_matches_scipy_row_normalized_definition() {
    let operator = UniformLaplacianOperator::new(4, &[[0, 1, 2], [0, 2, 3]], 2.0).unwrap();
    assert_eq!(operator.neighbors(0), Some(&[1, 2, 3][..]));
    assert_eq!(operator.neighbors(1), Some(&[0, 2][..]));

    let mut product = vec![0.0; 4];
    operator.apply(&[1.0, 2.0, 4.0, 8.0], &mut product).unwrap();
    let expected = [-22.0 / 3.0, -1.0, 2.0 / 3.0, 11.0];
    for (actual, wanted) in product.into_iter().zip(expected) {
        assert_abs_diff_eq!(actual, wanted, epsilon = 2.0e-15);
    }

    let input = [0.25, -2.0, 1.5, 3.0];
    let dual = [-1.0, 0.5, 2.0, -0.75];
    let mut forward = vec![0.0; 4];
    let mut transpose = vec![0.0; 4];
    operator.apply(&input, &mut forward).unwrap();
    operator.apply_transpose(&dual, &mut transpose).unwrap();
    let forward_dot: f64 = forward.iter().zip(dual).map(|(a, b)| a * b).sum();
    let transpose_dot: f64 = input.iter().zip(transpose).map(|(a, b)| a * b).sum();
    assert_abs_diff_eq!(forward_dot, transpose_dot, epsilon = 3.0e-15);
}

#[test]
fn barycentric_constraints_are_canonical_and_clip_effective_weight() {
    let constraint = BarycentricConstraint::new(
        [7, 2, 4],
        [0.2, 0.3, 0.5],
        Vec3::new(1.0, 2.0, 3.0),
        1.0e-3,
        1.0e-3,
    )
    .unwrap();
    assert_eq!(constraint.vertex_indices, [2, 4, 7]);
    assert_eq!(constraint.barycentric, [0.3, 0.5, 0.2]);
    assert_eq!(constraint.effective_weight, 0.05);

    let upper =
        BarycentricConstraint::new([0, 1, 2], [0.5, 0.25, 0.25], Vec3::ZERO, 100.0, 100.0).unwrap();
    assert_eq!(upper.effective_weight, 25.0);

    let operator = BarycentricLandmarkOperator::new(8, &[constraint], 100.0).unwrap();
    let input = [0.0, 0.0, 2.0, 0.0, 4.0, 0.0, 0.0, 8.0];
    let mut output = [0.0];
    operator.apply(&input, &mut output).unwrap();
    assert_abs_diff_eq!(output[0], 4.2 * 5.0_f64.sqrt(), epsilon = 2.0e-15);
}

#[test]
fn invalid_barycentric_constraints_are_rejected() {
    let error =
        BarycentricConstraint::new([0, 1, 2], [0.2, 0.3, 0.4], Vec3::ZERO, 1.0, 1.0).unwrap_err();
    assert!(matches!(error, LandmarkWarpError::BarycentricSum(_)));

    let error =
        BarycentricConstraint::new([0, 0, 2], [0.2, 0.3, 0.5], Vec3::ZERO, 1.0, 1.0).unwrap_err();
    assert_eq!(error, LandmarkWarpError::DuplicateConstraintVertex);
}

#[test]
fn anchor_rows_fix_only_positive_weight_vertices() {
    let operator = AnchorOperator::from_weights(&[0.0, 4.0, 0.0, 9.0]).unwrap();
    assert_eq!(operator.indices(), &[1, 3]);

    let mut product = [0.0; 2];
    operator.apply(&[1.0, 2.0, 3.0, 4.0], &mut product).unwrap();
    assert_eq!(product, [4.0, 12.0]);

    let mut transpose = [99.0; 4];
    operator
        .apply_transpose(&[5.0, 7.0], &mut transpose)
        .unwrap();
    assert_eq!(transpose, [0.0, 10.0, 0.0, 21.0]);
}

#[test]
fn stacked_operator_concatenates_rows_and_accumulates_transpose() {
    let first =
        CsrMatrix::from_triplets(2, 2, [(0, 0, 1.0), (0, 1, 2.0), (1, 0, 3.0), (1, 1, 4.0)])
            .unwrap();
    let second = CsrMatrix::from_triplets(1, 2, [(0, 0, 5.0), (0, 1, 6.0)]).unwrap();
    let stack = StackedOperator::new(vec![&first, &second]).unwrap();
    assert_eq!(stack.block_count(), 2);
    assert_eq!(stack.rows(), 3);
    assert_eq!(stack.columns(), 2);

    let mut product = [0.0; 3];
    stack.apply(&[1.0, 1.0], &mut product).unwrap();
    assert_eq!(product, [3.0, 7.0, 11.0]);

    let mut transpose = [0.0; 2];
    stack
        .apply_transpose(&[1.0, 2.0, 3.0], &mut transpose)
        .unwrap();
    assert_eq!(transpose, [22.0, 28.0]);

    let wrong_columns = CsrMatrix::from_triplets(1, 3, [(0, 0, 1.0)]).unwrap();
    assert_eq!(
        StackedOperator::new(vec![&first, &wrong_columns]).unwrap_err(),
        OperatorError::StackColumnMismatch {
            block: 1,
            expected: 2,
            actual: 3,
        }
    );
}

#[test]
fn initial_warp_matches_scipy_synthetic_oracle() {
    let options = LandmarkWarpOptions::default();
    assert_eq!(options.landmark_weight, 100.0);
    assert_eq!(options.laplacian_weight, 20.0);
    assert_eq!(options.minimum_line_search_tau, 1.0 / 64.0);
    assert_eq!(options.lsmr.absolute_tolerance, 1.0e-8);
    assert_eq!(options.lsmr.relative_tolerance, 1.0e-8);
    assert_eq!(options.lsmr.condition_limit, 1.0e10);
    assert_eq!(options.lsmr.max_iterations, Some(600));

    let base = grid_vertices();
    let triangles = grid_triangles();
    let constraints = vec![
        BarycentricConstraint::new(
            [4, 5, 8],
            [1.0, 0.0, 0.0],
            Vec3::new(1.2, 1.1, 0.4),
            2.0,
            1.0,
        )
        .unwrap(),
        BarycentricConstraint::new(
            [3, 6, 7],
            [0.2, 0.6, 0.2],
            Vec3::new(-0.1, 2.1, 0.2),
            0.5,
            1.0,
        )
        .unwrap(),
        BarycentricConstraint::new(
            [4, 5, 8],
            [0.0, 0.0, 1.0],
            Vec3::new(2.15, 2.0, 0.3),
            4.0,
            1.0,
        )
        .unwrap(),
    ];
    let mut anchors = vec![0.0; base.len()];
    anchors[0] = 1.0e6;
    anchors[1] = 30.0;
    anchors[2] = 1.0e6;

    let result = landmark_laplacian_warp(
        &base,
        &triangles,
        &constraints,
        &[0, 1, 2],
        &anchors,
        options,
        |_| true,
    )
    .unwrap();
    let expected = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [
            -0.162_636_791_852_513_4,
            1.200_687_583_872_778_4,
            0.172_995_283_607_763_03,
        ],
        [
            1.179_241_180_602_725,
            1.092_452_801_052_055_8,
            0.369_508_159_589_366_9,
        ],
        [
            2.103_865_679_798_113,
            0.993_512_595_191_233_8,
            0.149_939_299_190_522_98,
        ],
        [
            -0.290_223_664_999_895_94,
            2.293_566_874_279_582_3,
            0.202_815_119_493_529_6,
        ],
        [
            0.888_681_984_490_069_6,
            2.202_790_477_846_693,
            0.280_278_922_783_599_8,
        ],
        [
            2.146_244_449_040_094_3,
            2.005_089_503_609_298_6,
            0.300_070_150_655_980_1,
        ],
    ];
    for (actual, wanted) in result.vertices.iter().zip(expected) {
        for (coordinate, expected_coordinate) in actual.to_array().into_iter().zip(wanted) {
            assert_abs_diff_eq!(coordinate, expected_coordinate, epsilon = 2.0e-10);
        }
    }

    let report = &result.report;
    assert_eq!(report.line_search_tau, 1.0);
    assert_abs_diff_eq!(
        report.landmark_rms,
        0.053_306_697_099_028_175,
        epsilon = 2.0e-10
    );
    assert_abs_diff_eq!(
        report.landmark_p95,
        0.079_424_912_184_204_37,
        epsilon = 2.0e-10
    );
    assert_abs_diff_eq!(
        report.landmark_weighted_rms,
        0.031_693_022_968_789_8,
        epsilon = 2.0e-10
    );
    assert_abs_diff_eq!(
        report.max_displacement,
        0.459_940_494_080_952_33,
        epsilon = 2.0e-10
    );
    assert_eq!(report.solves.map(|solve| solve.stop.code()), [2, 2, 2]);
    assert_eq!(report.solves.map(|solve| solve.iterations), [11, 11, 11]);
    let expected_conditions = [
        181.862_255_796_736_43,
        171.035_896_912_212_68,
        154.811_329_382_261_1,
    ];
    for (solve, expected_condition) in report.solves.iter().zip(expected_conditions) {
        assert_abs_diff_eq!(
            solve.condition_estimate,
            expected_condition,
            epsilon = 2.0e-6
        );
    }
}

#[test]
fn topology_line_search_halves_tau_and_keeps_seam_exact() {
    let base = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(2.0, 0.0, 0.0),
    ];
    let deltas = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(9.0, 9.0, 9.0),
        Vec3::new(0.0, 2.0, 0.0),
    ];
    let tested = RefCell::new(Vec::new());
    let (vertices, tau) = topology_safe_step(&base, &deltas, &[1], 1.0 / 64.0, |candidate| {
        tested.borrow_mut().push(candidate[0].x);
        candidate[0].x <= 0.25
    })
    .unwrap();
    assert_eq!(tested.into_inner(), vec![1.0, 0.5, 0.25]);
    assert_eq!(tau, 0.25);
    assert_eq!(vertices[1], base[1]);
    assert_eq!(vertices[0], Vec3::new(0.25, 0.0, 0.0));
    assert_eq!(vertices[2], Vec3::new(2.0, 0.5, 0.0));
}

#[test]
fn topology_line_search_reports_failure_and_bad_input() {
    let base = [Vec3::ZERO];
    let deltas = [Vec3::new(1.0, 0.0, 0.0)];
    assert_eq!(
        topology_safe_step(&base, &deltas, &[], 0.25, |_| false),
        Err(LandmarkWarpError::TopologyLineSearchFailed)
    );
    assert_eq!(
        topology_safe_step(&base, &[], &[], 0.25, |_| true),
        Err(LandmarkWarpError::DeltaCountMismatch { base: 1, delta: 0 })
    );
    assert_eq!(
        topology_safe_step(&base, &deltas, &[1], 0.25, |_| true),
        Err(LandmarkWarpError::SeamVertexOutOfBounds(1))
    );
    assert_eq!(
        topology_safe_step(&[Vec3::new(f64::NAN, 0.0, 0.0)], &deltas, &[], 0.25, |_| {
            true
        },),
        Err(LandmarkWarpError::NonFiniteBaseVertex(0))
    );
    assert_eq!(
        topology_safe_step(
            &base,
            &[Vec3::new(0.0, f64::INFINITY, 0.0)],
            &[],
            0.25,
            |_| true,
        ),
        Err(LandmarkWarpError::NonFiniteDeltaVertex(0))
    );
}

#[test]
fn malformed_mesh_and_anchor_inputs_return_typed_errors() {
    let constraint =
        BarycentricConstraint::new([0, 1, 2], [1.0, 0.0, 0.0], Vec3::ZERO, 1.0, 1.0).unwrap();
    let base = [Vec3::ZERO, Vec3::ZERO, Vec3::ZERO];

    let error = landmark_laplacian_warp(
        &base,
        &[[0, 1, 3]],
        &[constraint],
        &[],
        &[0.0; 3],
        LandmarkWarpOptions::default(),
        |_| true,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        LandmarkWarpError::TriangleVertexOutOfBounds { vertex: 3, .. }
    ));

    let error = landmark_laplacian_warp(
        &base,
        &[],
        &[constraint],
        &[],
        &[0.0; 2],
        LandmarkWarpOptions::default(),
        |_| true,
    )
    .unwrap_err();
    assert_eq!(
        error,
        LandmarkWarpError::AnchorCountMismatch {
            expected: 3,
            actual: 2,
        }
    );
}
