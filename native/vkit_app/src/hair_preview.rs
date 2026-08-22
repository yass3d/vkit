use std::sync::Arc;

use vkit_core::{
    formats::{DazGeometry, Mesh},
    spatial::{SurfaceProjector, projector_for_mesh},
    vam::{
        HairGuideGeometry, HairLookPatch, HairOpticalSettings, HairPhysicsSettings,
        HairScalpGeometry, HairScalpMaterialSettings, HairSpreadSettings, HairWavinessSettings,
    },
};

use crate::skin_preview::SkinImage;
use glam::Vec3;

const MAX_CHILDREN_PER_GUIDE_TRIANGLE: usize = 64;

const MIN_RASTER_WIDTH_CM: f32 = 0.008;

const SCALP_SURFACE_LIFT_CM: f32 = 0.03;

#[derive(Clone, Debug)]
pub struct HairPreview {
    pub parts: Vec<HairPreviewPart>,

    pub body_capsules: Vec<HairBodyCapsule>,

    pub scalps: Vec<HairScalpPart>,
}

#[derive(Clone, Copy, Debug)]
pub struct ScalpAnchor {
    pub triangle: [u32; 3],
    pub barycentric: [f32; 3],

    pub normal_offset: f32,
}

#[derive(Clone, Debug)]
pub struct HairScalpPart {
    pub anchors: Arc<Vec<ScalpAnchor>>,
    pub triangles: Arc<Vec<[u32; 3]>>,
    pub uvs: Arc<Vec<[f32; 2]>>,
    pub diffuse: Option<Arc<SkinImage>>,
    pub specular: Option<Arc<SkinImage>>,
    pub gloss: Option<Arc<SkinImage>>,
    pub normal: Option<Arc<SkinImage>>,

    pub alpha: Option<Arc<SkinImage>>,
    pub material: HairScalpMaterialSettings,
}

#[derive(Clone, Copy, Debug)]
pub struct HairBodyCapsule {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct HairPreviewPart {
    pub guides: Arc<Vec<HairPreviewGuide>>,
    pub strands: Arc<Vec<HairPreviewStrand>>,
    pub root_color: [f32; 3],
    pub tip_color: [f32; 3],

    pub curve_density: u32,
    pub width: f32,
    pub metres_to_template: f32,
    pub optics: HairOpticalSettings,
    pub physics: HairPhysicsSettings,
    pub waviness: HairWavinessSettings,
    pub spread: HairSpreadSettings,

    pub strand_length_m: f32,

    pub nearby_joints: Vec<HairPreviewNearbyJoint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairPreviewNearbyJoint {
    pub a_guide: u32,
    pub a_point: u32,
    pub b_guide: u32,
    pub b_point: u32,
    pub elasticity: f32,
}

#[derive(Clone, Debug)]
pub struct HairPreviewGuide {
    pub binding: HairRootBinding,
    pub local_points: Vec<[f32; 3]>,
    pub painted_rigidity: Vec<f32>,
    pub curl_rand: [f32; 3],
    pub curl_phase: f32,
}

#[derive(Clone, Debug)]
pub struct HairPreviewStrand {
    pub point_count: u32,
    pub source: HairStrandSource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HairStrandSource {
    Guide(u32),
    Interpolated {
        guides: [u32; 3],
        barycentric: [f32; 3],
        length_barycentric: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug)]
pub struct HairRootBinding {
    pub triangle: [u32; 3],
    pub barycentric: [f32; 3],
    pub normal_offset: f32,
    pub base_tangent: [f32; 3],
    pub base_bitangent: [f32; 3],
    pub base_normal: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct HairPreviewAsset {
    pub geometry: Arc<HairGuideGeometry>,
    pub look: HairLookPatch,
    pub physics: HairPhysicsSettings,
}

fn body_capsules_from_template(template: &DazGeometry) -> Vec<HairBodyCapsule> {
    const SPANS: &[(&str, &str, &[&str])] = &[
        ("neck", "head", &["Neck"]),
        ("chest", "neck", &["Torso", "Nipples"]),
        ("abdomen2", "chest", &["Torso"]),
        ("lCollar", "lShldr", &["Shoulders", "Torso"]),
        ("rCollar", "rShldr", &["Shoulders", "Torso"]),
        ("lShldr", "lForeArm", &["Shoulders"]),
        ("rShldr", "rForeArm", &["Shoulders"]),
    ];
    let material_id = |name: &str| {
        template
            .material_groups
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
    };
    let mut capsules = Vec::new();
    for &(from, to, materials) in SPANS {
        let (Some(from), Some(to)) = (template.bone(from), template.bone(to)) else {
            continue;
        };
        let a = glam::Vec3::from_array(from.center_point.map(|v| v as f32));
        let b = glam::Vec3::from_array(to.center_point.map(|v| v as f32));
        let axis = b - a;
        let length_squared = axis.length_squared();
        if length_squared < 1.0e-6 {
            continue;
        }
        let wanted: Vec<usize> = materials
            .iter()
            .filter_map(|name| material_id(name))
            .collect();
        if wanted.is_empty() {
            continue;
        }

        let mut radial = Vec::new();
        let mut seen = vec![false; template.vertices.len()];
        for (face_index, face) in template.faces.iter().enumerate() {
            let material = template
                .material_group_indices
                .get(face_index)
                .map(|index| *index as usize);
            if !material.is_some_and(|index| wanted.contains(&index)) {
                continue;
            }
            for &vertex_id in face {
                let vertex_id = vertex_id as usize;
                if seen.get(vertex_id).copied().unwrap_or(true) {
                    continue;
                }
                seen[vertex_id] = true;
                let point = glam::Vec3::from_array(template.vertices[vertex_id].map(|v| v as f32));
                let t = (point - a).dot(axis) / length_squared;
                if !(0.05..=0.95).contains(&t) {
                    continue;
                }
                radial.push(point.distance(a + axis * t));
            }
        }
        if radial.len() < 50 {
            continue;
        }
        radial.sort_by(f32::total_cmp);
        let radius = radial[(radial.len() as f32 * 0.85) as usize % radial.len()];
        capsules.push(HairBodyCapsule {
            a: a.to_array(),
            b: b.to_array(),
            radius,
        });
    }
    capsules
}

fn build_preview_part(
    asset: &HairPreviewAsset,
    limit: usize,
    alignment: HairAlignment,
    mesh: &Mesh,
    projector: &SurfaceProjector,
    show_guides: bool,
) -> HairPreviewPart {
    let geometry = &asset.geometry;
    let look = &asset.look;
    let metres_to_template = alignment.scale * 100.0;
    let tip_standoff = look
        .collision_radius_m
        .unwrap_or(asset.physics.collision_radius_m)
        .max(0.0)
        * metres_to_template;
    let root_standoff = look
        .collision_radius_root_m
        .unwrap_or(asset.physics.collision_radius_root_m)
        .max(0.0)
        * metres_to_template;

    let strand_randoms = crate::unity_random::strand_randoms(geometry.guides.len());
    let mut guides = Vec::with_capacity(geometry.guides.len());
    let mut guide_map = vec![None; geometry.guides.len()];
    for (geometry_index, guide) in geometry.guides.iter().enumerate() {
        let points = guide.points_cm.clone();
        let Some((binding, local_points)) = bind_preview_points(
            points,
            alignment,
            root_standoff,
            tip_standoff,
            mesh,
            projector,
        ) else {
            continue;
        };
        if local_points.len() < 2 {
            continue;
        }
        let painted_rigidity = (0..local_points.len())
            .map(|index| guide.rigidity.get(index).copied().unwrap_or(1.0))
            .collect();
        guide_map[geometry_index] = Some(guides.len() as u32);
        let curl_rand = strand_randoms
            .get(geometry_index)
            .copied()
            .unwrap_or([0.5, 0.5, 0.5]);
        guides.push(HairPreviewGuide {
            binding,
            local_points,
            painted_rigidity,
            curl_rand,
            curl_phase: geometry_index as f32 + curl_rand[0],
        });
    }

    let children = render_children(look);
    let drawable: Vec<u32> = guide_map.iter().flatten().copied().collect();

    let mut valid_triangles = Vec::with_capacity(geometry.guide_triangles.len());
    for triangle in geometry.guide_triangles.iter() {
        let [Some(a), Some(b), Some(c)] =
            triangle.map(|index| guide_map.get(index as usize).copied().flatten())
        else {
            continue;
        };
        let point_count = guides[a as usize]
            .local_points
            .len()
            .min(guides[b as usize].local_points.len())
            .min(guides[c as usize].local_points.len());
        if point_count >= 2 {
            valid_triangles.push((triangle[0] as usize, [a, b, c], point_count as u32));
        }
    }

    let draw_guides = show_guides || valid_triangles.is_empty();
    let guide_demand = if draw_guides { drawable.len() } else { 0 };
    let demand = guide_demand + valid_triangles.len() * children;
    let guide_budget = if demand == 0 {
        0
    } else {
        ((limit as u64 * guide_demand as u64) / demand as u64) as usize
    }
    .clamp(guide_demand.min(1), guide_demand.max(1))
    .min(guide_demand);

    let mut strands = Vec::with_capacity(limit.min(demand));
    for slot in 0..guide_budget {
        let index = ((slot as u64 * drawable.len() as u64) / guide_budget.max(1) as u64) as usize;
        let guide_index = drawable[index.min(drawable.len() - 1)];
        let guide = &guides[guide_index as usize];
        strands.push(HairPreviewStrand {
            point_count: guide.local_points.len() as u32,
            source: HairStrandSource::Guide(guide_index),
        });
    }

    let barycentrics = if valid_triangles.is_empty() || children == 0 {
        Vec::new()
    } else {
        crate::unity_random::barycentric_table(geometry.guides.len())
    };
    let table = |strand: usize, slot: usize| -> [f32; 3] {
        barycentrics
            .get(strand * crate::unity_random::BARYCENTRICS_PER_STRAND + slot)
            .copied()
            .unwrap_or([1.0, 0.0, 0.0])
    };
    let virtual_children = valid_triangles.len().saturating_mul(children);
    let sampled_children = limit.saturating_sub(strands.len()).min(virtual_children);
    for sample in 0..sampled_children {
        let virtual_index = (((sample as u64 * 2 + 1) * virtual_children as u64)
            / (sampled_children as u64 * 2)) as usize;
        let triangle_slot = virtual_index / children;
        let child = virtual_index % children;
        let (root_strand, guides, point_count) = valid_triangles[triangle_slot];
        let slot = crate::unity_random::child_slot(child, children);
        strands.push(HairPreviewStrand {
            point_count,
            source: HairStrandSource::Interpolated {
                guides,
                barycentric: table(root_strand, slot),
                length_barycentric: table(0, slot),
            },
        });
    }

    let nearby_joints = geometry
        .nearby_joints
        .iter()
        .filter_map(|joint| {
            let a_guide = guide_map.get(joint.a[0] as usize).copied().flatten()?;
            let b_guide = guide_map.get(joint.b[0] as usize).copied().flatten()?;
            let a_points = guides[a_guide as usize].local_points.len() as u32;
            let b_points = guides[b_guide as usize].local_points.len() as u32;
            (joint.a[1] < a_points && joint.b[1] < b_points).then_some(HairPreviewNearbyJoint {
                a_guide,
                a_point: joint.a[1],
                b_guide,
                b_point: joint.b[1],
                elasticity: joint.elasticity.clamp(0.0, 1.0),
            })
        })
        .collect();

    let optics = look.optical_settings();
    HairPreviewPart {
        guides: Arc::new(guides),
        strands: Arc::new(strands),
        curve_density: look.curve_density.unwrap_or(8).max(1),
        root_color: look.root_color.unwrap_or([0.09, 0.035, 0.018]),
        tip_color: look
            .tip_color
            .or(look.root_color)
            .unwrap_or([0.16, 0.07, 0.03]),
        width: preview_strand_width(
            look.width_m.unwrap_or(0.0001),
            alignment,
            optics.shader_type.coverage_scale(),
        ),
        metres_to_template,
        optics,
        physics: asset.physics,
        waviness: look.waviness_settings(),
        spread: look.spread_settings(),
        strand_length_m: geometry.segment_length_cm.max(0.0) / 100.0
            * (geometry.segments.max(2) - 1) as f32,
        nearby_joints,
    }
}

fn bind_preview_points(
    points: Vec<[f32; 3]>,
    alignment: HairAlignment,
    root_standoff: f32,
    tip_standoff: f32,
    mesh: &Mesh,
    projector: &SurfaceProjector,
) -> Option<(HairRootBinding, Vec<[f32; 3]>)> {
    let root = alignment.apply(*points.first()?);
    let binding = bind_root(root, mesh, projector)?;
    let tangent = Vec3::from_array(binding.base_tangent);
    let bitangent = Vec3::from_array(binding.base_bitangent);
    let normal = Vec3::from_array(binding.base_normal);
    let last = points.len().saturating_sub(1).max(1) as f32;
    let local_points = points
        .into_iter()
        .enumerate()
        .map(|(index, point)| {
            let standoff = if index == 0 {
                0.0
            } else {
                let along = index as f32 / last;
                root_standoff + (tip_standoff - root_standoff) * along
            };
            let lifted = lift_off_surface(alignment.apply(point), standoff, mesh, projector);
            let delta = lifted - root;
            [delta.dot(tangent), delta.dot(bitangent), delta.dot(normal)]
        })
        .collect();
    Some((binding, local_points))
}

fn render_children(look: &HairLookPatch) -> usize {
    look.hair_multiplier
        .unwrap_or(16)
        .min(MAX_CHILDREN_PER_GUIDE_TRIANGLE as u32) as usize
}

fn preview_strand_width(
    authored_width_m: f32,
    alignment: HairAlignment,
    shader_coverage: f32,
) -> f32 {
    let authored_cm = authored_width_m.clamp(0.000_005, 0.005) * 100.0;
    authored_cm.max(MIN_RASTER_WIDTH_CM) * alignment.scale * shader_coverage
}

#[derive(Clone, Debug, Default)]
pub struct HairScalpTextures {
    pub diffuse: Option<Arc<SkinImage>>,
    pub specular: Option<Arc<SkinImage>>,
    pub gloss: Option<Arc<SkinImage>>,
    pub normal: Option<Arc<SkinImage>>,
    pub alpha: Option<Arc<SkinImage>>,
    pub material: HairScalpMaterialSettings,
    pub authored_material: bool,
}

#[derive(Clone, Copy, Debug)]
struct HairAlignment {
    scale: f32,
    mirror_x: bool,
}

impl HairAlignment {
    fn apply(self, point: [f32; 3]) -> Vec3 {
        let x = if self.mirror_x { -point[0] } else { point[0] };
        Vec3::new(x, point[1], point[2]) * self.scale
    }
}

fn build_scalp_anchors(
    scalp: &HairScalpGeometry,
    alignment: HairAlignment,
    mesh: &Mesh,
    projector: &SurfaceProjector,
) -> Result<Vec<ScalpAnchor>, String> {
    let lift = SCALP_SURFACE_LIFT_CM * alignment.scale;
    let mut anchors = Vec::with_capacity(scalp.vertices_cm.len());
    for point in &scalp.vertices_cm {
        let placed = alignment.apply(*point);
        let hit = projector
            .project([
                f64::from(placed.x),
                f64::from(placed.y),
                f64::from(placed.z),
            ])
            .map_err(|error| format!("{error:?}"))?;
        let triangle = *mesh
            .triangles
            .get(hit.primitive_id as usize)
            .ok_or("scalp anchor references a triangle outside the template")?;
        let surface = Vec3::new(
            hit.point[0] as f32,
            hit.point[1] as f32,
            hit.point[2] as f32,
        );

        let normal = triangle_normal(mesh, triangle).unwrap_or(Vec3::Y);
        anchors.push(ScalpAnchor {
            triangle,
            barycentric: [
                hit.barycentric[0] as f32,
                hit.barycentric[1] as f32,
                hit.barycentric[2] as f32,
            ],
            normal_offset: (placed - surface).dot(normal) + lift,
        });
    }
    Ok(anchors)
}

fn triangle_normal(mesh: &Mesh, triangle: [u32; 3]) -> Option<Vec3> {
    let [a, b, c] = triangle_points(mesh, triangle)?;
    (b - a).cross(c - a).try_normalize()
}

fn lift_off_surface(point: Vec3, standoff: f32, mesh: &Mesh, projector: &SurfaceProjector) -> Vec3 {
    if standoff <= 0.0 {
        return point;
    }
    let Ok(hit) = projector.project([f64::from(point.x), f64::from(point.y), f64::from(point.z)])
    else {
        return point;
    };
    let Some(normal) = mesh
        .triangles
        .get(hit.primitive_id as usize)
        .and_then(|triangle| triangle_normal(mesh, *triangle))
    else {
        return point;
    };
    let surface = Vec3::new(
        hit.point[0] as f32,
        hit.point[1] as f32,
        hit.point[2] as f32,
    );
    let signed = (point - surface).dot(normal);
    if signed >= standoff {
        point
    } else {
        point + normal * (standoff - signed)
    }
}

fn bind_root(root: Vec3, mesh: &Mesh, projector: &SurfaceProjector) -> Option<HairRootBinding> {
    let hit = projector
        .project([f64::from(root.x), f64::from(root.y), f64::from(root.z)])
        .ok()?;
    let triangle = *mesh.triangles.get(hit.primitive_id as usize)?;
    let [a, b, c] = triangle_points(mesh, triangle)?;
    let tangent = (b - a).try_normalize()?;
    let normal = (b - a).cross(c - a).try_normalize()?;
    let bitangent = normal.cross(tangent);
    let surface = Vec3::new(
        hit.point[0] as f32,
        hit.point[1] as f32,
        hit.point[2] as f32,
    );
    Some(HairRootBinding {
        triangle,
        barycentric: [
            hit.barycentric[0] as f32,
            hit.barycentric[1] as f32,
            hit.barycentric[2] as f32,
        ],
        normal_offset: (root - surface).dot(normal),
        base_tangent: tangent.to_array(),
        base_bitangent: bitangent.to_array(),
        base_normal: normal.to_array(),
    })
}

fn triangle_points(mesh: &Mesh, triangle: [u32; 3]) -> Option<[Vec3; 3]> {
    let [a, b, c] = triangle.map(|index| {
        mesh.vertices
            .get(index as usize)
            .map(|point| Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32))
    });
    Some([a?, b?, c?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalp_mesh(unit: f64) -> Mesh {
        let ring = 12;
        let mut vertices = vec![[0.0, 1.8 * unit, 0.0]];
        for step in 0..ring {
            let angle = std::f64::consts::TAU * step as f64 / ring as f64;
            vertices.push([
                angle.cos() * 0.09 * unit,
                1.72 * unit,
                angle.sin() * 0.09 * unit,
            ]);
        }
        let mut triangles = Vec::new();
        for step in 0..ring {
            let next = step % ring + 1;
            let following = (step + 1) % ring + 1;
            triangles.push([0, next as u32, following as u32]);
        }

        let base = vertices.len() as u32;
        vertices.push([-0.2 * unit, 0.0, 0.0]);
        vertices.push([0.2 * unit, 0.0, 0.0]);
        vertices.push([0.0, 0.0, 0.2 * unit]);
        triangles.push([base, base + 1, base + 2]);
        Mesh::new(vertices, triangles).unwrap()
    }

    #[test]
    fn the_root_point_is_never_lifted_off_the_cap() {
        let mesh = scalp_mesh(1.0);
        let projector = projector_for_mesh(&mesh).unwrap();
        let alignment = HairAlignment {
            scale: 1.0,
            mirror_x: false,
        };
        let points = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]];
        let (_, local) =
            bind_preview_points(points, alignment, 0.5, 0.9, &mesh, &projector).expect("bound");

        let root = local[0];
        assert!(
            root.iter().all(|axis| axis.abs() < 1.0e-4),
            "the root was pushed off its own anchor: {root:?}"
        );
        assert!(
            local[1][2].abs() > 1.0e-3 || local[2][2].abs() > 1.0e-3,
            "the standoff stopped applying to the rest of the strand"
        );
    }

    #[test]
    fn collision_standoff_lifts_penetrating_points_and_leaves_clear_ones() {
        let mesh = Mesh::new(
            vec![
                [-1.0, 1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 3, 2]],
        )
        .unwrap();
        let projector = projector_for_mesh(&mesh).unwrap();
        let standoff = 0.02;

        for depth in [-0.5_f32, -0.05, -0.01, 0.0, 0.005, 0.015] {
            let lifted = lift_off_surface(
                Vec3::new(0.2, 1.0 + depth, -0.3),
                standoff,
                &mesh,
                &projector,
            );
            let cleared = lifted.y - 1.0;
            assert!(
                (cleared - standoff).abs() < 1.0e-4,
                "depth {depth}: cleared to {cleared}, want {standoff}"
            );
        }

        let clear = Vec3::new(0.2, 1.5, -0.3);
        assert_eq!(lift_off_surface(clear, standoff, &mesh, &projector), clear);

        let inside = Vec3::new(0.2, 0.9, -0.3);
        assert_eq!(lift_off_surface(inside, 0.0, &mesh, &projector), inside);
    }

    #[test]
    fn the_authored_width_reaches_the_shader_unscaled() {
        let centimetre_template = HairAlignment {
            scale: 1.0,
            mirror_x: false,
        };
        let metre_template = HairAlignment {
            scale: 0.01,
            mirror_x: false,
        };
        let width_m = 4.95768e-5;
        let centimetre_width = preview_strand_width(width_m, centimetre_template, 1.0);
        let metre_width = preview_strand_width(width_m, metre_template, 1.0);

        assert!(
            (centimetre_width - 0.008).abs() < 1.0e-6,
            "the raster floor moved: {centimetre_width}",
        );
        assert!((metre_width - 0.00008).abs() < 1.0e-8);
        assert!((centimetre_width / 100.0 - metre_width).abs() < 1.0e-8);

        let library_median = preview_strand_width(0.0001, centimetre_template, 1.0);
        assert!(
            (library_median - 0.01).abs() < 1.0e-6,
            "0.0001 m is 0.01 cm and nothing more: {library_median}",
        );
    }
}

#[cfg(test)]
mod alignment_tests {

    #[test]
    fn guides_and_children_share_the_budget_instead_of_guides_taking_it_all() {
        let guides = 1_800_usize;
        let triangles = 3_400_usize;
        let children = 15_usize;
        let limit = 8_000_usize;

        let demand = guides + triangles * children;
        let guide_budget = ((limit as u64 * guides as u64) / demand as u64) as usize;
        assert!(
            guide_budget < guides,
            "the guides would still take everything"
        );
        assert!(
            limit - guide_budget > limit / 2,
            "children got {} of {limit}, which is not most of the hair",
            limit - guide_budget,
        );
        let last = ((guide_budget.saturating_sub(1) as u64 * guides as u64)
            / guide_budget.max(1) as u64) as usize;
        assert!(
            last > guides * 3 / 4,
            "the drawn guides stop at {last} of {guides}, leaving the rest bald",
        );
    }
}

pub struct AuthoringScalp {
    pub geometry: Arc<vkit_core::vam::HairScalpGeometry>,
    pub anchors: Arc<Vec<ScalpAnchor>>,
    pub textures: HairScalpTextures,
}

pub fn wrap_scalp_to_head(
    geometry: &vkit_core::vam::HairScalpGeometry,
    bed: &HeadBed,
) -> Result<Vec<ScalpAnchor>, String> {
    build_scalp_anchors(
        geometry,
        HairAlignment {
            scale: 1.0,
            mirror_x: false,
        },
        &bed.mesh,
        &bed.projector,
    )
}

pub struct HeadBed {
    pub mesh: Mesh,
    pub projector: SurfaceProjector,
    pub body_capsules: Vec<HairBodyCapsule>,

    pub generation: u64,
}

static HEAD_BED_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl HeadBed {
    /// Lay the bed on the head as it is now.
    ///
    /// The template is still what the body capsules are read from — they are a
    /// figure's proportions, not a face's — but everything hair is planted on,
    /// wrapped to and bound against comes from the mesh handed in, so a morph
    /// moves the scalp with it.
    pub fn build_on(template: &DazGeometry, mesh: Mesh) -> Result<Self, String> {
        let projector = projector_for_mesh(&mesh).map_err(|error| format!("{error:?}"))?;
        Ok(Self {
            body_capsules: body_capsules_from_template(template),
            mesh,
            projector,
            generation: HEAD_BED_GENERATION
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .wrapping_add(1),
        })
    }
}

pub fn authoring_hair_preview(
    guide_geometry: Arc<vkit_core::vam::HairGuideGeometry>,
    look: vkit_core::vam::HairLookPatch,
    physics: vkit_core::vam::HairPhysicsSettings,
    bed: &HeadBed,
    limit: usize,
    scalp: Option<AuthoringScalp>,
) -> Result<HairPreview, String> {
    let HeadBed {
        mesh,
        projector,
        body_capsules,
        ..
    } = bed;
    let asset = HairPreviewAsset {
        geometry: guide_geometry,
        look,
        physics,
    };
    let alignment = HairAlignment {
        scale: 1.0,
        mirror_x: false,
    };
    let part = build_preview_part(&asset, limit, alignment, mesh, projector, true);
    let scalps = match scalp {
        Some(AuthoringScalp {
            geometry,
            anchors,
            textures,
        }) => {
            let mut material = textures.material;
            if !textures.authored_material {
                material.diffuse_color = part.root_color;
            }
            vec![HairScalpPart {
                anchors,
                triangles: Arc::new(geometry.triangles.clone()),
                uvs: Arc::new(geometry.uvs.clone()),
                diffuse: textures.diffuse.clone(),
                specular: textures.specular.clone(),
                gloss: textures.gloss.clone(),
                normal: textures.normal.clone(),
                alpha: textures.alpha.clone(),
                material,
            }]
        }
        None => Vec::new(),
    };
    Ok(HairPreview {
        parts: vec![part],
        body_capsules: body_capsules.clone(),
        scalps,
    })
}
