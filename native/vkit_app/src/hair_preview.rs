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
use rayon::prelude::*;

const MAX_CHILDREN_PER_GUIDE_TRIANGLE: usize = 64;

/// `DAZSkinWrap` puts every wrapped vertex at
///
/// ```text
/// skin + n · ( max(baked, moveToSurfaceOffset) + surfaceOffset
///              + surfaceNormalWrapNormalDot · additionalThicknessMultiplier )
/// ```
///
/// `baked` is the standoff the cap was authored with, measured against the
/// figure it was authored on. The other three are constants, and they are the
/// reason a cap does not lie flat on the skin: together they are 1.3 mm before
/// the authored standoff is added at all.
const SCALP_SURFACE_OFFSET_CM: f32 = 0.03;

// `moveToSurfaceOffset` and `additionalThicknessMultiplier` are in the wrap's
// formula and are NOT in the total these caps come out with. Ledger 4.8
// measured every provider through this same code path against the bundle's own
// neutral base, and the answer was `baked + surfaceOffset` and nothing else:
// Udane 2.4 mm baked and 2.7 mm total, Leyton 3.1 and 3.4, Soleil and Omri 2.1
// and 2.4, Krayon 4.1 and 4.4, PantyRegion ~0 and 0.3. A floor under `baked`
// would have lifted PantyRegion off the skin and it is flush; the thickness
// term would have added a millimetre to all seven and none of them carries it.

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

/// One guide after its root has been projected onto the head: where it is
/// bound, where its points sit in the bed's space, and what rigidity was
/// painted on it.
type BoundGuide = (HairRootBinding, Vec<[f32; 3]>, Vec<f32>);

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
    // Binding a guide projects its root onto the head, and every guide's
    // projection is independent of every other's. They were done one at a time,
    // and a stroke rebuilds the whole part each time the throttle lets one
    // through, so this loop ran hundreds of projections in the middle of a
    // frame that was already drawing.
    //
    // Two passes rather than one: the map runs in parallel, and the fold that
    // follows is sequential so `guides` and `guide_map` come out in exactly the
    // order the single loop produced. The indices are the wire format between
    // guides, triangles and the barycentric table — reordering them silently
    // repaints the whole part.
    let bound: Vec<Option<BoundGuide>> = geometry
        .guides
        .par_iter()
        .map(|guide| {
            let (binding, local_points) = bind_preview_points(
                guide.points_cm.clone(),
                alignment,
                root_standoff,
                tip_standoff,
                mesh,
                projector,
            )?;
            if local_points.len() < 2 {
                return None;
            }
            // Empty means the file painted none, which is not the same as
            // painting every point solid.
            let painted_rigidity: Vec<f32> = if guide.rigidity.is_empty() {
                Vec::new()
            } else {
                (0..local_points.len())
                    .map(|index| guide.rigidity.get(index).copied().unwrap_or(1.0))
                    .collect()
            };
            Some((binding, local_points, painted_rigidity))
        })
        .collect();
    for (geometry_index, bound) in bound.into_iter().enumerate() {
        let Some((binding, local_points, painted_rigidity)) = bound else {
            continue;
        };
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
    if demand > limit {
        // A style thinned to fit is a style the person is not seeing. Say so
        // rather than let the preview quietly disagree with the game.
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Info,
            "hair",
            "preview_thinned",
            &format!(
                "{demand} strands wanted, {limit} drawn ({} guide triangles x {children} children)",
                valid_triangles.len()
            ),
        );
    }
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
        width: preview_strand_width(look.width_m.unwrap_or(0.0001), alignment),
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

/// `_StandWidth = width x WorldScale`. There is no floor of any kind in the
/// game, and the range is the one HairSimControl registers.
///
/// The floor that used to stand here was in world space, applied before any
/// projection, so no zoom could undo it: 205 of the 1140 installed sims author
/// a width under it and were drawn coarser than the game draws them, the
/// thinnest by a factor of nearly ten. The legibility of a sub-pixel strand is
/// the rasterizer's business, not the authored width's.
fn preview_strand_width(authored_width_m: f32, alignment: HairAlignment) -> f32 {
    authored_width_m.clamp(0.0, 0.001) * 100.0 * alignment.scale
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

/// Bind a cap to a head the way `DAZSkinWrap` does.
///
/// The standoff has two halves and they answer different questions. `baked` is
/// how far the cap was authored to stand off the figure it was authored on —
/// a property of the CAP, the same whatever look is loaded — and it is measured
/// against `neutral`. The rest are the wrap's own constants. What must never
/// enter it is the distance from the cap to the head currently loaded: that is
/// the departure between two shapes, and baking it made the wrapped cap
/// reproduce the authored skull instead of the one under it, standing off the
/// skin at the forehead and the back of the head by exactly how far the look
/// had moved.
fn build_scalp_anchors(
    scalp: &HairScalpGeometry,
    alignment: HairAlignment,
    mesh: &Mesh,
    projector: &SurfaceProjector,
    neutral: Option<(&Mesh, &SurfaceProjector)>,
) -> Result<Vec<ScalpAnchor>, String> {
    let scale = alignment.scale;
    let mut anchors = Vec::with_capacity(scalp.vertices_cm.len());
    let mut measured: Vec<f32> = Vec::with_capacity(scalp.vertices_cm.len());
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
        let baked = authored_standoff(placed, neutral).unwrap_or(0.0);
        measured.push(baked);
        anchors.push(ScalpAnchor {
            triangle,
            barycentric: [
                hit.barycentric[0] as f32,
                hit.barycentric[1] as f32,
                hit.barycentric[2] as f32,
            ],
            normal_offset: (baked + SCALP_SURFACE_OFFSET_CM) * scale,
        });
    }
    if !measured.is_empty() {
        let mut sorted = measured.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let at = |fraction: f32| sorted[((sorted.len() - 1) as f32 * fraction) as usize];
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Info,
            "hair",
            "scalp_standoff",
            &format!(
                "n={}; baked p10={:.4} p50={:.4} p90={:.4} max={:.4} cm;                  total p50={:.4} cm; neutral={}",
                sorted.len(),
                at(0.10),
                at(0.50),
                at(0.90),
                sorted[sorted.len() - 1],
                (at(0.50) + SCALP_SURFACE_OFFSET_CM) * scale,
                neutral.is_some(),
            ),
        );
    }
    Ok(anchors)
}

/// How far this cap vertex stands off the figure it was authored on.
///
/// Returns `None` before a figure is loaded, which is the case `surfaceOffset`
/// alone has to cover.
fn authored_standoff(placed: Vec3, neutral: Option<(&Mesh, &SurfaceProjector)>) -> Option<f32> {
    let (mesh, projector) = neutral?;
    let hit = projector
        .project([
            f64::from(placed.x),
            f64::from(placed.y),
            f64::from(placed.z),
        ])
        .ok()?;
    let triangle = *mesh.triangles.get(hit.primitive_id as usize)?;
    let normal = triangle_normal(mesh, triangle)?;
    let surface = Vec3::new(
        hit.point[0] as f32,
        hit.point[1] as f32,
        hit.point[2] as f32,
    );
    // Outside the skin only: a cap vertex that lands inside carries no standoff
    // of its own, which is what PantyRegion measures as.
    Some((placed - surface).dot(normal).max(0.0))
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
        // A width from the thin end of the library, well under the floor that
        // used to rewrite it.
        let width_m = 4.95768e-5;
        let centimetre_width = preview_strand_width(width_m, centimetre_template);
        let metre_width = preview_strand_width(width_m, metre_template);

        assert!(
            (centimetre_width - width_m * 100.0).abs() < 1.0e-9,
            "the authored width is carried through unchanged: {centimetre_width}",
        );
        assert!((centimetre_width / 100.0 - metre_width).abs() < 1.0e-9);

        let library_median = preview_strand_width(0.0001, centimetre_template);
        assert!(
            (library_median - 0.01).abs() < 1.0e-6,
            "0.0001 m is 0.01 cm and nothing more: {library_median}",
        );
        // The thinnest hair in the library, drawn nearly ten times too wide by
        // the floor this test used to pin.
        let thinnest = preview_strand_width(8.19e-6, centimetre_template);
        assert!(
            (thinnest - 8.19e-4).abs() < 1.0e-9,
            "the thinnest shipped strand is still floored: {thinnest}",
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

/// The figure as it was before a look, as something a cap can be measured
/// against.
fn neutral_surface(template: &DazGeometry) -> Option<(Mesh, SurfaceProjector)> {
    let vertices: Vec<[f64; 3]> = template.vertices.clone();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for face in &template.faces {
        for corner in 1..face.len().saturating_sub(1) {
            triangles.push([face[0], face[corner], face[corner + 1]]);
        }
    }
    if triangles.is_empty() {
        return None;
    }
    let mesh = Mesh::new(vertices, triangles).ok()?;
    let projector = projector_for_mesh(&mesh).ok()?;
    Some((mesh, projector))
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
        bed.neutral
            .as_ref()
            .map(|(mesh, projector)| (mesh, projector)),
    )
}

pub struct HeadBed {
    pub mesh: Mesh,
    pub projector: SurfaceProjector,
    pub body_capsules: Vec<HairBodyCapsule>,

    /// The figure before any look was applied, which is what the scalp caps
    /// were authored against. Only the standoff is read from it; where a cap
    /// vertex BINDS is decided entirely by `mesh`.
    pub neutral: Option<(Mesh, SurfaceProjector)>,

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
        let neutral = neutral_surface(template);
        Ok(Self {
            body_capsules: body_capsules_from_template(template),
            neutral,
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
    // The game never draws a guide: every strand on screen is one of the
    // `hairMultiplier` tessellation children of a complete triangle, and a
    // strand belonging to no complete triangle is physics only (ledger 1).
    // Drawing them put the longest, least gathered strands in the scene on top
    // of the rest — a guide takes length1 whole and sits on guide 0, so it gets
    // no spread toward the triangle centre. Those were the wiry flyaways.
    // `build_preview_part` still falls back to guides when a part has no
    // complete triangle at all, so a part being planted is not invisible.
    let part = build_preview_part(&asset, limit, alignment, mesh, projector, false);
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

#[cfg(test)]
mod guide_visibility_tests {
    /// Ledger 1, the first absolute law: the game draws no guides. Every strand
    /// on screen is a tessellation child of a complete triangle, and a strand
    /// in no complete triangle is physics only.
    ///
    /// A drawn guide is the worst offender it could be: `render_segments` gives
    /// it a length barycentric of `[1, 0, 0]`, so it takes `length1` whole while
    /// its neighbours take their tier, and it sits exactly on guide 0, so the
    /// spread never gathers it toward the triangle centre.
    #[test]
    fn the_authoring_preview_asks_for_children_only() {
        let source = include_str!("hair_preview.rs");
        let call = source
            .lines()
            .find(|line| line.contains("let part = build_preview_part(&asset,"))
            .expect("the authoring preview builds a part");
        assert!(
            call.trim_end().ends_with("projector, false);"),
            "the authoring preview asks for guides to be drawn: {call}",
        );
    }

    #[test]
    fn a_part_with_no_complete_triangle_still_draws_something() {
        // The one case where guides may stand in: nothing else exists to draw,
        // and a part being planted must not be invisible.
        let show_guides = false;
        let no_triangles: Vec<u32> = Vec::new();
        let draw_guides = show_guides || no_triangles.is_empty();
        assert!(draw_guides);

        let some_triangles = [0_u32];
        let draw_guides = show_guides || some_triangles.is_empty();
        assert!(
            !draw_guides,
            "once triangles exist, only children are drawn"
        );
    }
}

#[cfg(test)]
mod scalp_wrap_tests {
    use super::*;

    /// A ball of triangles, optionally squashed along one axis, standing in for
    /// a head whose look has been changed under a cap authored on another one.
    fn ball(radius: f32, squash: [f32; 3], rings: usize) -> Mesh {
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let columns = rings * 2;
        for ring in 0..=rings {
            let polar = std::f32::consts::PI * ring as f32 / rings as f32;
            for step in 0..columns {
                let azimuth = std::f32::consts::TAU * step as f32 / columns as f32;
                let point = [
                    radius * polar.sin() * azimuth.cos() * squash[0],
                    radius * polar.cos() * squash[1],
                    radius * polar.sin() * azimuth.sin() * squash[2],
                ];
                vertices.push([
                    f64::from(point[0]),
                    f64::from(point[1]),
                    f64::from(point[2]),
                ]);
            }
        }
        for ring in 0..rings {
            for step in 0..columns {
                let next = (step + 1) % columns;
                let a = (ring * columns + step) as u32;
                let b = (ring * columns + next) as u32;
                let c = ((ring + 1) * columns + step) as u32;
                let d = ((ring + 1) * columns + next) as u32;
                triangles.push([a, d, c]);
                triangles.push([a, b, d]);
            }
        }
        Mesh {
            vertices,
            triangles,
        }
    }

    /// The cap the bundle authored: a patch of the neutral ball's own surface.
    fn cap_on(mesh: &Mesh, take: usize) -> HairScalpGeometry {
        let vertices_cm: Vec<[f32; 3]> = mesh
            .vertices
            .iter()
            .take(take)
            .map(|point| [point[0] as f32, point[1] as f32, point[2] as f32])
            .collect();
        let triangles = (2..vertices_cm.len())
            .map(|corner| [0, (corner - 1) as u32, corner as u32])
            .collect();
        HairScalpGeometry {
            materials: Vec::new(),
            uvs: vec![[0.0, 0.0]; vertices_cm.len()],
            vertices_cm,
            triangles,
        }
    }

    /// The standoff a cap authored flush against the skin gets: `surfaceOffset`
    /// and nothing else. Ledger 4.8 measured PantyRegion, which IS flush, at
    /// exactly this.
    const FLUSH_STANDOFF_CM: f32 = SCALP_SURFACE_OFFSET_CM;

    fn hug_distances(cap: &HairScalpGeometry, head: &Mesh, neutral: &Mesh) -> Vec<f32> {
        let projector = projector_for_mesh(head).expect("a head projects");
        let neutral_projector = projector_for_mesh(neutral).expect("a figure projects");
        let anchors = build_scalp_anchors(
            cap,
            HairAlignment {
                scale: 1.0,
                mirror_x: false,
            },
            head,
            &projector,
            Some((neutral, &neutral_projector)),
        )
        .expect("the cap wraps");
        anchors
            .iter()
            .map(|anchor| {
                let [a, b, c] = triangle_points(head, anchor.triangle).expect("a triangle");
                let on_surface = a * anchor.barycentric[0]
                    + b * anchor.barycentric[1]
                    + c * anchor.barycentric[2];
                let placed = anchored_test_position(head, anchor);
                (placed - on_surface).length()
            })
            .collect()
    }

    fn anchored_test_position(head: &Mesh, anchor: &ScalpAnchor) -> Vec3 {
        let [a, b, c] = triangle_points(head, anchor.triangle).expect("a triangle");
        let normal = (b - a).cross(c - a).normalize();
        a * anchor.barycentric[0]
            + b * anchor.barycentric[1]
            + c * anchor.barycentric[2]
            + normal * anchor.normal_offset
    }

    /// The invariant: a wrapped cap stands the SAME distance off every head it
    /// is wrapped to. The distance is the cap's own authored standoff plus the
    /// wrap's constants; the shape of the head decides where it binds and
    /// nothing else.
    #[test]
    fn a_cap_stands_the_same_distance_off_every_head() {
        let neutral = ball(10.0, [1.0, 1.0, 1.0], 12);
        let cap = cap_on(&neutral, 60);

        for (label, squash) in [
            ("the head it was authored on", [1.0_f32, 1.0, 1.0]),
            ("a flatter skull", [1.0, 1.0, 0.72]),
            ("a taller one", [0.88, 1.25, 0.95]),
            ("a wider one", [1.3, 0.95, 1.05]),
        ] {
            let head = ball(10.0, squash, 12);
            let worst = hug_distances(&cap, &head, &neutral)
                .into_iter()
                .fold(0.0_f32, |held, value| {
                    held.max((value - FLUSH_STANDOFF_CM).abs())
                });
            assert!(
                worst < 1.0e-3,
                "{label}: a cap vertex sat {worst} cm off its standoff, so the wrap is \
                 reading the head it was given instead of the figure the cap was authored on",
            );
        }
    }

    /// And a cap that WAS authored to stand off keeps that standoff, on every
    /// head. Dropping it is what put the cap inside the skin.
    #[test]
    fn a_cap_authored_clear_of_the_skin_keeps_its_own_gap() {
        let neutral = ball(10.0, [1.0, 1.0, 1.0], 12);
        let flush = cap_on(&neutral, 60);
        // The same cap, half a centimetre further out along its own normals —
        // which for a ball is straight out from the centre.
        let raised = HairScalpGeometry {
            vertices_cm: flush
                .vertices_cm
                .iter()
                .map(|point| {
                    let out = Vec3::from_array(*point).normalize();
                    (Vec3::from_array(*point) + out * 0.5).to_array()
                })
                .collect(),
            ..flush.clone()
        };

        for squash in [[1.0_f32, 1.0, 1.0], [1.0, 1.0, 0.72], [1.3, 0.95, 1.05]] {
            let head = ball(10.0, squash, 12);
            let wanted = 0.5 + SCALP_SURFACE_OFFSET_CM;
            let worst = hug_distances(&raised, &head, &neutral)
                .into_iter()
                .fold(0.0_f32, |held, value| held.max((value - wanted).abs()));
            assert!(
                worst < 0.02,
                "the authored half centimetre came out {worst} cm wrong on {squash:?}",
            );
        }
    }

    /// And the failure it replaces, stated so nobody restores it: the distance
    /// between the authored cap and the loaded head is exactly the departure
    /// between the two shapes, which is what used to be baked in.
    #[test]
    fn the_departure_that_used_to_be_baked_is_real_and_large() {
        let neutral = ball(10.0, [1.0, 1.0, 1.0], 12);
        let cap = cap_on(&neutral, 60);
        let flattened = ball(10.0, [1.0, 1.0, 0.72], 12);
        let projector = projector_for_mesh(&flattened).expect("projects");

        let mut worst = 0.0_f32;
        for point in &cap.vertices_cm {
            let placed = Vec3::from_array(*point);
            let hit = projector
                .project([
                    f64::from(placed.x),
                    f64::from(placed.y),
                    f64::from(placed.z),
                ])
                .expect("a nearest point");
            let surface = Vec3::new(
                hit.point[0] as f32,
                hit.point[1] as f32,
                hit.point[2] as f32,
            );
            worst = worst.max((placed - surface).length());
        }
        assert!(
            worst > 1.0,
            "the test heads are too alike to show the defect: worst departure {worst}",
        );
    }
}

#[cfg(test)]
mod scalp_standoff_tests {
    use super::*;

    /// The numbers ledger 4.8 measured through this same code path, against the
    /// bundle's own neutral base. Every one of them is `baked + surfaceOffset`:
    /// a floor under `baked` would lift PantyRegion off a skin it is flush
    /// with, and the thickness term would add a millimetre to all seven.
    #[test]
    fn the_total_gap_is_the_authored_standoff_plus_one_constant() {
        for (provider, baked_mm, total_mm) in [
            ("UdaneScalp", 2.4_f32, 2.7_f32),
            ("LeytonScalp", 3.1, 3.4),
            ("SoleilScalp", 2.1, 2.4),
            ("OmriScalp", 2.1, 2.4),
            ("KrayonScalp", 4.1, 4.4),
            ("VictoriaElitePonytailHairScalp", 1.4, 1.7),
            ("PantyRegionScalp", 0.0, 0.3),
        ] {
            let computed = baked_mm + SCALP_SURFACE_OFFSET_CM * 10.0;
            assert!(
                (computed - total_mm).abs() < 0.05,
                "{provider}: {baked_mm} mm baked comes out {computed} mm, measured {total_mm}",
            );
        }
    }

    /// And the constant is the one the wrap calls `surfaceOffset`, 0.0003 m.
    #[test]
    fn the_constant_is_the_wraps_own_surface_offset() {
        assert!((SCALP_SURFACE_OFFSET_CM - 0.03).abs() < 1.0e-6);
    }
}
