use std::sync::Arc;

use vkit_core::{
    formats::{DazGeometry, Mesh},
    spatial::{SurfaceProjector, projector_for_mesh},
    vam::{
        HairGuideGeometry, HairLookPatch, HairOpticalSettings, HairPhysicsSettings, HairPreset,
        HairScalpGeometry, HairScalpMaterialSettings, HairSpreadSettings, HairWavinessSettings,
    },
};

use crate::skin_preview::SkinImage;
use glam::Vec3;

const MAX_PREVIEW_STRANDS: usize = 48_000;
const MAX_CHILDREN_PER_GUIDE_TRIANGLE: usize = 64;

const SUBPIXEL_STRAND_COVERAGE_GAIN: f32 = 1.0;

const MIN_RASTER_WIDTH_CM: f32 = 0.008;

const CANDIDATE_UNIT_SCALES: [f32; 2] = [1.0, 0.01];

const MAX_ROOT_RESIDUAL_RATIO: f32 = 0.012;

const ALIGNMENT_SAMPLE: usize = 64;

const SCALP_SURFACE_LIFT_CM: f32 = 0.2;

#[derive(Clone, Debug)]
pub struct HairPreview {
    pub preset_id: String,
    pub parts: Vec<HairPreviewPart>,

    pub body_capsules: Vec<HairBodyCapsule>,

    pub scalps: Vec<HairScalpPart>,

    pub skipped_parts: Vec<String>,
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

pub fn build_hair_preview(
    preset: &HairPreset,
    template: &DazGeometry,
    assets: &[HairPreviewAsset],
    scalps: &[(Arc<HairScalpGeometry>, HairScalpTextures)],
) -> Result<HairPreview, String> {
    if assets.len() > preset.parts.len() {
        return Err("hair part asset count does not match the selected preset".to_owned());
    }
    if assets.is_empty() && scalps.is_empty() {
        return Err("hair preset has no parts this preview can build".to_owned());
    }
    let mesh = template
        .to_ordered_obj(None)
        .map_err(|error| error.to_string())?
        .triangulated()
        .map_err(|error| error.to_string())?;
    let projector = projector_for_mesh(&mesh).map_err(|error| format!("{error:?}"))?;
    let alignment = fit_alignment(assets, scalps, &mesh, &projector)?;

    let demand: Vec<usize> = assets
        .iter()
        .map(|asset| strand_demand(&asset.geometry, &asset.look))
        .collect();
    let total_demand: usize = demand.iter().sum::<usize>().max(1);
    let mut parts = Vec::with_capacity(assets.len());
    let mut spent = 0_usize;
    for (index, asset) in assets.iter().enumerate() {
        let share = ((MAX_PREVIEW_STRANDS as u64 * demand[index] as u64) / total_demand as u64)
            as usize
            + 1;
        let share = share.min(MAX_PREVIEW_STRANDS.saturating_sub(spent));
        if share == 0 {
            break;
        }
        let part = build_preview_part(asset, share, alignment, &mesh, &projector);
        spent = spent.saturating_add(part.strands.len());
        parts.push(part);
    }
    let fallback_root_color = darkest_root_color(&parts);
    let caps = scalps
        .iter()
        .map(|(scalp, textures)| {
            build_scalp_anchors(scalp, alignment, &mesh, &projector).map(|anchors| {
                let mut material = textures.material;
                if !textures.authored_material {
                    material.diffuse_color = fallback_root_color;
                }
                HairScalpPart {
                    anchors: Arc::new(anchors),
                    triangles: Arc::new(scalp.triangles.clone()),
                    uvs: Arc::new(scalp.uvs.clone()),
                    diffuse: textures.diffuse.clone(),
                    specular: textures.specular.clone(),
                    gloss: textures.gloss.clone(),
                    normal: textures.normal.clone(),
                    alpha: textures.alpha.clone(),
                    material,
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if caps.is_empty() && parts.iter().all(|part| part.strands.is_empty()) {
        return Err("selected hair preset has no renderable geometry".to_owned());
    }
    Ok(HairPreview {
        preset_id: preset.stable_id.clone(),
        parts,
        scalps: caps,
        skipped_parts: Vec::new(),
        body_capsules: body_capsules_from_template(template),
    })
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

    let mut guides = Vec::with_capacity(geometry.guides.len());
    let mut guide_map = vec![None; geometry.guides.len()];
    for (geometry_index, guide) in geometry.guides.iter().enumerate() {
        let points = styled_points(&guide.points_cm, look);
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
        guides.push(HairPreviewGuide {
            binding,
            local_points,
            painted_rigidity,
        });
    }

    let mut strands = Vec::with_capacity(limit.min(strand_demand(geometry, look)));
    for guide_index in guide_map.iter().flatten().copied() {
        if strands.len() >= limit {
            break;
        }
        let guide = &guides[guide_index as usize];
        strands.push(HairPreviewStrand {
            point_count: guide.local_points.len() as u32,
            source: HairStrandSource::Guide(guide_index),
        });
    }

    let children = render_children(look);
    let mut valid_triangles = Vec::with_capacity(geometry.guide_triangles.len());
    for (triangle_index, triangle) in geometry.guide_triangles.iter().enumerate() {
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
            valid_triangles.push((triangle_index, [a, b, c], point_count as u32));
        }
    }

    let virtual_children = valid_triangles.len().saturating_mul(children);
    let sampled_children = limit.saturating_sub(strands.len()).min(virtual_children);
    for sample in 0..sampled_children {
        let virtual_index = (((sample as u64 * 2 + 1) * virtual_children as u64)
            / (sampled_children as u64 * 2)) as usize;
        let triangle_slot = virtual_index / children;
        let child = virtual_index % children;
        let (triangle_index, guides, point_count) = valid_triangles[triangle_slot];
        strands.push(HairPreviewStrand {
            point_count,
            source: HairStrandSource::Interpolated {
                guides,
                barycentric: child_barycentric(triangle_index, child),
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
            let along = index as f32 / last;
            let standoff = root_standoff + (tip_standoff - root_standoff) * along;
            let lifted = lift_off_surface(alignment.apply(point), standoff, mesh, projector);
            let delta = lifted - root;
            [delta.dot(tangent), delta.dot(bitangent), delta.dot(normal)]
        })
        .collect();
    Some((binding, local_points))
}

fn strand_demand(geometry: &HairGuideGeometry, look: &HairLookPatch) -> usize {
    let children = render_children(look);
    geometry.guides.len() + geometry.guide_triangles.len() * children
}

fn render_children(look: &HairLookPatch) -> usize {
    look.hair_multiplier
        .unwrap_or(16)
        .saturating_sub(1)
        .min(MAX_CHILDREN_PER_GUIDE_TRIANGLE as u32) as usize
}

fn preview_strand_width(
    authored_width_m: f32,
    alignment: HairAlignment,
    shader_coverage: f32,
) -> f32 {
    let authored_cm = authored_width_m.clamp(0.000_005, 0.005) * 100.0;
    authored_cm.max(MIN_RASTER_WIDTH_CM)
        * alignment.scale
        * SUBPIXEL_STRAND_COVERAGE_GAIN
        * shader_coverage
}

fn darkest_root_color(parts: &[HairPreviewPart]) -> [f32; 3] {
    parts
        .iter()
        .map(|part| part.root_color)
        .min_by(|left, right| {
            let luminance =
                |color: [f32; 3]| color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722;
            luminance(*left).total_cmp(&luminance(*right))
        })
        .unwrap_or([0.035, 0.018, 0.012])
}

fn has_authored_scalp_texture(look: &HairLookPatch) -> bool {
    look.scalp_diffuse.is_some()
        || look.scalp_specular.is_some()
        || look.scalp_gloss.is_some()
        || look.scalp_normal.is_some()
        || look.scalp_alpha.is_some()
}

pub(crate) fn has_authored_scalp_material(look: &HairLookPatch) -> bool {
    has_authored_scalp_texture(look)
        || look.scalp_diffuse_color.is_some()
        || look.scalp_specular_color.is_some()
        || look.scalp_specular_intensity.is_some()
        || look.scalp_glossiness.is_some()
        || look.scalp_specular_fresnel.is_some()
        || look.scalp_alpha_adjust.is_some()
}

fn styled_points(points: &[[f32; 3]], look: &HairLookPatch) -> Vec<[f32; 3]> {
    let Some(root) = points.first().copied() else {
        return Vec::new();
    };
    let root = Vec3::from_array(root);
    let last = points.len().saturating_sub(1).max(1) as f32;
    points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let t = index as f32 / last;
            let scale = length_scale(look, t);
            (root + (Vec3::from_array(*point) - root) * scale).to_array()
        })
        .collect()
}

fn length_scale(look: &HairLookPatch, t: f32) -> f32 {
    let values = look
        .length
        .map(|value| value.unwrap_or(1.0).clamp(0.1, 3.0));
    if t <= 0.5 {
        values[0] + (values[1] - values[0]) * t * 2.0
    } else {
        values[1] + (values[2] - values[1]) * (t - 0.5) * 2.0
    }
}

fn child_barycentric(triangle: usize, child: usize) -> [f32; 3] {
    let seed = (triangle as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add((child as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9));
    let u = (((seed >> 16) & 0xffff) as f32 + 0.5) / 65_536.0;
    let v = (((seed >> 40) & 0xffff) as f32 + 0.5) / 65_536.0;
    let root_u = u.sqrt();
    [1.0 - root_u, root_u * (1.0 - v), root_u * v]
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

fn fit_alignment(
    assets: &[HairPreviewAsset],
    scalps: &[(Arc<HairScalpGeometry>, HairScalpTextures)],
    mesh: &Mesh,
    projector: &SurfaceProjector,
) -> Result<HairAlignment, String> {
    let total = assets
        .iter()
        .map(|asset| asset.geometry.guides.len())
        .sum::<usize>();
    let mut roots: Vec<[f32; 3]> = assets
        .iter()
        .flat_map(|asset| asset.geometry.guides.iter())
        .filter_map(|guide| guide.points_cm.first().copied())
        .step_by(total.div_ceil(ALIGNMENT_SAMPLE).max(1))
        .collect();
    if roots.is_empty() {
        let total = scalps
            .iter()
            .map(|(scalp, _)| scalp.vertices_cm.len())
            .sum::<usize>();
        roots = scalps
            .iter()
            .flat_map(|(scalp, _)| scalp.vertices_cm.iter().copied())
            .step_by(total.div_ceil(ALIGNMENT_SAMPLE).max(1))
            .collect();
    }
    if roots.is_empty() {
        return Err("hair asset has no geometry to align".to_owned());
    }
    let extent = mesh_extent(mesh);
    let mut best: Option<(f32, HairAlignment)> = None;
    for scale in CANDIDATE_UNIT_SCALES {
        for mirror_x in [false, true] {
            let alignment = HairAlignment { scale, mirror_x };
            let mut distances = roots
                .iter()
                .filter_map(|root| {
                    let point = alignment.apply(*root);
                    projector
                        .project([f64::from(point.x), f64::from(point.y), f64::from(point.z)])
                        .ok()
                        .map(|hit| hit.distance_squared.max(0.0).sqrt() as f32)
                })
                .collect::<Vec<_>>();
            if distances.is_empty() {
                continue;
            }
            distances.sort_by(f32::total_cmp);
            let median = distances[distances.len() / 2];
            if best.is_none_or(|(best_median, _)| median < best_median) {
                best = Some((median, alignment));
            }
        }
    }
    let Some((median, alignment)) = best else {
        return Err("hair roots could not be projected onto the G2 template".to_owned());
    };
    if median > extent * MAX_ROOT_RESIDUAL_RATIO {
        return Err(format!(
            "hair roots sit {median:.3} from the scalp, which no unit or handedness explains"
        ));
    }
    Ok(alignment)
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

fn mesh_extent(mesh: &Mesh) -> f32 {
    let mut minimum = [f64::MAX; 3];
    let mut maximum = [f64::MIN; 3];
    for vertex in &mesh.vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex[axis]);
            maximum[axis] = maximum[axis].max(vertex[axis]);
        }
    }
    (0..3)
        .map(|axis| (maximum[axis] - minimum[axis]) as f32)
        .fold(0.0_f32, f32::max)
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
    use vkit_core::vam::HairGuide;

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

    fn guides_in_centimetres(mirror_x: bool) -> Vec<HairPreviewAsset> {
        let ring = 12;
        let guides = (0..ring)
            .map(|step| {
                let angle = std::f64::consts::TAU * step as f64 / ring as f64;
                let x = (angle.cos() * 8.0) as f32;
                HairGuide {
                    scalp_index: step as u32,
                    points_cm: vec![
                        [
                            if mirror_x { -x } else { x },
                            174.0,
                            (angle.sin() * 8.0) as f32,
                        ],
                        [
                            if mirror_x { -x } else { x },
                            180.0,
                            (angle.sin() * 8.0) as f32,
                        ],
                    ],
                    rigidity: vec![1.0; 2],
                }
            })
            .collect();
        vec![HairPreviewAsset {
            geometry: Arc::new(HairGuideGeometry {
                provider_name: "test".to_owned(),
                segments: 2,
                segment_length_cm: 6.0,
                scalp_vertex_count: ring,
                guides,
                guide_triangles: Vec::new(),
                root_map: Vec::new(),
                nearby_joints: Vec::new(),
            }),
            look: HairLookPatch::default(),
            physics: HairPhysicsSettings::default(),
        }]
    }

    #[test]
    fn alignment_recovers_the_template_unit_without_being_told() {
        for (unit, expected) in [(1.0_f64, 0.01_f32), (100.0, 1.0)] {
            let mesh = scalp_mesh(unit);
            let projector = projector_for_mesh(&mesh).unwrap();
            let assets = guides_in_centimetres(false);
            let alignment = fit_alignment(&assets, &[], &mesh, &projector).unwrap();
            assert!(
                (alignment.scale - expected).abs() < f32::EPSILON,
                "template unit {unit}: got {}",
                alignment.scale
            );
            assert!(!alignment.mirror_x);
        }
    }

    #[test]
    fn alignment_recovers_a_mirrored_x_axis() {
        let mesh = Mesh::new(
            vec![
                [0.02, 1.72, -0.05],
                [0.14, 1.72, -0.05],
                [0.08, 1.72, 0.05],
                [-0.2, 0.0, 0.0],
                [0.2, 0.0, 0.0],
                [0.0, 0.0, 0.2],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap();
        let projector = projector_for_mesh(&mesh).unwrap();
        let root_on_the_right = |mirror: bool| {
            let x = if mirror { -8.0 } else { 8.0 };
            vec![HairPreviewAsset {
                geometry: Arc::new(HairGuideGeometry {
                    provider_name: "test".to_owned(),
                    segments: 2,
                    segment_length_cm: 6.0,
                    scalp_vertex_count: 1,
                    guides: vec![HairGuide {
                        scalp_index: 0,
                        points_cm: vec![[x, 173.0, 0.0], [x, 180.0, 0.0]],
                        rigidity: vec![1.0; 2],
                    }],
                    guide_triangles: Vec::new(),
                    root_map: Vec::new(),
                    nearby_joints: Vec::new(),
                }),
                look: HairLookPatch::default(),
                physics: HairPhysicsSettings::default(),
            }]
        };
        let straight = fit_alignment(&root_on_the_right(false), &[], &mesh, &projector).unwrap();
        assert!(!straight.mirror_x);
        let mirrored = fit_alignment(&root_on_the_right(true), &[], &mesh, &projector).unwrap();
        assert!(mirrored.mirror_x);
    }

    fn dome_cap(unit: f64, inward_winding: bool) -> HairScalpGeometry {
        let ring = 12;
        let mut vertices = vec![[0.0_f32, (1.8 * unit) as f32, 0.0]];
        for step in 0..ring {
            let angle = std::f64::consts::TAU * step as f64 / ring as f64;
            vertices.push([
                (angle.cos() * 0.09 * unit) as f32,
                (1.72 * unit) as f32,
                (angle.sin() * 0.09 * unit) as f32,
            ]);
        }
        let triangles = (0..ring)
            .map(|step| {
                let next = (step % ring + 1) as u32;
                let following = ((step + 1) % ring + 1) as u32;
                if inward_winding {
                    [0, following, next]
                } else {
                    [0, next, following]
                }
            })
            .collect();
        HairScalpGeometry {
            materials: vec!["Material".to_owned()],
            vertices_cm: vertices,
            uvs: Vec::new(),
            triangles,
        }
    }

    #[test]
    fn a_scalp_cap_rides_on_the_head_and_stands_off_its_skin() {
        let mesh = scalp_mesh(1.0);
        let projector = projector_for_mesh(&mesh).unwrap();
        for inward in [false, true] {
            let cap = Arc::new(dome_cap(100.0, inward));
            let alignment = fit_alignment(
                &[],
                &[(Arc::clone(&cap), HairScalpTextures::default())],
                &mesh,
                &projector,
            )
            .unwrap();
            let anchors = build_scalp_anchors(&cap, alignment, &mesh, &projector).unwrap();
            assert_eq!(anchors.len(), cap.vertices_cm.len());
            for anchor in &anchors {
                assert!(
                    anchor.normal_offset > 0.0,
                    "inward_winding={inward}: cap sits inside the head"
                );
                let sum: f32 = anchor.barycentric.iter().sum();
                assert!((sum - 1.0).abs() < 1.0e-3, "{:?}", anchor.barycentric);
            }
        }
    }

    #[test]
    fn alignment_refuses_hair_that_belongs_to_no_scalp() {
        let mesh = scalp_mesh(1.0);
        let projector = projector_for_mesh(&mesh).unwrap();
        let mut assets = guides_in_centimetres(false);
        let stray = Arc::make_mut(&mut assets[0].geometry);
        for guide in &mut stray.guides {
            for point in &mut guide.points_cm {
                point[1] += 500.0;
            }
        }
        assert!(fit_alignment(&assets, &[], &mesh, &projector).is_err());
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
    fn roots_bind_to_the_surface_under_them_not_to_a_vam_scalp_slot() {
        let mesh = scalp_mesh(1.0);
        let projector = projector_for_mesh(&mesh).unwrap();
        let assets = guides_in_centimetres(false);
        let alignment = fit_alignment(&assets, &[], &mesh, &projector).unwrap();
        for guide in &assets[0].geometry.guides {
            let root = alignment.apply(guide.points_cm[0]);
            let binding = bind_root(root, &mesh, &projector).unwrap();
            let [a, b, c] = triangle_points(&mesh, binding.triangle).unwrap();
            let normal = Vec3::from_array(binding.base_normal);
            let rebuilt = a * binding.barycentric[0]
                + b * binding.barycentric[1]
                + c * binding.barycentric[2]
                + normal * binding.normal_offset;
            assert!(
                (rebuilt - root).length() < 0.002,
                "root {root:?} rebuilt as {rebuilt:?}"
            );
        }
    }

    #[test]
    fn dense_children_reference_guides_without_copying_simulation_particles() {
        let mesh = scalp_mesh(1.0);
        let projector = projector_for_mesh(&mesh).unwrap();
        let mut assets = guides_in_centimetres(false);
        let guide_count = {
            let geometry = Arc::make_mut(&mut assets[0].geometry);
            geometry.guide_triangles = vec![[0, 1, 2]];
            geometry.guides.len()
        };

        assets[0].look.hair_multiplier = Some(5);
        let alignment = fit_alignment(&assets, &[], &mesh, &projector).unwrap();
        let part = build_preview_part(&assets[0], 64, alignment, &mesh, &projector);

        assert_eq!(part.guides.len(), guide_count);
        assert_eq!(part.strands.len(), guide_count + 4);
        assert_eq!(
            part.strands
                .iter()
                .filter(|strand| matches!(strand.source, HairStrandSource::Interpolated { .. }))
                .count(),
            4
        );
        assert!(part.strands.iter().all(|strand| strand.point_count == 2));
    }

    #[test]
    fn detail_multiplies_the_strands_while_density_tessellates_them() {
        let geometry = HairGuideGeometry {
            provider_name: "test".to_owned(),
            segments: 2,
            segment_length_cm: 1.0,
            scalp_vertex_count: 3,
            guides: Vec::new(),
            guide_triangles: vec![[0, 1, 2]],
            root_map: Vec::new(),
            nearby_joints: Vec::new(),
        };

        let mut look = HairLookPatch {
            curve_density: Some(50),
            hair_multiplier: Some(3),
            ..Default::default()
        };
        assert_eq!(strand_demand(&geometry, &look), 2);
        look.hair_multiplier = Some(30);
        assert_eq!(strand_demand(&geometry, &look), 29);
        look.curve_density = Some(8);
        assert_eq!(
            strand_demand(&geometry, &look),
            29,
            "density is tessellation along a strand, not a count of strands"
        );
    }

    #[test]
    fn physical_vam_width_keeps_the_established_subpixel_coverage() {
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

        assert!((centimetre_width - 0.008).abs() < 1.0e-6);
        assert!((metre_width - 0.00008).abs() < 1.0e-8);
        assert!((centimetre_width / 100.0 - metre_width).abs() < 1.0e-8);
    }
}
