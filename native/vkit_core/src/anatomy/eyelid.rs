use std::collections::{BTreeMap, BTreeSet};

use nalgebra::{Matrix4, Vector4};

use crate::formats::{DazGeometry, HEAD_SKIN_MATERIALS};
use crate::math::Vec3;
use crate::spatial::deformation_safety_mask;

use super::{
    AnatomyError, material_and_polygon_group_vertices, material_vertices, polygon_group_vertices,
};

const APERTURE_VERTEX_COUNT: usize = 21;
const TRANSITION_RINGS: usize = 4;
const MAXIMUM_CORRECTION_CM: f64 = 0.35;
const NOOP_TOLERANCE_CM: f64 = 1.0e-10;
const MAXIMUM_GLOBE_RIGIDITY_ERROR_CM: f64 = 5.0e-4;
const MINIMUM_GLOBE_SCALE: f64 = 0.90;
const MAXIMUM_GLOBE_SCALE: f64 = 1.12;
const MINIMUM_ORIENTATION_COSINE: f64 = 0.03;
const MINIMUM_AREA_RATIO: f64 = 0.08;
const MAXIMUM_AREA_RATIO: f64 = 4.0;
const MINIMUM_EDGE_RATIO: f64 = 0.70;
const MAXIMUM_EDGE_RATIO: f64 = 1.45;
const MAXIMUM_LINE_SEARCH_STEPS: usize = 14;
const MAXIMUM_LOCAL_REPAIR_STEPS: usize = 8;

const PIN_HOLD_RADIUS_CM: f64 = 0.9;

const EYE_ATTACHED_MATERIALS: &[&str] = &["Lacrimals", "Tear", "Eyelashes"];
const GLOBE_SURFACE_MATERIALS: &[&str] = &["Sclera", "Cornea"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EyeSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EyelidSideReceipt {
    pub side: EyeSide,
    pub aperture_vertex_count: usize,
    pub affected_skin_vertex_count: usize,
    pub transition_ring_count: usize,
    pub globe_vertex_count: usize,
    pub globe_radius_cm: f64,
    pub globe_fit_rms_cm: f64,
    pub globe_translation: Vec3,
    pub globe_scale: f64,
    pub maximum_globe_rigidity_error_cm: f64,
    pub maximum_rest_error_before_cm: f64,
    pub maximum_rest_error_after_cm: f64,
    pub maximum_requested_displacement_cm: f64,
    pub maximum_applied_displacement_cm: f64,
    pub mean_lower_strength: f64,
    pub mean_upper_inner_strength: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct EyelidCanonicalGuard<'a> {
    pub canonical_vertices: &'a [Vec3],
    pub minimum_orientation_cosine: f64,
    pub minimum_area_ratio: f64,
    pub maximum_area_ratio: f64,
}

#[derive(Clone, Copy, Debug)]
struct SafetyLimits {
    minimum_orientation_cosine: f64,
    minimum_area_ratio: f64,
    maximum_area_ratio: f64,
}

impl EyelidCanonicalGuard<'_> {
    const fn limits(&self) -> SafetyLimits {
        SafetyLimits {
            minimum_orientation_cosine: self.minimum_orientation_cosine,
            minimum_area_ratio: self.minimum_area_ratio,
            maximum_area_ratio: self.maximum_area_ratio,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EyelidConformationReceipt {
    pub left: EyelidSideReceipt,
    pub right: EyelidSideReceipt,
    pub accepted_scale: f64,
    pub line_search_steps: usize,
    pub exact_noop: bool,
    pub protected_vertex_count: usize,
    pub protected_vertices_changed: bool,
    pub minimum_edge_ratio: f64,
    pub maximum_edge_ratio: f64,
    pub minimum_area_ratio: f64,
    pub maximum_area_ratio: f64,
    pub minimum_orientation_cosine: f64,
}

#[derive(Clone, Debug)]
struct SkinTopology {
    skin_mask: Vec<bool>,
    adjacency: Vec<Vec<usize>>,
    edges: Vec<(usize, usize)>,
    triangles: Vec<[usize; 3]>,
    boundary_loops: Vec<Vec<usize>>,
}

#[derive(Clone, Debug)]
struct CompactSafetyTopology {
    global_vertices: Vec<usize>,
    triangles: Vec<[u32; 3]>,
}

impl CompactSafetyTopology {
    fn build(triangles: &[[usize; 3]]) -> Result<Self, AnatomyError> {
        let global_vertices = triangles
            .iter()
            .flat_map(|triangle| triangle.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let local_indices = global_vertices
            .iter()
            .enumerate()
            .map(|(local, &global)| {
                u32::try_from(local)
                    .map(|local| (global, local))
                    .map_err(|_| AnatomyError::InvalidTransitionConfiguration)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let triangles = triangles
            .iter()
            .map(|triangle| {
                Ok([
                    *local_indices
                        .get(&triangle[0])
                        .ok_or(AnatomyError::InvalidTransitionConfiguration)?,
                    *local_indices
                        .get(&triangle[1])
                        .ok_or(AnatomyError::InvalidTransitionConfiguration)?,
                    *local_indices
                        .get(&triangle[2])
                        .ok_or(AnatomyError::InvalidTransitionConfiguration)?,
                ])
            })
            .collect::<Result<Vec<_>, AnatomyError>>()?;
        Ok(Self {
            global_vertices,
            triangles,
        })
    }

    fn gather(&self, vertices: &[Vec3]) -> Vec<[f64; 3]> {
        self.global_vertices
            .iter()
            .map(|&index| vertices[index].to_array())
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
struct Sphere {
    center: Vec3,
    radius: f64,
    rms: f64,
}

struct SidePlan {
    side: EyeSide,
    sphere: Sphere,
    translation: Vec3,
    globe_scale: f64,
    rigidity_error: f64,
    globe_vertex_count: usize,
    aperture: Vec<usize>,
    rest_radii: BTreeMap<usize, f64>,
    displacement: BTreeMap<usize, Vec3>,
    maximum_rest_error_before: f64,
    maximum_requested_displacement: f64,
    mean_lower_strength: f64,
    mean_upper_inner_strength: f64,
}

#[derive(Clone, Copy, Debug)]
struct SafetyMetrics {
    minimum_edge_ratio: f64,
    maximum_edge_ratio: f64,
    minimum_area_ratio: f64,
    maximum_area_ratio: f64,
    minimum_orientation_cosine: f64,
}

#[derive(Clone, Debug)]
pub struct EyelidConformationPlan {
    vertex_count: usize,
    topology_fingerprint: u64,
    topology: SkinTopology,
    compact_safety: CompactSafetyTopology,
    left_globe: Vec<usize>,
    right_globe: Vec<usize>,
    left_eye_surface: Vec<usize>,
    right_eye_surface: Vec<usize>,
    left_aperture: Vec<usize>,
    right_aperture: Vec<usize>,
    protected: BTreeSet<usize>,
}

impl EyelidConformationPlan {
    pub fn build(reference: &DazGeometry) -> Result<Self, AnatomyError> {
        reference
            .validate()
            .map_err(|_| AnatomyError::InvalidTransitionConfiguration)?;
        let reference_vertices = reference
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut topology = build_skin_topology(reference)?;
        let attached = material_vertices(reference, EYE_ATTACHED_MATERIALS)?;
        let left_globe = polygon_group_vertices(reference, &["lEye"])?;
        let right_globe = polygon_group_vertices(reference, &["rEye"])?;
        let left_eye_surface = eye_surface_vertices(reference, "lEye", &left_globe)?;
        let right_eye_surface = eye_surface_vertices(reference, "rEye", &right_globe)?;
        let left_sphere = fit_eye_sphere(
            "lEye",
            left_globe.len(),
            &left_eye_surface,
            &reference_vertices,
        )?;
        let right_sphere = fit_eye_sphere(
            "rEye",
            right_globe.len(),
            &right_eye_surface,
            &reference_vertices,
        )?;
        let (left_aperture, right_aperture) = choose_eye_apertures(
            &topology.boundary_loops,
            &reference_vertices,
            left_sphere,
            right_sphere,
        )?;
        let protected = attached
            .iter()
            .chain(&left_globe)
            .chain(&right_globe)
            .copied()
            .collect();

        let potentially_moved = skin_rings(&left_aperture, &topology, &protected)?
            .into_keys()
            .chain(skin_rings(&right_aperture, &topology, &protected)?.into_keys())
            .collect::<BTreeSet<_>>();
        topology
            .edges
            .retain(|(a, b)| potentially_moved.contains(a) || potentially_moved.contains(b));
        topology.triangles.retain(|triangle| {
            triangle
                .iter()
                .any(|index| potentially_moved.contains(index))
        });
        let compact_safety = CompactSafetyTopology::build(&topology.triangles)?;
        Ok(Self {
            vertex_count: reference.vertices.len(),
            topology_fingerprint: eyelid_topology_fingerprint(reference),
            topology,
            compact_safety,
            left_globe,
            right_globe,
            left_eye_surface,
            right_eye_surface,
            left_aperture,
            right_aperture,
            protected,
        })
    }

    pub fn conform(
        &self,
        reference: &DazGeometry,
        vertices: &mut [Vec3],
    ) -> Result<EyelidConformationReceipt, AnatomyError> {
        conform_eyelids_to_globes_with_plan(self, reference, vertices, true)
    }

    pub fn conform_bound_vertices(
        &self,
        reference_vertices: &[Vec3],
        vertices: &mut [Vec3],
    ) -> Result<EyelidConformationReceipt, AnatomyError> {
        conform_eyelids_to_globes_with_reference_vertices(
            self,
            reference_vertices,
            vertices,
            None,
            &[],
        )
    }

    pub fn conform_bound_vertices_with_canonical_guard(
        &self,
        reference_vertices: &[Vec3],
        vertices: &mut [Vec3],
        guard: EyelidCanonicalGuard<'_>,
    ) -> Result<EyelidConformationReceipt, AnatomyError> {
        conform_eyelids_to_globes_with_reference_vertices(
            self,
            reference_vertices,
            vertices,
            Some(guard),
            &[],
        )
    }

    pub fn conform_bound_vertices_holding_pins(
        &self,
        reference_vertices: &[Vec3],
        vertices: &mut [Vec3],
        guard: EyelidCanonicalGuard<'_>,
        pinned_points: &[Vec3],
    ) -> Result<EyelidConformationReceipt, AnatomyError> {
        conform_eyelids_to_globes_with_reference_vertices(
            self,
            reference_vertices,
            vertices,
            Some(guard),
            pinned_points,
        )
    }

    pub const fn vertex_count(&self) -> usize {
        self.vertex_count
    }
}

impl SafetyMetrics {
    const IDENTITY: Self = Self {
        minimum_edge_ratio: 1.0,
        maximum_edge_ratio: 1.0,
        minimum_area_ratio: 1.0,
        maximum_area_ratio: 1.0,
        minimum_orientation_cosine: 1.0,
    };

    fn safe(self) -> bool {
        self.minimum_edge_ratio >= MINIMUM_EDGE_RATIO
            && self.maximum_edge_ratio <= MAXIMUM_EDGE_RATIO
            && self.minimum_area_ratio >= MINIMUM_AREA_RATIO
            && self.maximum_area_ratio <= MAXIMUM_AREA_RATIO
            && self.minimum_orientation_cosine >= MINIMUM_ORIENTATION_COSINE
    }
}

pub fn conform_eyelids_to_globes(
    reference: &DazGeometry,
    vertices: &mut [Vec3],
) -> Result<EyelidConformationReceipt, AnatomyError> {
    EyelidConformationPlan::build(reference)?.conform(reference, vertices)
}

fn conform_eyelids_to_globes_with_plan(
    plan: &EyelidConformationPlan,
    reference: &DazGeometry,
    vertices: &mut [Vec3],
    validate_topology: bool,
) -> Result<EyelidConformationReceipt, AnatomyError> {
    if reference.vertices.len() != plan.vertex_count {
        return Err(AnatomyError::VertexCountMismatch {
            base: plan.vertex_count,
            fitted: reference.vertices.len(),
        });
    }
    if validate_topology && plan.topology_fingerprint != eyelid_topology_fingerprint(reference) {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let reference_vertices = reference
        .vertices
        .iter()
        .copied()
        .map(Vec3::from)
        .collect::<Vec<_>>();
    conform_eyelids_to_globes_with_reference_vertices(
        plan,
        &reference_vertices,
        vertices,
        None,
        &[],
    )
}

fn conform_eyelids_to_globes_with_reference_vertices(
    plan: &EyelidConformationPlan,
    reference_vertices: &[Vec3],
    vertices: &mut [Vec3],
    canonical_guard: Option<EyelidCanonicalGuard<'_>>,
    pinned_points: &[Vec3],
) -> Result<EyelidConformationReceipt, AnatomyError> {
    if reference_vertices.len() != plan.vertex_count {
        return Err(AnatomyError::VertexCountMismatch {
            base: plan.vertex_count,
            fitted: reference_vertices.len(),
        });
    }
    if plan.vertex_count != vertices.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: plan.vertex_count,
            fitted: vertices.len(),
        });
    }
    for (index, vertex) in reference_vertices.iter().enumerate() {
        if !vertex.is_finite() {
            return Err(AnatomyError::NonFiniteVertex {
                name: "eyelid reference",
                index,
            });
        }
    }
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex.is_finite() {
            return Err(AnatomyError::NonFiniteVertex {
                name: "eyelid candidate",
                index,
            });
        }
    }

    let topology = &plan.topology;
    let protected_before = plan
        .protected
        .iter()
        .map(|&index| (index, vertices[index]))
        .collect::<Vec<_>>();

    let left_reference_sphere = fit_eye_sphere(
        "lEye",
        plan.left_globe.len(),
        &plan.left_eye_surface,
        reference_vertices,
    )?;
    let right_reference_sphere = fit_eye_sphere(
        "rEye",
        plan.right_globe.len(),
        &plan.right_eye_surface,
        reference_vertices,
    )?;
    let left_current_sphere = fit_eye_sphere(
        "lEye",
        plan.left_globe.len(),
        &plan.left_eye_surface,
        vertices,
    )?;
    let right_current_sphere = fit_eye_sphere(
        "rEye",
        plan.right_globe.len(),
        &plan.right_eye_surface,
        vertices,
    )?;
    let eye_midpoint = (left_reference_sphere.center + right_reference_sphere.center) * 0.5;

    let head = HeadContext {
        reference: reference_vertices,
        vertices,
        topology,
        protected: &plan.protected,
        pinned_points,
    };
    let left = build_side_plan(
        EyeSubject {
            side: EyeSide::Left,
            reference_sphere: left_reference_sphere,
            current_sphere: left_current_sphere,
            globe: &plan.left_globe,
            aperture: plan.left_aperture.clone(),
            eye_midpoint,
        },
        head,
    )?;
    let right = build_side_plan(
        EyeSubject {
            side: EyeSide::Right,
            reference_sphere: right_reference_sphere,
            current_sphere: right_current_sphere,
            globe: &plan.right_globe,
            aperture: plan.right_aperture.clone(),
            eye_midpoint,
        },
        head,
    )?;

    let mut displacement = left.displacement.clone();
    for (&index, &value) in &right.displacement {
        if displacement.insert(index, value).is_some() {
            return Err(AnatomyError::InvalidTransitionConfiguration);
        }
    }
    let requested_maximum = displacement
        .values()
        .copied()
        .map(Vec3::norm)
        .reduce(f64::max)
        .unwrap_or(0.0);
    if requested_maximum <= NOOP_TOLERANCE_CM {
        return Ok(make_receipt(
            &left,
            &right,
            vertices,
            vertices,
            ConformOutcome {
                accepted_scale: 1.0,
                line_search_steps: 0,
                exact_noop: true,
                protected_vertex_count: plan.protected.len(),
                protected_vertices_changed: false,
                metrics: SafetyMetrics::IDENTITY,
            },
        ));
    }

    let before = vertices.to_vec();
    let moved = displacement.keys().copied().collect::<BTreeSet<_>>();
    let compact_reference = plan.compact_safety.gather(reference_vertices);
    let compact_before = plan.compact_safety.gather(&before);
    let before_unsafe = deformation_safety_mask(
        &compact_reference,
        &compact_before,
        &plan.compact_safety.triangles,
        MINIMUM_ORIENTATION_COSINE,
        MINIMUM_AREA_RATIO,
        MAXIMUM_AREA_RATIO,
    )
    .map_err(|_| AnatomyError::InvalidTransitionConfiguration)?;

    let canonical_baseline = canonical_guard
        .map(|guard| {
            if guard.canonical_vertices.len() != plan.vertex_count {
                return Err(AnatomyError::VertexCountMismatch {
                    base: plan.vertex_count,
                    fitted: guard.canonical_vertices.len(),
                });
            }
            let compact_canonical = plan.compact_safety.gather(guard.canonical_vertices);
            let before_unsafe_canonical = deformation_safety_mask(
                &compact_canonical,
                &compact_before,
                &plan.compact_safety.triangles,
                guard.minimum_orientation_cosine,
                guard.minimum_area_ratio,
                guard.maximum_area_ratio,
            )
            .map_err(|_| AnatomyError::InvalidTransitionConfiguration)?;
            Ok((guard, compact_canonical, before_unsafe_canonical))
        })
        .transpose()?;
    let introduced_unsafe = |candidate: &[Vec3]| -> Result<BTreeSet<usize>, AnatomyError> {
        let mut unsafe_vertices = introduced_unsafe_moved_vertices(
            candidate,
            topology,
            &plan.compact_safety,
            &compact_reference,
            &before_unsafe,
            &moved,
            SafetyLimits {
                minimum_orientation_cosine: MINIMUM_ORIENTATION_COSINE,
                minimum_area_ratio: MINIMUM_AREA_RATIO,
                maximum_area_ratio: MAXIMUM_AREA_RATIO,
            },
        )?;
        if let Some((guard, compact_canonical, before_unsafe_canonical)) = &canonical_baseline {
            unsafe_vertices.extend(introduced_unsafe_moved_vertices(
                candidate,
                topology,
                &plan.compact_safety,
                compact_canonical,
                before_unsafe_canonical,
                &moved,
                guard.limits(),
            )?);
        }
        Ok(unsafe_vertices)
    };
    let mut accepted = None;
    let mut local_scales = moved
        .iter()
        .copied()
        .map(|index| (index, 1.0_f64))
        .collect::<BTreeMap<_, _>>();
    for step in 0..=MAXIMUM_LOCAL_REPAIR_STEPS {
        let mut candidate = before.clone();
        for (&index, &delta) in &displacement {
            candidate[index] = before[index] + delta * local_scales[&index];
        }
        let metrics = safety_metrics(reference_vertices, &before, &candidate, topology, &moved)?;
        let unsafe_vertices = introduced_unsafe(&candidate)?;
        if metrics.safe() && unsafe_vertices.is_empty() {
            let minimum_scale = local_scales
                .values()
                .copied()
                .reduce(f64::min)
                .unwrap_or(1.0);
            accepted = Some((candidate, minimum_scale, step, metrics));
            break;
        }
        if unsafe_vertices.is_empty() || step == MAXIMUM_LOCAL_REPAIR_STEPS {
            break;
        }
        let mut attenuate = unsafe_vertices.clone();
        for index in unsafe_vertices {
            attenuate.extend(
                plan.topology.adjacency[index]
                    .iter()
                    .copied()
                    .filter(|neighbor| moved.contains(neighbor)),
            );
        }
        for index in attenuate {
            if let Some(scale) = local_scales.get_mut(&index) {
                *scale *= 0.5;
            }
        }
    }
    for step in 0..=MAXIMUM_LINE_SEARCH_STEPS {
        if accepted.is_some() {
            break;
        }
        let scale = 0.5_f64.powi(step as i32);
        let mut candidate = before.clone();
        for (&index, &delta) in &displacement {
            candidate[index] = before[index] + delta * scale;
        }
        let metrics = safety_metrics(reference_vertices, &before, &candidate, topology, &moved)?;
        let unsafe_vertices = introduced_unsafe(&candidate)?;
        if metrics.safe() && unsafe_vertices.is_empty() {
            accepted = Some((candidate, scale, step, metrics));
            break;
        }
    }
    let (candidate, accepted_scale, line_search_steps, metrics) = accepted.unwrap_or_else(|| {
        (
            before.clone(),
            0.0,
            MAXIMUM_LINE_SEARCH_STEPS + 1,
            SafetyMetrics::IDENTITY,
        )
    });
    vertices.copy_from_slice(&candidate);
    let protected_vertices_changed = protected_before
        .iter()
        .any(|&(index, point)| vertices[index] != point);
    if protected_vertices_changed {
        vertices.copy_from_slice(&before);
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    Ok(make_receipt(
        &left,
        &right,
        vertices,
        &before,
        ConformOutcome {
            accepted_scale,
            line_search_steps,
            exact_noop: false,
            protected_vertex_count: plan.protected.len(),
            protected_vertices_changed,
            metrics,
        },
    ))
}

#[derive(Clone, Copy, Debug)]
struct ConformOutcome {
    accepted_scale: f64,
    line_search_steps: usize,
    exact_noop: bool,
    protected_vertex_count: usize,
    protected_vertices_changed: bool,
    metrics: SafetyMetrics,
}

fn make_receipt(
    left: &SidePlan,
    right: &SidePlan,
    vertices: &[Vec3],
    before: &[Vec3],
    outcome: ConformOutcome,
) -> EyelidConformationReceipt {
    let ConformOutcome {
        accepted_scale,
        line_search_steps,
        exact_noop,
        protected_vertex_count,
        protected_vertices_changed,
        metrics,
    } = outcome;
    let side_receipt = |plan: &SidePlan| {
        let current_center = plan.sphere.center;
        let maximum_rest_error_after_cm = plan
            .aperture
            .iter()
            .map(|&index| {
                let rest = plan_rest_radius(plan, index);
                ((vertices[index] - current_center).norm() - rest).abs()
            })
            .reduce(f64::max)
            .unwrap_or(0.0);
        EyelidSideReceipt {
            side: plan.side,
            aperture_vertex_count: plan.aperture.len(),
            affected_skin_vertex_count: plan.displacement.len(),
            transition_ring_count: TRANSITION_RINGS,
            globe_vertex_count: plan.globe_vertex_count,
            globe_radius_cm: plan.sphere.radius,
            globe_fit_rms_cm: plan.sphere.rms,
            globe_translation: plan.translation,
            globe_scale: plan.globe_scale,
            maximum_globe_rigidity_error_cm: plan.rigidity_error,
            maximum_rest_error_before_cm: plan.maximum_rest_error_before,
            maximum_rest_error_after_cm,
            maximum_requested_displacement_cm: plan.maximum_requested_displacement,
            maximum_applied_displacement_cm: plan
                .displacement
                .keys()
                .map(|&index| (vertices[index] - before[index]).norm())
                .reduce(f64::max)
                .unwrap_or(0.0),
            mean_lower_strength: plan.mean_lower_strength,
            mean_upper_inner_strength: plan.mean_upper_inner_strength,
        }
    };
    EyelidConformationReceipt {
        left: side_receipt(left),
        right: side_receipt(right),
        accepted_scale,
        line_search_steps,
        exact_noop,
        protected_vertex_count,
        protected_vertices_changed,
        minimum_edge_ratio: metrics.minimum_edge_ratio,
        maximum_edge_ratio: metrics.maximum_edge_ratio,
        minimum_area_ratio: metrics.minimum_area_ratio,
        maximum_area_ratio: metrics.maximum_area_ratio,
        minimum_orientation_cosine: metrics.minimum_orientation_cosine,
    }
}

fn plan_rest_radius(plan: &SidePlan, index: usize) -> f64 {
    plan.rest_radii[&index]
}

fn eye_surface_vertices(
    reference: &DazGeometry,
    group: &'static str,
    globe: &[usize],
) -> Result<Vec<usize>, AnatomyError> {
    if globe.is_empty() {
        return Err(AnatomyError::MissingComponent { component: group });
    }
    let mut surface =
        material_and_polygon_group_vertices(reference, GLOBE_SURFACE_MATERIALS, &[group])?;
    if surface.len() < 4 {
        surface = globe.to_vec();
    }
    if surface.len() < 4 {
        return Err(AnatomyError::MissingComponent {
            component: "eye globe surface",
        });
    }
    Ok(surface)
}

fn fit_eye_sphere(
    group: &'static str,
    globe_vertex_count: usize,
    surface: &[usize],
    reference_vertices: &[Vec3],
) -> Result<Sphere, AnatomyError> {
    if globe_vertex_count == 0 {
        return Err(AnatomyError::MissingComponent { component: group });
    }
    if surface.len() < 4 {
        return Err(AnatomyError::MissingComponent {
            component: "eye globe surface",
        });
    }
    let mut normal = Matrix4::<f64>::zeros();
    let mut right = Vector4::<f64>::zeros();
    for &index in surface {
        let point = reference_vertices[index];
        let row = Vector4::new(2.0 * point.x, 2.0 * point.y, 2.0 * point.z, 1.0);
        let value = point.norm_squared();
        normal += row * row.transpose();
        right += row * value;
    }
    let solved = normal
        .lu()
        .solve(&right)
        .ok_or(AnatomyError::InvalidTransitionConfiguration)?;
    let center = Vec3::new(solved[0], solved[1], solved[2]);
    let radius_squared = solved[3] + center.norm_squared();
    if !center.is_finite() || !radius_squared.is_finite() || radius_squared <= 0.0 {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let radius = radius_squared.sqrt();
    let rms = (surface
        .iter()
        .map(|&index| ((reference_vertices[index] - center).norm() - radius).powi(2))
        .sum::<f64>()
        / surface.len() as f64)
        .sqrt();
    if !rms.is_finite() || rms > radius * 0.08 {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    Ok(Sphere {
        center,
        radius,
        rms,
    })
}

fn build_skin_topology(reference: &DazGeometry) -> Result<SkinTopology, AnatomyError> {
    let face_mask = reference.face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    let mut skin_mask = vec![false; reference.vertices.len()];
    let mut edge_counts = BTreeMap::<(usize, usize), usize>::new();
    let mut skin_edges = BTreeSet::<(usize, usize)>::new();
    let mut safety_edges = BTreeSet::<(usize, usize)>::new();
    let mut triangles = Vec::new();
    for (face, selected) in reference.faces.iter().zip(face_mask) {
        for offset in 0..face.len() {
            let a = face[offset] as usize;
            let b = face[(offset + 1) % face.len()] as usize;
            let edge = ordered_edge(a, b);
            safety_edges.insert(edge);
            if selected {
                *edge_counts.entry(edge).or_default() += 1;
                skin_edges.insert(edge);
            }
        }
        match *face.as_slice() {
            [a, b, c] => triangles.push([a as usize, b as usize, c as usize]),
            [a, b, c, d] => {
                triangles.push([a as usize, b as usize, c as usize]);
                triangles.push([a as usize, c as usize, d as usize]);
            }
            _ => return Err(AnatomyError::InvalidTransitionConfiguration),
        }
        if !selected {
            continue;
        }
        for &index in face {
            skin_mask[index as usize] = true;
        }
    }
    let mut adjacency = vec![BTreeSet::new(); reference.vertices.len()];
    for &(a, b) in &skin_edges {
        adjacency[a].insert(b);
        adjacency[b].insert(a);
    }
    let mut boundary_adjacency = vec![BTreeSet::new(); reference.vertices.len()];
    for (&(a, b), &count) in &edge_counts {
        if count == 1 {
            boundary_adjacency[a].insert(b);
            boundary_adjacency[b].insert(a);
        }
    }
    let boundary_loops = ordered_boundary_loops(&boundary_adjacency)?;
    Ok(SkinTopology {
        skin_mask,
        adjacency: adjacency
            .into_iter()
            .map(|neighbors| neighbors.into_iter().collect())
            .collect(),
        edges: safety_edges.into_iter().collect(),
        triangles,
        boundary_loops,
    })
}

fn ordered_edge(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

fn eyelid_topology_fingerprint(reference: &DazGeometry) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(PRIME);
    };
    mix(reference.vertices.len() as u64);
    mix(reference.faces.len() as u64);
    for face in &reference.faces {
        mix(face.len() as u64);
        for &index in face {
            mix(u64::from(index));
        }
    }
    for &index in &reference.polygon_group_indices {
        mix(u64::from(index));
    }
    for &index in &reference.material_group_indices {
        mix(u64::from(index));
    }
    for name in reference
        .polygon_groups
        .iter()
        .chain(&reference.material_groups)
    {
        mix(name.len() as u64);
        for byte in name.bytes() {
            mix(u64::from(byte));
        }
    }
    hash
}

fn ordered_boundary_loops(adjacency: &[BTreeSet<usize>]) -> Result<Vec<Vec<usize>>, AnatomyError> {
    let mut unvisited = adjacency
        .iter()
        .enumerate()
        .filter_map(|(index, neighbors)| (!neighbors.is_empty()).then_some(index))
        .collect::<BTreeSet<_>>();
    let mut loops = Vec::new();
    while let Some(&start) = unvisited.iter().next() {
        let mut component = BTreeSet::new();
        let mut stack = vec![start];
        unvisited.remove(&start);
        while let Some(index) = stack.pop() {
            component.insert(index);
            for &neighbor in &adjacency[index] {
                if unvisited.remove(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        if component.iter().any(|&index| adjacency[index].len() != 2) {
            continue;
        }
        let start = *component.iter().next().expect("component is nonempty");
        let mut ordered = vec![start];
        let mut previous = usize::MAX;
        let mut current = start;
        loop {
            let next = adjacency[current]
                .iter()
                .copied()
                .find(|&neighbor| neighbor != previous)
                .ok_or(AnatomyError::InvalidTransitionConfiguration)?;
            if next == start {
                break;
            }
            if ordered.contains(&next) {
                return Err(AnatomyError::InvalidTransitionConfiguration);
            }
            ordered.push(next);
            previous = current;
            current = next;
        }
        if ordered.len() == component.len() {
            loops.push(ordered);
        }
    }
    Ok(loops)
}

fn choose_eye_apertures(
    loops: &[Vec<usize>],
    reference: &[Vec3],
    left: Sphere,
    right: Sphere,
) -> Result<(Vec<usize>, Vec<usize>), AnatomyError> {
    let candidates = loops
        .iter()
        .filter(|loop_indices| loop_indices.len() == APERTURE_VERTEX_COUNT)
        .collect::<Vec<_>>();
    if candidates.len() < 2 {
        return Err(AnatomyError::MissingComponent {
            component: "21-vertex eye apertures",
        });
    }
    let score = |indices: &[usize], sphere: Sphere| {
        indices
            .iter()
            .map(|&index| ((reference[index] - sphere.center).norm() - sphere.radius).abs())
            .sum::<f64>()
            / indices.len() as f64
    };
    let mut best = None;
    for (left_index, left_loop) in candidates.iter().enumerate() {
        for (right_index, right_loop) in candidates.iter().enumerate() {
            if left_index == right_index {
                continue;
            }
            let candidate_score = score(left_loop, left) + score(right_loop, right);
            if best
                .as_ref()
                .is_none_or(|(best_score, _, _)| candidate_score < *best_score)
            {
                best = Some((candidate_score, (*left_loop).clone(), (*right_loop).clone()));
            }
        }
    }
    best.map(|(_, left, right)| (left, right))
        .ok_or(AnatomyError::InvalidTransitionConfiguration)
}

struct EyeSubject<'a> {
    side: EyeSide,
    reference_sphere: Sphere,
    current_sphere: Sphere,
    globe: &'a [usize],
    aperture: Vec<usize>,
    eye_midpoint: Vec3,
}

#[derive(Clone, Copy)]
struct HeadContext<'a> {
    reference: &'a [Vec3],
    vertices: &'a [Vec3],
    topology: &'a SkinTopology,
    protected: &'a BTreeSet<usize>,

    pinned_points: &'a [Vec3],
}

fn pin_hold(point: Vec3, pinned_points: &[Vec3]) -> f64 {
    let nearest = pinned_points
        .iter()
        .map(|pin| (*pin - point).norm())
        .reduce(f64::min);
    let Some(distance) = nearest else {
        return 0.0;
    };
    if distance >= PIN_HOLD_RADIUS_CM {
        return 0.0;
    }
    let closeness = 1.0 - distance / PIN_HOLD_RADIUS_CM;
    closeness * closeness * (3.0 - 2.0 * closeness)
}

fn build_side_plan(eye: EyeSubject<'_>, head: HeadContext<'_>) -> Result<SidePlan, AnatomyError> {
    let EyeSubject {
        side,
        reference_sphere,
        current_sphere,
        globe,
        aperture,
        eye_midpoint,
    } = eye;
    let HeadContext {
        reference,
        vertices,
        topology,
        protected,
        pinned_points,
    } = head;
    if aperture
        .iter()
        .any(|&index| !topology.skin_mask[index] || protected.contains(&index))
    {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let globe_scale = current_sphere.radius / reference_sphere.radius;
    if !globe_scale.is_finite()
        || !(MINIMUM_GLOBE_SCALE..=MAXIMUM_GLOBE_SCALE).contains(&globe_scale)
    {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let translation = current_sphere.center - reference_sphere.center;
    let rigidity_error = globe
        .iter()
        .map(|&index| {
            let expected =
                current_sphere.center + (reference[index] - reference_sphere.center) * globe_scale;
            (vertices[index] - expected).norm()
        })
        .reduce(f64::max)
        .unwrap_or(0.0);
    if rigidity_error > MAXIMUM_GLOBE_RIGIDITY_ERROR_CM {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let current_center = current_sphere.center;
    let aperture_center = aperture
        .iter()
        .map(|&index| reference[index])
        .fold(Vec3::ZERO, |sum, point| sum + point)
        / aperture.len() as f64;
    let outward_raw = reference_sphere.center - eye_midpoint;
    let outward = outward_raw / outward_raw.norm().max(1.0e-12);
    let lower_extent = aperture
        .iter()
        .map(|&index| aperture_center.y - reference[index].y)
        .reduce(f64::max)
        .unwrap_or(1.0)
        .max(1.0e-6);
    let outer_extent = aperture
        .iter()
        .map(|&index| (reference[index] - aperture_center).dot(outward))
        .reduce(f64::max)
        .unwrap_or(1.0)
        .max(1.0e-6);

    let mut rest_radii = BTreeMap::new();
    let mut raw = Vec::with_capacity(aperture.len());
    let mut maximum_rest_error_before: f64 = 0.0;
    let mut lower_strengths = Vec::new();
    let mut upper_inner_strengths = Vec::new();
    for &index in &aperture {
        let rest_radius = (reference[index] - reference_sphere.center).norm() * globe_scale;
        rest_radii.insert(index, rest_radius);
        let current = vertices[index] - current_center;
        let current_radius = current.norm();
        if current_radius <= 1.0e-12 {
            return Err(AnatomyError::InvalidTransitionConfiguration);
        }
        let lower = ((aperture_center.y - reference[index].y) / lower_extent).clamp(0.0, 1.0);
        let outer =
            ((reference[index] - aperture_center).dot(outward) / outer_extent).clamp(0.0, 1.0);
        let strength = (0.55 + 0.30 * lower + 0.15 * outer).min(1.0);
        if lower >= 0.5 {
            lower_strengths.push(strength);
        }
        if lower <= 0.25 && outer <= 0.25 {
            upper_inner_strengths.push(strength);
        }
        let error = rest_radius - current_radius;
        maximum_rest_error_before = maximum_rest_error_before.max(error.abs());
        raw.push(clamp_length(
            current / current_radius * (error * strength),
            MAXIMUM_CORRECTION_CM,
        ));
    }
    for _ in 0..2 {
        raw = (0..raw.len())
            .map(|index| {
                raw[index] * 0.5
                    + raw[(index + raw.len() - 1) % raw.len()] * 0.25
                    + raw[(index + 1) % raw.len()] * 0.25
            })
            .collect();
    }
    let aperture_corrections = aperture
        .iter()
        .copied()
        .zip(raw.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let rings = skin_rings(&aperture, topology, protected)?;
    let sigma_squared = (current_sphere.radius * 0.5).max(0.25).powi(2);
    let mut displacement = BTreeMap::new();
    for (&index, &ring) in &rings {
        let correction = if ring == 0 {
            aperture_corrections[&index]
        } else {
            let mut weighted = Vec3::ZERO;
            let mut weight_sum = 0.0;
            for &seed in &aperture {
                let distance_squared = (reference[index] - reference[seed]).norm_squared();
                let weight = (-0.5 * distance_squared / sigma_squared).exp();
                weighted += aperture_corrections[&seed] * weight;
                weight_sum += weight;
            }
            let fade = (TRANSITION_RINGS + 1 - ring) as f64 / (TRANSITION_RINGS + 1) as f64;
            weighted / weight_sum.max(1.0e-20) * fade
        };

        let held = 1.0 - pin_hold(reference[index], pinned_points);
        displacement.insert(
            index,
            clamp_length(correction * held, MAXIMUM_CORRECTION_CM),
        );
    }
    let maximum_requested_displacement = displacement
        .values()
        .copied()
        .map(Vec3::norm)
        .reduce(f64::max)
        .unwrap_or(0.0);
    Ok(SidePlan {
        side,
        sphere: current_sphere,
        translation,
        globe_scale,
        rigidity_error,
        globe_vertex_count: globe.len(),
        aperture,
        rest_radii,
        displacement,
        maximum_rest_error_before,
        maximum_requested_displacement,
        mean_lower_strength: mean(&lower_strengths),
        mean_upper_inner_strength: mean(&upper_inner_strengths),
    })
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn clamp_length(value: Vec3, maximum: f64) -> Vec3 {
    let length = value.norm();
    if length > maximum {
        value * (maximum / length)
    } else {
        value
    }
}

fn skin_rings(
    aperture: &[usize],
    topology: &SkinTopology,
    protected: &BTreeSet<usize>,
) -> Result<BTreeMap<usize, usize>, AnatomyError> {
    let mut rings = aperture
        .iter()
        .copied()
        .map(|index| (index, 0))
        .collect::<BTreeMap<_, _>>();
    let mut frontier = aperture.iter().copied().collect::<BTreeSet<_>>();
    for ring in 1..=TRANSITION_RINGS {
        let mut next = BTreeSet::new();
        for &index in &frontier {
            for &neighbor in &topology.adjacency[index] {
                if topology.skin_mask[neighbor]
                    && !protected.contains(&neighbor)
                    && !rings.contains_key(&neighbor)
                {
                    next.insert(neighbor);
                }
            }
        }
        for &index in &next {
            rings.insert(index, ring);
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    if rings.len() == aperture.len() {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    Ok(rings)
}

fn safety_metrics(
    reference: &[Vec3],
    before: &[Vec3],
    candidate: &[Vec3],
    topology: &SkinTopology,
    moved: &BTreeSet<usize>,
) -> Result<SafetyMetrics, AnatomyError> {
    let mut metrics = SafetyMetrics::IDENTITY;
    for &(a, b) in &topology.edges {
        if !moved.contains(&a) && !moved.contains(&b) {
            continue;
        }
        let base = (before[a] - before[b]).norm();
        let fitted = (candidate[a] - candidate[b]).norm();
        if base <= 1.0e-12 || !fitted.is_finite() {
            return Err(AnatomyError::InvalidTransitionConfiguration);
        }
        let ratio = fitted / base;
        metrics.minimum_edge_ratio = metrics.minimum_edge_ratio.min(ratio);
        metrics.maximum_edge_ratio = metrics.maximum_edge_ratio.max(ratio);
    }
    for &[a, b, c] in &topology.triangles {
        if !moved.contains(&a) && !moved.contains(&b) && !moved.contains(&c) {
            continue;
        }
        let reference_normal = cross(reference[b] - reference[a], reference[c] - reference[a]);
        let candidate_normal = cross(candidate[b] - candidate[a], candidate[c] - candidate[a]);
        let reference_area = reference_normal.norm();
        let candidate_area = candidate_normal.norm();
        if reference_area <= 1.0e-12 || candidate_area <= 1.0e-12 {
            return Ok(SafetyMetrics {
                minimum_area_ratio: 0.0,
                minimum_orientation_cosine: -1.0,
                ..metrics
            });
        }
        let area_ratio = candidate_area / reference_area;
        let orientation =
            reference_normal.dot(candidate_normal) / (reference_area * candidate_area);
        metrics.minimum_area_ratio = metrics.minimum_area_ratio.min(area_ratio);
        metrics.maximum_area_ratio = metrics.maximum_area_ratio.max(area_ratio);
        metrics.minimum_orientation_cosine = metrics.minimum_orientation_cosine.min(orientation);
    }
    Ok(metrics)
}

fn introduced_unsafe_moved_vertices(
    candidate: &[Vec3],
    topology: &SkinTopology,
    compact: &CompactSafetyTopology,
    compact_reference: &[[f64; 3]],
    before_unsafe: &[bool],
    moved: &BTreeSet<usize>,
    limits: SafetyLimits,
) -> Result<BTreeSet<usize>, AnatomyError> {
    if before_unsafe.len() != topology.triangles.len()
        || compact.triangles.len() != topology.triangles.len()
    {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let candidate_arrays = compact.gather(candidate);
    let candidate_unsafe = deformation_safety_mask(
        compact_reference,
        &candidate_arrays,
        &compact.triangles,
        limits.minimum_orientation_cosine,
        limits.minimum_area_ratio,
        limits.maximum_area_ratio,
    )
    .map_err(|_| AnatomyError::InvalidTransitionConfiguration)?;
    let mut result = BTreeSet::new();
    for ((&[a, b, c], before_unsafe), candidate_unsafe) in topology
        .triangles
        .iter()
        .zip(before_unsafe.iter().copied())
        .zip(candidate_unsafe)
    {
        if candidate_unsafe && !before_unsafe {
            result.extend([a, b, c].into_iter().filter(|index| moved.contains(index)));
        }
    }
    Ok(result)
}

fn cross(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(
        left.y * right.z - left.z * right.y,
        left.z * right.x - left.x * right.z,
        left.x * right.y - left.y * right.x,
    )
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    struct Fixture {
        geometry: DazGeometry,
        left_aperture: Vec<usize>,
        right_aperture: Vec<usize>,
        globe_and_attachments: Vec<usize>,
    }

    struct Builder {
        vertices: Vec<[f64; 3]>,
        faces: Vec<Vec<u32>>,
        polygon_indices: Vec<u32>,
        material_indices: Vec<u32>,
    }

    impl Builder {
        fn face(&mut self, face: Vec<usize>, polygon: u32, material: u32) {
            self.faces
                .push(face.into_iter().map(|index| index as u32).collect());
            self.polygon_indices.push(polygon);
            self.material_indices.push(material);
        }

        fn skin_annulus(&mut self, center_x: f64) -> Vec<usize> {
            let mut rings = Vec::new();
            for ring in 0..=5 {
                let mut indices = Vec::new();
                for column in 0..APERTURE_VERTEX_COUNT {
                    let angle = TAU * column as f64 / APERTURE_VERTEX_COUNT as f64;
                    let width = 1.25 + ring as f64 * 0.28;
                    let height = 0.68 + ring as f64 * 0.18;
                    indices.push(self.vertices.len());
                    self.vertices.push([
                        center_x + angle.cos() * width,
                        10.0 + angle.sin() * height,
                        1.02 - ring as f64 * 0.035,
                    ]);
                }
                rings.push(indices);
            }
            for ring in 0..5 {
                for column in 0..APERTURE_VERTEX_COUNT {
                    let next = (column + 1) % APERTURE_VERTEX_COUNT;
                    self.face(
                        vec![
                            rings[ring][column],
                            rings[ring][next],
                            rings[ring + 1][next],
                            rings[ring + 1][column],
                        ],
                        0,
                        0,
                    );
                }
            }
            rings[0].clone()
        }

        fn globe(&mut self, center_x: f64, polygon: u32) -> Vec<usize> {
            let center = Vec3::new(center_x, 10.0, 0.0);
            let offsets = [
                Vec3::new(1.5, 0.0, 0.0),
                Vec3::new(-1.5, 0.0, 0.0),
                Vec3::new(0.0, 1.5, 0.0),
                Vec3::new(0.0, -1.5, 0.0),
                Vec3::new(0.0, 0.0, 1.5),
                Vec3::new(0.0, 0.0, -1.5),
            ];
            let indices = offsets
                .into_iter()
                .map(|offset| {
                    let index = self.vertices.len();
                    self.vertices.push((center + offset).to_array());
                    index
                })
                .collect::<Vec<_>>();
            for face in [
                [0, 2, 4],
                [2, 1, 4],
                [1, 3, 4],
                [3, 0, 4],
                [2, 0, 5],
                [1, 2, 5],
                [3, 1, 5],
                [0, 3, 5],
            ] {
                self.face(face.map(|corner| indices[corner]).to_vec(), polygon, 1);
            }
            indices
        }

        fn attachment(&mut self, center_x: f64) -> Vec<usize> {
            let start = self.vertices.len();
            self.vertices.extend([
                [center_x - 0.15, 9.85, 1.25],
                [center_x + 0.15, 9.85, 1.25],
                [center_x, 10.05, 1.27],
            ]);
            let indices = vec![start, start + 1, start + 2];
            self.face(indices.clone(), 0, 2);
            indices
        }
    }

    fn fixture() -> Fixture {
        let mut builder = Builder {
            vertices: Vec::new(),
            faces: Vec::new(),
            polygon_indices: Vec::new(),
            material_indices: Vec::new(),
        };
        let left_aperture = builder.skin_annulus(3.0);
        let right_aperture = builder.skin_annulus(-3.0);
        let left_globe = builder.globe(3.0, 1);
        let right_globe = builder.globe(-3.0, 2);
        let left_attachment = builder.attachment(3.0);
        let right_attachment = builder.attachment(-3.0);
        let globe_and_attachments = left_globe
            .into_iter()
            .chain(right_globe)
            .chain(left_attachment)
            .chain(right_attachment)
            .collect();
        let geometry = DazGeometry::new(
            "synthetic-eyelids".into(),
            builder.vertices,
            builder.faces,
            crate::formats::GroupTable {
                indices: builder.polygon_indices,
                names: vec!["head".into(), "lEye".into(), "rEye".into()],
            },
            crate::formats::GroupTable {
                indices: builder.material_indices,
                names: vec!["Face".into(), "Sclera".into(), "Tear".into()],
            },
            json!({}),
        )
        .unwrap();
        Fixture {
            geometry,
            left_aperture,
            right_aperture,
            globe_and_attachments,
        }
    }

    #[test]
    fn identity_is_bit_exact_and_discovers_both_twenty_one_vertex_apertures() {
        let fixture = fixture();
        let mut vertices = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let before = vertices.clone();
        let receipt = conform_eyelids_to_globes(&fixture.geometry, &mut vertices).unwrap();
        assert_eq!(vertices, before);
        assert!(receipt.exact_noop);
        assert_eq!(receipt.left.aperture_vertex_count, APERTURE_VERTEX_COUNT);
        assert_eq!(receipt.right.aperture_vertex_count, APERTURE_VERTEX_COUNT);
        assert_eq!(receipt.left.maximum_applied_displacement_cm, 0.0);
        assert_eq!(receipt.right.maximum_applied_displacement_cm, 0.0);
        assert!(!receipt.protected_vertices_changed);
    }

    #[test]
    fn conformation_reduces_rest_error_and_never_touches_globes_or_attachments() {
        let fixture = fixture();
        let reference = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut vertices = reference.clone();
        for (&index, center_x) in fixture
            .left_aperture
            .iter()
            .map(|index| (index, 3.0))
            .chain(fixture.right_aperture.iter().map(|index| (index, -3.0)))
        {
            let center = Vec3::new(center_x, 10.0, 0.0);
            let radial = vertices[index] - center;
            vertices[index] -= radial / radial.norm() * 0.24;
        }
        let protected_before = fixture
            .globe_and_attachments
            .iter()
            .map(|&index| (index, vertices[index]))
            .collect::<Vec<_>>();
        let receipt = conform_eyelids_to_globes(&fixture.geometry, &mut vertices).unwrap();
        assert!(!receipt.exact_noop);
        assert!(receipt.accepted_scale > 0.0);
        assert!(
            receipt.left.maximum_rest_error_after_cm < receipt.left.maximum_rest_error_before_cm
        );
        assert!(
            receipt.right.maximum_rest_error_after_cm < receipt.right.maximum_rest_error_before_cm
        );
        assert!(receipt.left.maximum_applied_displacement_cm <= MAXIMUM_CORRECTION_CM);
        assert!(receipt.right.maximum_applied_displacement_cm <= MAXIMUM_CORRECTION_CM);
        assert!(receipt.left.mean_lower_strength > receipt.left.mean_upper_inner_strength);
        assert!(receipt.right.mean_lower_strength > receipt.right.mean_upper_inner_strength);
        for (index, before) in protected_before {
            assert_eq!(vertices[index], before);
        }
        assert!(!receipt.protected_vertices_changed);
        assert!(receipt.minimum_edge_ratio >= MINIMUM_EDGE_RATIO);
        assert!(receipt.maximum_edge_ratio <= MAXIMUM_EDGE_RATIO);
        assert!(receipt.minimum_area_ratio >= MINIMUM_AREA_RATIO);
        assert!(receipt.maximum_area_ratio <= MAXIMUM_AREA_RATIO);
        assert!(receipt.minimum_orientation_cosine >= MINIMUM_ORIENTATION_COSINE);
    }

    #[test]
    fn canonical_guard_matches_the_unguarded_pass_when_baselines_agree() {
        let fixture = fixture();
        let plan = EyelidConformationPlan::build(&fixture.geometry).unwrap();
        let reference = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut displaced = reference.clone();
        for (&index, center_x) in fixture
            .left_aperture
            .iter()
            .map(|index| (index, 3.0))
            .chain(fixture.right_aperture.iter().map(|index| (index, -3.0)))
        {
            let center = Vec3::new(center_x, 10.0, 0.0);
            let radial = displaced[index] - center;
            displaced[index] -= radial / radial.norm() * 0.24;
        }

        let mut unguarded = displaced.clone();
        let unguarded_receipt = plan
            .conform_bound_vertices(&reference, &mut unguarded)
            .unwrap();
        let mut guarded = displaced.clone();
        let guarded_receipt = plan
            .conform_bound_vertices_with_canonical_guard(
                &reference,
                &mut guarded,
                EyelidCanonicalGuard {
                    canonical_vertices: &reference,
                    minimum_orientation_cosine: MINIMUM_ORIENTATION_COSINE,
                    minimum_area_ratio: MINIMUM_AREA_RATIO,
                    maximum_area_ratio: MAXIMUM_AREA_RATIO,
                },
            )
            .unwrap();
        assert_eq!(unguarded, guarded);
        assert_eq!(unguarded_receipt, guarded_receipt);
        assert!(guarded_receipt.accepted_scale > 0.0);
    }

    #[test]
    fn a_pinned_lid_keeps_what_the_fit_gave_it_and_the_other_eye_is_untouched() {
        let fixture = fixture();
        let plan = EyelidConformationPlan::build(&fixture.geometry).unwrap();
        let reference = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut displaced = reference.clone();
        for (&index, center_x) in fixture
            .left_aperture
            .iter()
            .map(|index| (index, 3.0))
            .chain(fixture.right_aperture.iter().map(|index| (index, -3.0)))
        {
            let center = Vec3::new(center_x, 10.0, 0.0);
            let radial = displaced[index] - center;
            displaced[index] -= radial / radial.norm() * 0.24;
        }
        let guard = || EyelidCanonicalGuard {
            canonical_vertices: &reference,
            minimum_orientation_cosine: MINIMUM_ORIENTATION_COSINE,
            minimum_area_ratio: MINIMUM_AREA_RATIO,
            maximum_area_ratio: MAXIMUM_AREA_RATIO,
        };

        let mut unpinned = displaced.clone();
        plan.conform_bound_vertices_with_canonical_guard(&reference, &mut unpinned, guard())
            .unwrap();

        let pinned_index = fixture.left_aperture[0];
        let mut pinned = displaced.clone();
        plan.conform_bound_vertices_holding_pins(
            &reference,
            &mut pinned,
            guard(),
            &[reference[pinned_index]],
        )
        .unwrap();

        assert!(
            (unpinned[pinned_index] - displaced[pinned_index]).norm() > 0.02,
            "the unpinned conformation should move this lid vertex at all"
        );
        assert_eq!(pinned[pinned_index], displaced[pinned_index]);

        let opposite = fixture.left_aperture[APERTURE_VERTEX_COUNT / 2];
        assert!(
            (reference[opposite] - reference[pinned_index]).norm() > PIN_HOLD_RADIUS_CM,
            "the fixture's aperture must be wider than a pin's reach"
        );
        assert_eq!(pinned[opposite], unpinned[opposite]);
        for &index in &fixture.right_aperture {
            assert_eq!(pinned[index], unpinned[index]);
        }
    }

    #[test]
    fn canonical_guard_fails_closed_when_any_movement_breaks_the_final_baseline() {
        let fixture = fixture();
        let plan = EyelidConformationPlan::build(&fixture.geometry).unwrap();
        let reference = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut displaced = reference.clone();
        for (&index, center_x) in fixture
            .left_aperture
            .iter()
            .map(|index| (index, 3.0))
            .chain(fixture.right_aperture.iter().map(|index| (index, -3.0)))
        {
            let center = Vec3::new(center_x, 10.0, 0.0);
            let radial = displaced[index] - center;
            displaced[index] -= radial / radial.norm() * 0.24;
        }
        let before = displaced.clone();

        let receipt = plan
            .conform_bound_vertices_with_canonical_guard(
                &reference,
                &mut displaced,
                EyelidCanonicalGuard {
                    canonical_vertices: &before,
                    minimum_orientation_cosine: 0.999_999,
                    minimum_area_ratio: 0.999_999,
                    maximum_area_ratio: 1.000_001,
                },
            )
            .unwrap();
        assert_eq!(displaced, before);
        assert_eq!(receipt.accepted_scale, 0.0);
        assert!(!receipt.protected_vertices_changed);
    }

    #[test]
    fn uniformly_scaled_globes_use_their_current_centers_and_radii() {
        let fixture = fixture();
        let reference = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut vertices = reference.clone();
        let scale = 1.08;
        let translation = Vec3::new(0.0, 0.04, 0.06);
        for &index in &fixture.globe_and_attachments {
            let center_x = if reference[index].x >= 0.0 { 3.0 } else { -3.0 };
            let center = Vec3::new(center_x, 10.0, 0.0);
            vertices[index] = center + (reference[index] - center) * scale + translation;
        }
        let protected_before = fixture
            .globe_and_attachments
            .iter()
            .map(|&index| (index, vertices[index]))
            .collect::<Vec<_>>();

        let receipt = conform_eyelids_to_globes(&fixture.geometry, &mut vertices).unwrap();

        assert!((receipt.left.globe_scale - scale).abs() < 1.0e-10);
        assert!((receipt.right.globe_scale - scale).abs() < 1.0e-10);
        assert!(receipt.left.maximum_globe_rigidity_error_cm < 1.0e-10);
        assert!(receipt.right.maximum_globe_rigidity_error_cm < 1.0e-10);
        assert!(
            receipt.left.maximum_rest_error_after_cm <= receipt.left.maximum_rest_error_before_cm
        );
        assert!(
            receipt.right.maximum_rest_error_after_cm <= receipt.right.maximum_rest_error_before_cm
        );
        for (index, before) in protected_before {
            assert_eq!(vertices[index], before);
        }
        assert!(!receipt.protected_vertices_changed);
    }

    #[test]
    fn cached_plan_and_full_safety_topology_produce_identical_corrections() {
        let fixture = fixture();
        let reference = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let mut filtered_vertices = reference.clone();
        for &index in fixture.left_aperture.iter().chain(&fixture.right_aperture) {
            filtered_vertices[index].z -= 0.18;
        }
        let mut full_vertices = filtered_vertices.clone();
        let filtered_plan = EyelidConformationPlan::build(&fixture.geometry).unwrap();
        let mut full_plan = filtered_plan.clone();
        full_plan.topology = build_skin_topology(&fixture.geometry).unwrap();
        full_plan.compact_safety =
            CompactSafetyTopology::build(&full_plan.topology.triangles).unwrap();
        let filtered_receipt = filtered_plan
            .conform(&fixture.geometry, &mut filtered_vertices)
            .unwrap();
        let full_receipt = full_plan
            .conform(&fixture.geometry, &mut full_vertices)
            .unwrap();
        assert_eq!(filtered_vertices, full_vertices);
        assert_eq!(filtered_receipt, full_receipt);
        assert!(filtered_plan.topology.triangles.len() <= full_plan.topology.triangles.len());
        assert!(filtered_plan.topology.edges.len() <= full_plan.topology.edges.len());
    }

    #[test]
    fn cached_plan_rejects_a_same_size_but_different_topology() {
        let fixture = fixture();
        let plan = EyelidConformationPlan::build(&fixture.geometry).unwrap();
        let mut changed = fixture.geometry.clone();
        changed.faces[0].swap(0, 1);
        let mut vertices = changed
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();

        assert_eq!(
            plan.conform(&changed, &mut vertices).unwrap_err(),
            AnatomyError::InvalidTransitionConfiguration
        );
    }

    #[test]
    fn requested_correction_is_capped_before_the_topology_line_search() {
        let fixture = fixture();
        let mut vertices = fixture
            .geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        for &index in fixture.left_aperture.iter().chain(&fixture.right_aperture) {
            vertices[index].z -= 2.0;
        }
        let receipt = conform_eyelids_to_globes(&fixture.geometry, &mut vertices).unwrap();
        assert!(receipt.left.maximum_requested_displacement_cm <= MAXIMUM_CORRECTION_CM);
        assert!(receipt.right.maximum_requested_displacement_cm <= MAXIMUM_CORRECTION_CM);
    }

    #[test]
    #[ignore = "requires the user's licensed local Genesis2Female.dsf"]
    fn canonical_g2_detects_apertures_and_is_bit_exact() {
        let path = std::env::var_os("VKIT_G2_DSF")
            .map(PathBuf::from)
            .expect("set VKIT_G2_DSF to the licensed Genesis2Female.dsf");
        let geometry = crate::formats::load_dsf_path(path, 0).unwrap();
        let mut vertices = geometry
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let before = vertices.clone();
        let receipt = conform_eyelids_to_globes(&geometry, &mut vertices).unwrap();
        assert_eq!(vertices, before);
        assert!(receipt.exact_noop);
        assert_eq!(receipt.left.aperture_vertex_count, APERTURE_VERTEX_COUNT);
        assert_eq!(receipt.right.aperture_vertex_count, APERTURE_VERTEX_COUNT);
        assert_eq!(receipt.left.globe_vertex_count, 571);
        assert_eq!(receipt.right.globe_vertex_count, 571);
        assert!(receipt.left.globe_fit_rms_cm < 0.02);
        assert!(receipt.right.globe_fit_rms_cm < 0.02);
    }

    #[test]
    #[ignore = "requires licensed G2 geometry and a private fitted OBJ"]
    fn fitted_g2_reduces_eyelid_rest_error_without_touching_anatomy() {
        let dsf_path = std::env::var_os("VKIT_G2_DSF")
            .map(PathBuf::from)
            .expect("set VKIT_G2_DSF");
        let output_path = std::env::var_os("VKIT_G2_OUTPUT")
            .map(PathBuf::from)
            .expect("set VKIT_G2_OUTPUT");
        let geometry = crate::formats::load_dsf_path(dsf_path, 0).unwrap();
        let output = crate::formats::load_ordered_obj(output_path).unwrap();
        assert_eq!(output.vertices.len(), geometry.vertices.len());
        let mut vertices = output
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        let protected = material_vertices(&geometry, EYE_ATTACHED_MATERIALS)
            .unwrap()
            .into_iter()
            .chain(polygon_group_vertices(&geometry, &["lEye", "rEye"]).unwrap())
            .map(|index| (index, vertices[index]))
            .collect::<Vec<_>>();
        let receipt = conform_eyelids_to_globes(&geometry, &mut vertices).unwrap();
        assert!(
            receipt.left.maximum_rest_error_after_cm
                <= receipt.left.maximum_rest_error_before_cm + 1.0e-12
        );
        assert!(
            receipt.right.maximum_rest_error_after_cm
                <= receipt.right.maximum_rest_error_before_cm + 1.0e-12
        );
        assert!(receipt.left.maximum_applied_displacement_cm <= MAXIMUM_CORRECTION_CM);
        assert!(receipt.right.maximum_applied_displacement_cm <= MAXIMUM_CORRECTION_CM);
        for (index, before) in protected {
            assert_eq!(vertices[index], before);
        }
    }
}
