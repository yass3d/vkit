use std::cell::Cell;
use std::io::{BufReader, Cursor};
use std::rc::Rc;

use serde_json::json;
use vkit_core::fit::LandmarkWarpOptions;
use vkit_core::formats::{
    DazGeometry, GroupTable, Mesh, SurfaceAttachment, parse_ordered_obj, write_ordered_obj,
};
use vkit_core::pipeline::{
    AnatomyPreservation, ManualFitRequest, NumericSurfacePin, PipelineError, PipelineStage,
    RigidAnatomyComponent, SimilarityAnatomyComponent, TopologyGuardOptions, run_manual_fit,
};
use vkit_core::symmetry::SymmetryOptions;

fn canonical_vertices() -> Vec<[f64; 3]> {
    vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ]
}

fn faces() -> Vec<Vec<u32>> {
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

fn template() -> DazGeometry {
    let faces = faces();
    DazGeometry::new(
        "synthetic-g2".into(),
        canonical_vertices(),
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
    .unwrap()
}

fn shifted_scan(shift: [f64; 3]) -> Mesh {
    Mesh::new(
        canonical_vertices()
            .into_iter()
            .map(|point| {
                [
                    point[0] + shift[0],
                    point[1] + shift[1],
                    point[2] + shift[2],
                ]
            })
            .collect(),
        faces()
            .into_iter()
            .map(|face| [face[0], face[1], face[2]])
            .collect(),
    )
    .unwrap()
}

fn attachment(primitive: usize, barycentric: [f64; 3]) -> SurfaceAttachment {
    let face = &faces()[primitive];
    SurfaceAttachment {
        triangle_vertex_ids: [face[0], face[1], face[2]],
        barycentric,
        primitive_id: Some(primitive as u32),
    }
}

fn pins() -> Vec<NumericSurfacePin> {
    (0..12)
        .map(|index| {
            let primitive = index % faces().len();
            let barycentric = match index % 3 {
                0 => [0.6, 0.2, 0.2],
                1 => [0.2, 0.6, 0.2],
                _ => [0.2, 0.2, 0.6],
            };
            NumericSurfacePin {
                pair_index: index as u32,
                scan: attachment(primitive, barycentric),
                template: attachment(primitive, barycentric),
                alignment_weight: 1.0,
                fit_weight: 1.0,
                confidence: 1.0,
            }
        })
        .collect()
}

fn request() -> ManualFitRequest {
    ManualFitRequest {
        scan: shifted_scan([3.0, -2.0, 0.75]),
        template: template(),
        pins: pins(),
        symmetry: SymmetryOptions::default(),
        seam_vertices: Vec::new(),
        anchor_weights: Vec::new(),
        warp: LandmarkWarpOptions::default(),
        topology_guard: TopologyGuardOptions::default(),
        anatomy: AnatomyPreservation {
            rigid_components: vec![RigidAnatomyComponent {
                vertices: vec![4],
                control_vertices: vec![0, 1, 2],
            }],
            similarity_components: vec![SimilarityAnatomyComponent {
                vertices: vec![5],
                control_vertices: vec![0, 1, 2, 3],
                minimum_scale: 0.98,
                maximum_scale: 1.02,
            }],
        },
        scan_fidelity: 1.0,
    }
}

fn run(request: &ManualFitRequest) -> Result<vkit_core::pipeline::ManualFitResult, PipelineError> {
    run_manual_fit(request, |_| {}, || false)
}

#[test]
fn small_manual_fit_is_deterministic_and_restores_template_space() {
    let request = request();
    let first = run(&request).unwrap();
    let second = run(&request).unwrap();
    assert_eq!(first, second);
    assert!(first.topology.valid);
    assert_eq!(first.output.faces.len(), request.template.faces.len());
    assert_eq!(first.aligned_scan.triangles, request.scan.triangles);
    assert_eq!(first.anatomy.rigid_translations.len(), 1);
    assert_eq!(first.anatomy.similarity_transforms.len(), 1);

    let maximum_error = first
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
        .fold(0.0_f64, f64::max);
    assert!(
        maximum_error < 1.0e-9,
        "template-space error {maximum_error}"
    );
}

#[test]
fn progress_is_monotonic_complete_and_stage_ordered() {
    let mut events = Vec::new();
    let result = run_manual_fit(&request(), |event| events.push(event), || false).unwrap();
    assert!(result.topology.valid);
    assert_eq!(events.first().unwrap().stage, PipelineStage::Validation);
    assert_eq!(events.last().unwrap().stage, PipelineStage::Complete);
    assert_eq!(events.last().unwrap().completed_units, 1_000);
    assert!(events.windows(2).all(|pair| {
        pair[0].completed_units <= pair[1].completed_units && pair[0].stage < pair[1].stage
    }));
    assert!(
        events
            .iter()
            .all(|event| { event.total_units == 1_000 && (0.0..=1.0).contains(&event.fraction()) })
    );
}

#[test]
fn cancellation_stops_at_the_reported_boundary() {
    let cancel = Rc::new(Cell::new(false));
    let progress_flag = Rc::clone(&cancel);
    let cancel_flag = Rc::clone(&cancel);
    let error = run_manual_fit(
        &request(),
        move |event| {
            if event.stage == PipelineStage::ScanSymmetry {
                progress_flag.set(true);
            }
        },
        move || cancel_flag.get(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PipelineError::Cancelled(PipelineStage::ScanSymmetry)
    ));
}

#[test]
fn skewed_pair_indices_and_insufficient_pairs_are_rejected() {
    let mut skewed = request();
    skewed.pins[5].pair_index = 8;
    assert!(matches!(
        run(&skewed),
        Err(PipelineError::PinIndexMismatch {
            position: 5,
            actual: 8
        })
    ));

    let mut insufficient = request();
    insufficient.pins.truncate(11);
    assert!(matches!(
        run(&insufficient),
        Err(PipelineError::TooFewFitPins {
            required: 12,
            actual: 11
        })
    ));
}

#[test]
fn surface_pin_primitive_mismatch_is_not_silently_resolved() {
    let mut request = request();
    request.pins[3].scan.triangle_vertex_ids = attachment(4, [0.6, 0.2, 0.2]).triangle_vertex_ids;
    let error = run(&request).unwrap_err();
    assert!(matches!(
        error,
        PipelineError::PinTriangleMismatch {
            pin: 3,
            side: vkit_core::pipeline::PinSide::Scan
        }
    ));
}

#[test]
fn generated_obj_round_trip_preserves_order_winding_and_materials() {
    let result = run(&request()).unwrap();
    let mut bytes = Vec::new();
    write_ordered_obj(&mut bytes, &result.output).unwrap();
    let parsed = parse_ordered_obj(BufReader::new(Cursor::new(bytes))).unwrap();
    assert_eq!(parsed, result.output);
    assert_eq!(
        parsed
            .faces
            .iter()
            .map(|face| face.vertex_indices.clone())
            .collect::<Vec<_>>(),
        faces()
    );
    assert!(parsed.faces.iter().all(|face| {
        face.group.as_deref() == Some("Face") && face.material.as_deref() == Some("Face")
    }));
}
