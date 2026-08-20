use std::{collections::BTreeMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use egui_wgpu::{wgpu, winit::Painter as EguiPainter};
use glam::{Mat4, Vec3};
use thiserror::Error;
use vkit_core::{
    formats::Mesh,
    surface_smoothing::{SurfaceSmoothingScratch, SurfaceSmoothingTopology},
};

#[cfg(test)]
use crate::skin_preview::{SkinChannel, SkinUvOrientation};
use crate::{
    hair_renderer::{HairRenderResources, ScalpRenderResources},
    lighting::{LightingPreset, MAX_PUNCTUAL_LIGHTS},
    scene::SurfaceMesh,
};

mod mesh;
mod mip;
mod shaders;
mod skin;

use self::mesh::*;
pub use self::mesh::{MeshPaintCallback, RenderStyle};
pub(crate) use self::skin::SkinRenderResources;
pub use self::skin::{SkinPaintCallback, SkinVisibilityGroup, SkinVisibilityGroups};
#[cfg(test)]
use self::{shaders::*, skin::*};

pub const MSAA_SAMPLES: u32 = 4;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
pub const DEFAULT_LIGHT_YAW_RADIANS: f32 = 0.0;

#[cfg(test)]
const BASE_KEY_DIRECTION: Vec3 = Vec3::new(-0.42, 0.66, 0.61);
#[cfg(test)]
const BASE_FILL_DIRECTION: Vec3 = Vec3::new(0.67, 0.22, 0.31);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SceneUniform {
    view_projection: [f32; 16],
    model: [f32; 16],
    normal_matrix: [f32; 16],
    color: [f32; 4],
    eye: [f32; 4],
    lighting: [f32; 4],
    key_light: [f32; 4],
    fill_light: [f32; 4],
    environment_top: [f32; 4],
    environment_bottom: [f32; 4],
    grading: [f32; 4],
    punctual_meta: [f32; 4],

    punctual: [[f32; 4]; 12],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderDepthScope {
    #[default]
    Shared,
    ResetBeforeDraw,
}

impl RenderDepthScope {
    pub const fn resets_before_draw(self) -> bool {
        matches!(self, Self::ResetBeforeDraw)
    }
}

#[derive(Debug, Error)]
pub enum RendererInstallError {
    #[error("wgpu render state is unavailable after window creation")]
    MissingRenderState,
    #[error("mesh/skin shader or pipeline creation failed: {0}")]
    PipelineCreation(String),
}

pub fn install(painter: &EguiPainter) -> Result<(), RendererInstallError> {
    let render_state = painter
        .render_state()
        .ok_or(RendererInstallError::MissingRenderState)?;

    render_state
        .device
        .on_uncaptured_error(std::sync::Arc::new(|error: wgpu::Error| {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SEEN: AtomicU64 = AtomicU64::new(0);
            let seen = SEEN.fetch_add(1, Ordering::Relaxed);
            if seen < 8 || seen.is_multiple_of(512) {
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Error,
                    "renderer",
                    "wgpu_uncaptured_error",
                    &error.to_string(),
                );
            }
        }));

    let validation_scope = render_state
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let resources = MeshRenderResources::new(
        &render_state.device,
        render_state.target_format,
        MSAA_SAMPLES,
        DEPTH_FORMAT,
    );
    let skin_resources = SkinRenderResources::new(
        &render_state.device,
        render_state.target_format,
        MSAA_SAMPLES,
        DEPTH_FORMAT,
    );
    if let Some(error) = pollster::block_on(validation_scope.pop()) {
        return Err(RendererInstallError::PipelineCreation(error.to_string()));
    }

    let hair_scope = render_state
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let hair_resources = HairRenderResources::new(
        &render_state.device,
        render_state.target_format,
        MSAA_SAMPLES,
    );
    let scalp_resources = ScalpRenderResources::new(
        &render_state.device,
        &render_state.queue,
        render_state.target_format,
        MSAA_SAMPLES,
    );
    let hair_error = pollster::block_on(hair_scope.pop());

    let bloom_scope = render_state
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let bloom_resources = crate::bloom::BloomResources::new(
        &render_state.device,
        render_state.target_format,
        1280,
        720,
    );
    let bloom_error = pollster::block_on(bloom_scope.pop());

    let mut renderer = render_state.renderer.write();
    renderer.callback_resources.insert(resources);
    renderer.callback_resources.insert(skin_resources);
    if let Some(error) = bloom_error {
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Error,
            "renderer",
            "bloom_pipeline_failed",
            &error.to_string(),
        );
    } else {
        renderer.callback_resources.insert(bloom_resources);
    }
    if let Some(error) = hair_error {
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Error,
            "renderer",
            "hair_pipeline_unavailable",
            &error.to_string(),
        );
    } else {
        renderer.callback_resources.insert(hair_resources);
        renderer.callback_resources.insert(scalp_resources);
    }
    Ok(())
}

fn sanitized_light_yaw(value: f32) -> f32 {
    if value.is_finite() {
        value.rem_euclid(std::f32::consts::TAU)
    } else {
        DEFAULT_LIGHT_YAW_RADIANS
    }
}

fn normal_matrix(model: Mat4) -> Mat4 {
    let determinant = model.determinant();
    if !determinant.is_finite() || determinant.abs() <= 1.0e-20 {
        return Mat4::IDENTITY;
    }
    let normal_matrix = model.inverse().transpose();
    if normal_matrix
        .to_cols_array()
        .into_iter()
        .all(f32::is_finite)
    {
        normal_matrix
    } else {
        Mat4::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LightingUniformData {
    key_light: [f32; 4],
    fill_light: [f32; 4],
    environment_top: [f32; 4],
    environment_bottom: [f32; 4],
    punctual_meta: [f32; 4],
    punctual: [[f32; 4]; 12],
}

fn punctual_uniform_data(
    profile: &crate::lighting::LightingProfile,
    frame_radius: f32,
) -> ([f32; 4], [[f32; 4]; 12]) {
    let scale = if frame_radius.is_finite() && frame_radius > 1.0e-4 {
        frame_radius
    } else {
        1.0
    };
    let mut lanes = [[0.0_f32; 4]; 12];
    let count = profile.punctual_count.min(MAX_PUNCTUAL_LIGHTS as u32);
    for (index, light) in profile.punctual.iter().take(count as usize).enumerate() {
        let direction = Vec3::from(light.direction).normalize_or_zero();
        lanes[index * 3] = [
            light.position[0] * scale,
            light.position[1] * scale,
            light.position[2] * scale,
            light.range * scale,
        ];
        lanes[index * 3 + 1] = [direction.x, direction.y, direction.z, light.cos_inner];

        let gain = scale * scale;
        lanes[index * 3 + 2] = [
            light.radiance[0] * gain,
            light.radiance[1] * gain,
            light.radiance[2] * gain,
            light.cos_outer,
        ];
    }
    ([count as f32, 0.0, 0.0, 0.0], lanes)
}

fn lighting_uniform_data(preset: LightingPreset, frame_radius: f32) -> LightingUniformData {
    let profile = preset.profile();
    let (punctual_meta, punctual) = punctual_uniform_data(&profile, frame_radius);
    LightingUniformData {
        punctual_meta,
        punctual,
        key_light: [
            profile.key_radiance[0],
            profile.key_radiance[1],
            profile.key_radiance[2],
            0.0,
        ],
        fill_light: [
            profile.fill_radiance[0],
            profile.fill_radiance[1],
            profile.fill_radiance[2],
            0.0,
        ],
        environment_top: [
            profile.environment_top[0],
            profile.environment_top[1],
            profile.environment_top[2],
            profile.specular_strength,
        ],
        environment_bottom: [
            profile.environment_bottom[0],
            profile.environment_bottom[1],
            profile.environment_bottom[2],
            profile.grazing_strength,
        ],
    }
}

fn srgb_to_linear_component(value: f32) -> f32 {
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn rgba_srgb_to_linear(color: [f32; 4]) -> [f32; 4] {
    [
        srgb_to_linear_component(color[0]),
        srgb_to_linear_component(color[1]),
        srgb_to_linear_component(color[2]),
        color[3],
    ]
}

fn rgba8_srgb_to_linear(color: [u8; 4]) -> [f32; 4] {
    rgba_srgb_to_linear([
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
        f32::from(color[3]) / 255.0,
    ])
}

#[cfg(test)]
fn linear_to_srgb_component(value: f32) -> f32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
fn display_component(linear_value: f32, target_is_srgb: bool) -> f32 {
    let safe = if linear_value.is_finite() {
        linear_value.max(0.0)
    } else {
        0.0
    };
    let exposed = safe * (1.0 / 0.82);
    let mapped = exposed / (1.0 + exposed);
    if target_is_srgb {
        mapped
    } else {
        linear_to_srgb_component(mapped)
    }
}

#[cfg(test)]
fn clear_shell_alpha(n_dot_v: f32, f0: f32, maximum: f32) -> f32 {
    let n_dot_v = if n_dot_v.is_finite() {
        n_dot_v.clamp(0.0, 1.0)
    } else {
        1.0
    };
    (f0 + (1.0 - f0) * (1.0 - n_dot_v).powi(5)).min(maximum)
}

#[cfg(test)]
fn environment_specular_weight(
    roughness: f32,
    specular_level: f32,
    n_dot_v: f32,
    specular_strength: f32,
    grazing_strength: f32,
) -> f32 {
    let roughness = roughness.clamp(0.0, 1.0);
    let n_dot_v = n_dot_v.clamp(0.0, 1.0);
    let smoothness = 1.0 - roughness;
    let f0 = 0.018 + specular_level.clamp(0.0, 1.0) * 0.07;
    let grazing_limit = smoothness.max(f0);
    let fresnel = f0 + (grazing_limit - f0) * (1.0 - n_dot_v).powi(5);
    let visibility = 0.12 + smoothness * smoothness * specular_strength.max(0.0);
    let grazing = 1.0 + grazing_strength.max(0.0) * (1.0 - n_dot_v).powi(2);
    fresnel * visibility * grazing
}

#[cfg(test)]
fn rotated_light_directions(yaw_radians: f32) -> (Vec3, Vec3) {
    let yaw = sanitized_light_yaw(yaw_radians);
    let (sine, cosine) = yaw.sin_cos();
    let rotate = |direction: Vec3| {
        Vec3::new(
            cosine * direction.x + sine * direction.z,
            direction.y,
            -sine * direction.x + cosine * direction.z,
        )
        .normalize()
    };
    (rotate(BASE_KEY_DIRECTION), rotate(BASE_FILL_DIRECTION))
}

pub(crate) fn evict_lru_scenes<T>(
    scenes: &mut BTreeMap<u64, T>,
    keep: u64,
    cap: usize,
    last_used: impl Fn(&T) -> u64,
) {
    while scenes.len() > cap {
        let stale = scenes
            .iter()
            .filter(|&(&key, _)| key != keep)
            .min_by_key(|(_, scene)| last_used(scene))
            .map(|(&key, _)| key);
        let Some(stale) = stale else {
            break;
        };
        scenes.remove(&stale);
    }
}

#[derive(Debug, Default)]
struct SmoothedPositionCache {
    source_mesh: Option<Arc<Mesh>>,
    topology: Option<Arc<SurfaceSmoothingTopology>>,
    smooth_passes: u8,
    scratch: SurfaceSmoothingScratch,
    #[cfg(test)]
    evaluations: usize,
}

impl SmoothedPositionCache {
    fn positions(&mut self, mesh: &SurfaceMesh, smooth_passes: u8) -> Option<&[[f64; 3]]> {
        debug_assert!(smooth_passes > 0);
        let reusable = self
            .source_mesh
            .as_ref()
            .zip(self.topology.as_ref())
            .is_some_and(|(source, topology)| {
                Arc::ptr_eq(source, &mesh.mesh)
                    && Arc::ptr_eq(topology, &mesh.smoothing_topology)
                    && self.smooth_passes == smooth_passes
            });
        if !reusable {
            mesh.smoothing_topology
                .smooth_into(&mesh.mesh.vertices, smooth_passes, &mut self.scratch)
                .ok()?;
            self.source_mesh = Some(Arc::clone(&mesh.mesh));
            self.topology = Some(Arc::clone(&mesh.smoothing_topology));
            self.smooth_passes = smooth_passes;
            #[cfg(test)]
            {
                self.evaluations += 1;
            }
        }
        Some(self.scratch.positions())
    }

    #[cfg(test)]
    const fn evaluations(&self) -> usize {
        self.evaluations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneTarget {
    Screen,

    Hdr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_smoothing_changes_only_the_uploaded_surface_stream() {
        let mesh = SurfaceMesh::new(
            Mesh::new(
                vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap();
        let authoritative = mesh.mesh.vertices.clone();
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut smoothing = SmoothedPositionCache::default();

        fill_render_vertices(&mut vertices, &mut normals, &mut smoothing, &mesh, 1);

        assert_eq!(mesh.mesh.vertices, authoritative);
        assert_eq!(vertices[0].position, [0.75, 0.75, 0.0]);
        assert_eq!(vertices[1].position, [0.5, 0.75, 0.0]);
        assert_eq!(vertices[2].position, [0.75, 0.5, 0.0]);
        assert!(
            vertices
                .iter()
                .all(|vertex| vertex.normal.into_iter().all(f32::is_finite))
        );
    }

    #[test]
    fn smoothing_cache_evaluates_shared_mesh_only_once_across_surface_views() {
        let mesh = SurfaceMesh::new(
            Mesh::new(
                vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap();
        let mut second_view = mesh.clone();
        second_view.revision = second_view.revision.wrapping_add(99);
        let mut cache = SmoothedPositionCache::default();

        let first_ptr = cache.positions(&mesh, 4).unwrap().as_ptr();
        let second_ptr = cache.positions(&second_view, 4).unwrap().as_ptr();

        assert_eq!(first_ptr, second_ptr);
        assert_eq!(cache.evaluations(), 1);
    }

    #[test]
    fn translucent_solid_meshes_draw_a_depth_prepass_then_one_blend_layer() {
        assert_eq!(
            mesh_pass_sequence(RenderStyle::Solid, true),
            &[
                MeshPassKind::TranslucentDepthPrepass,
                MeshPassKind::TranslucentColor,
            ]
        );
        assert_eq!(
            mesh_pass_sequence(RenderStyle::Solid, false),
            &[MeshPassKind::SolidOpaque]
        );

        assert_eq!(
            mesh_pass_sequence(RenderStyle::Xray, true),
            &[MeshPassKind::Xray]
        );
        assert_eq!(
            mesh_pass_sequence(RenderStyle::Wire, true),
            &[MeshPassKind::Wire]
        );
        assert!(mesh_color_is_translucent([1.0, 1.0, 1.0, 0.62]));
        assert!(!mesh_color_is_translucent([1.0, 1.0, 1.0, 1.0]));
        assert!(!mesh_color_is_translucent([1.0, 1.0, 1.0, f32::NAN]));
    }

    #[test]
    fn translucent_pass_pipeline_state_shades_exactly_the_nearest_surface() {
        let prepass = mesh_pipeline_config(MeshPassKind::TranslucentDepthPrepass);
        assert!(prepass.depth_write_enabled);
        assert_eq!(prepass.fragment_entry, "fs_depth_only");
        assert_eq!(prepass.write_mask, wgpu::ColorWrites::empty());
        assert!(prepass.blend.is_none());
        assert_eq!(prepass.topology, wgpu::PrimitiveTopology::TriangleList);

        let color = mesh_pipeline_config(MeshPassKind::TranslucentColor);
        assert!(!color.depth_write_enabled);
        assert_eq!(color.fragment_entry, "fs_main");
        assert_eq!(color.blend, Some(wgpu::BlendState::ALPHA_BLENDING));
        assert_eq!(color.write_mask, wgpu::ColorWrites::ALL);

        let solid = mesh_pipeline_config(MeshPassKind::SolidOpaque);
        assert!(solid.depth_write_enabled);
        assert_eq!(solid.fragment_entry, "fs_main");

        let xray = mesh_pipeline_config(MeshPassKind::Xray);
        assert!(!xray.depth_write_enabled);
        assert_eq!(xray.blend, Some(wgpu::BlendState::ALPHA_BLENDING));

        let wire = mesh_pipeline_config(MeshPassKind::Wire);
        assert_eq!(wire.topology, wgpu::PrimitiveTopology::LineList);

        assert!(SHADER.contains("fn fs_depth_only"));
    }

    #[test]
    fn default_light_data_preserves_the_original_key_and_fill() {
        let (key, fill) = rotated_light_directions(DEFAULT_LIGHT_YAW_RADIANS);
        assert!(key.abs_diff_eq(BASE_KEY_DIRECTION.normalize(), 1.0e-6));
        assert!(fill.abs_diff_eq(BASE_FILL_DIRECTION.normalize(), 1.0e-6));
        assert_eq!(std::mem::size_of::<SceneUniform>() % 16, 0);
    }

    #[test]
    fn inverse_transpose_keeps_nonuniformly_scaled_normals_perpendicular() {
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(2.5, 0.55, 1.4),
            glam::Quat::from_euler(glam::EulerRot::XYZ, 0.37, -0.82, 1.13),
            Vec3::new(7.0, -3.0, 2.0),
        );
        let tangent_a = Vec3::new(1.0, 1.0, 0.0).normalize();
        let tangent_b = Vec3::new(0.0, 1.0, 1.0).normalize();
        let local_normal = tangent_a.cross(tangent_b).normalize();
        let world_tangent_a = model.transform_vector3(tangent_a);
        let world_tangent_b = model.transform_vector3(tangent_b);
        let transformed_normal = normal_matrix(model)
            .transform_vector3(local_normal)
            .normalize();

        assert!(transformed_normal.dot(world_tangent_a).abs() < 1.0e-5);
        assert!(transformed_normal.dot(world_tangent_b).abs() < 1.0e-5);
        assert!(
            transformed_normal.dot(world_tangent_a.cross(world_tangent_b).normalize()) > 0.999_99
        );
        for shader in [SHADER, SKIN_SHADER] {
            assert!(shader.contains("normal_matrix: mat4x4<f32>"));
            assert!(shader.contains("scene.normal_matrix * vec4<f32>(input.normal, 0.0)"));
            assert!(!shader.contains("scene.model * vec4<f32>(input.normal, 0.0)"));
        }
    }

    #[test]
    fn invalid_normal_matrix_falls_back_to_a_finite_identity() {
        assert_eq!(
            normal_matrix(Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0))),
            Mat4::IDENTITY
        );
    }

    #[test]
    fn light_yaw_rotates_only_around_world_y_and_rejects_non_finite_values() {
        let (base_key, _) = rotated_light_directions(0.0);
        let (turned_key, _) = rotated_light_directions(std::f32::consts::FRAC_PI_2);
        assert!((turned_key.y - base_key.y).abs() < 1.0e-6);
        assert!((turned_key.x - base_key.z).abs() < 1.0e-6);
        assert!((turned_key.z + base_key.x).abs() < 1.0e-6);
        assert_eq!(
            rotated_light_directions(f32::NAN),
            rotated_light_directions(DEFAULT_LIGHT_YAW_RADIANS)
        );
    }

    #[test]
    fn independent_mesh_layers_explicitly_reset_depth_before_drawing() {
        assert!(!RenderDepthScope::default().resets_before_draw());
        assert!(RenderDepthScope::ResetBeforeDraw.resets_before_draw());
        assert!(DEPTH_RESET_SHADER.contains("@builtin(frag_depth)"));
        assert!(DEPTH_RESET_SHADER.contains("output.depth = 1.0"));
    }

    #[test]
    fn skin_depth_scope_resets_complete_layers_and_shares_grouped_depth() {
        assert!(RenderDepthScope::ResetBeforeDraw.resets_before_draw());
        assert!(!RenderDepthScope::Shared.resets_before_draw());
    }

    #[test]
    fn skin_attachment_visibility_preserves_result_toggles() {
        let all = skin_visibility_mask(SkinVisibilityGroups::ALL, true, true);
        assert_ne!(all & channel_bit(SkinChannel::Lacrimal), 0);
        assert_ne!(all & channel_bit(SkinChannel::Tear), 0);
        assert_ne!(all & channel_bit(SkinChannel::Eyelashes), 0);
        let hidden = skin_visibility_mask(SkinVisibilityGroups::ALL, false, false);
        assert_eq!(hidden & channel_bit(SkinChannel::Lacrimal), 0);
        assert_eq!(hidden & channel_bit(SkinChannel::Tear), 0);
        assert_eq!(hidden & channel_bit(SkinChannel::Eyelashes), 0);
        assert_ne!(hidden & channel_bit(SkinChannel::Face), 0);
    }

    #[test]
    fn skin_visibility_groups_map_only_owned_channels() {
        let head = SkinVisibilityGroups::HEAD_SKIN.channel_mask();
        assert_ne!(head & channel_bit(SkinChannel::Face), 0);
        assert_ne!(head & channel_bit(SkinChannel::Torso), 0);
        assert_eq!(head & channel_bit(SkinChannel::Sclera), 0);
        let eyes = SkinVisibilityGroups::EYES.channel_mask();
        for channel in [
            SkinChannel::Sclera,
            SkinChannel::Iris,
            SkinChannel::Pupil,
            SkinChannel::Cornea,
            SkinChannel::EyeReflection,
        ] {
            assert_ne!(eyes & channel_bit(channel), 0);
        }
        assert_eq!(eyes & channel_bit(SkinChannel::Lacrimal), 0);
        assert_eq!(
            SkinVisibilityGroups::EYELASHES.channel_mask(),
            channel_bit(SkinChannel::Eyelashes)
        );
        assert_eq!(SkinVisibilityGroups::default(), SkinVisibilityGroups::ALL);
    }

    #[test]
    fn skin_shader_separates_cutout_lashes_from_transmissive_eye_shells() {
        assert!(SKIN_SHADER.contains("albedo.a < 0.25"));
        assert!(SKIN_SHADER.contains("fs_skin_transparent"));
        assert!(SKIN_SHADER.contains("input.channel == 5u"));
        assert!(SKIN_SHADER.contains("input.channel == 6u"));
        assert!(SKIN_SHADER.contains("input.channel == 8u"));

        assert!(SKIN_SHADER.contains("var f0 = 0.025;"));
        assert!(SKIN_SHADER.contains("f0 = 0.020;"));
        assert!(SKIN_SHADER.contains("f0 + (1.0 - f0) * pow(1.0 - n_dot_v, 5.0)"));

        assert!(SKIN_SHADER.contains("min(fresnel + highlight, MAX_CLEAR_SHELL_COVERAGE)"));

        assert!(!SKIN_SHADER.contains("min(fresnel, 0.16)"));
        assert!(!SKIN_SHADER.contains("min(fresnel, 0.12)"));
        assert!(!SKIN_SHADER.contains("min(fresnel, 0.20)"));
        assert!(!SKIN_SHADER.contains("vec3<f32>(0.42)"));
        assert!(SKIN_SHADER.contains("if (!front_facing) { return vec4<f32>(0.0); }"));

        assert!(
            SKIN_SHADER.contains("eye_lamp_direction(rotated_key_direction(), view_direction)")
        );
        assert!(SKIN_SHADER.contains("dot(reflection_direction, rotated_fill_direction())"));
        assert!(!SKIN_SHADER.contains("0.015 + key_amount"));
        assert!(!SKIN_SHADER.contains("0.008 + fill_amount"));
        assert!(SKIN_SHADER.contains(") * lighting_exposure()"));
    }

    #[test]
    fn eye_and_lash_vertical_orientation_is_texture_source_aware() {
        let uv = [0.25, 0.20];
        assert_eq!(
            SkinChannel::Face.texture_uv(uv, SkinUvOrientation::VamCacheDirectV),
            [0.25, 0.80]
        );
        assert_eq!(
            SkinChannel::Torso.texture_uv(uv, SkinUvOrientation::VamCacheDirectV),
            [0.25, 0.80]
        );
        for channel in [
            SkinChannel::Sclera,
            SkinChannel::Iris,
            SkinChannel::Pupil,
            SkinChannel::Lacrimal,
            SkinChannel::InnerMouth,
            SkinChannel::Teeth,
            SkinChannel::Gums,
            SkinChannel::Tongue,
            SkinChannel::Eyelashes,
        ] {
            assert_eq!(
                channel.texture_uv(uv, SkinUvOrientation::VamCacheDirectV),
                uv,
                "VaM cache {channel:?}"
            );
            assert_eq!(
                channel.texture_uv(uv, SkinUvOrientation::ObjFlipV),
                [0.25, 0.80],
                "ordinary image {channel:?}"
            );
        }
    }

    #[test]
    fn texture_origin_conversion_does_not_reverse_the_normal_map_tangent_basis() {
        let original = [0.25, 0.20];
        let (face_sample, face_tangent) =
            skin_render_uvs(SkinChannel::Face, original, SkinUvOrientation::ObjFlipV);
        assert_eq!(face_sample, [0.25, 0.80]);
        assert_eq!(face_tangent, original);

        let (eye_sample, eye_tangent) = skin_render_uvs(
            SkinChannel::Sclera,
            original,
            SkinUvOrientation::VamCacheDirectV,
        );
        assert_eq!(
            eye_sample, original,
            "verified VaM eye atlas stays direct-V"
        );
        assert_eq!(eye_tangent, original);
        let (ordinary_eye_sample, ordinary_eye_tangent) =
            skin_render_uvs(SkinChannel::Sclera, original, SkinUvOrientation::ObjFlipV);
        assert_eq!(ordinary_eye_sample, [0.25, 0.80]);
        assert_eq!(ordinary_eye_tangent, original);
        assert!(SKIN_SHADER.contains("dpdx(input.tangent_uv)"));
        assert!(SKIN_SHADER.contains("dpdy(input.tangent_uv)"));
        assert!(!SKIN_SHADER.contains("let uv_dx = dpdx(input.uv)"));
    }

    #[test]
    fn missing_eye_atlas_keeps_sclera_light_and_pupil_near_black() {
        let colors = [
            [238, 238, 228, 255],
            [82, 112, 116, 255],
            [190, 92, 102, 255],
            [67, 18, 23, 255],
            [242, 232, 205, 255],
            [151, 57, 68, 255],
            [178, 75, 88, 255],
            [22, 15, 11, 255],
        ];
        let mut textured = [false; 8];
        let sclera = skin_channel_tint_data(&colors, &textured, SkinChannel::Sclera);
        let pupil = skin_channel_tint_data(&colors, &textured, SkinChannel::Pupil);
        assert!(sclera[0] > 0.85 && sclera[1] > 0.85 && sclera[2] > 0.75);
        assert!(pupil[0] < 0.01 && pupil[1] < 0.01 && pupil[2] < 0.01);

        textured[1] = true;
        assert!(
            skin_channel_tint_data(&colors, &textured, SkinChannel::Pupil)[0] < 0.01,
            "an iris-only texture must not claim the full-eye pupil atlas"
        );
        textured[0] = true;
        assert_eq!(
            skin_channel_tint_data(&colors, &textured, SkinChannel::Pupil),
            [1.0; 4],
            "a valid atlas already contains the authored pupil value"
        );
    }

    #[test]
    fn skin_shader_uses_linear_pbr_maps_and_derivative_tangent_frame() {
        assert!(SKIN_SHADER.contains("face_surface_texture"));
        assert!(SKIN_SHADER.contains("mouth_surface_texture"));
        assert!(SKIN_SHADER.contains("sclera_surface_texture"));
        assert!(SKIN_SHADER.contains("iris_surface_texture"));
        assert!(SKIN_SHADER.contains("lacrimal_surface_texture"));
        for removed in [
            "inner_mouth_surface_texture",
            "teeth_surface_texture",
            "gums_surface_texture",
            "tongue_surface_texture",
        ] {
            assert!(!SKIN_SHADER.contains(removed));
        }
        assert!(SKIN_SHADER.contains("surface.specular_level"));
        assert!(SKIN_SHADER.contains("surface.roughness"));
        assert!(SKIN_SHADER.contains("tangent_space_normal"));
        assert!(SKIN_SHADER.contains("distribution_ggx"));
        assert!(SKIN_SHADER.contains("surface.roughness = 1.0 - packed.a"));
        assert!(SKIN_SHADER.contains("uv_area_scale"));
        assert!(!SKIN_SHADER.contains("abs(determinant) < 0.0000001"));
        assert!(!SKIN_SHADER.contains("dot(cross(normal, tangent), bitangent)"));
    }

    #[test]
    fn the_skin_shader_binds_exactly_the_textures_the_device_is_asked_for() {
        for binding in 0..=SKIN_SAMPLER_BINDING {
            assert!(
                SKIN_SHADER.contains(&format!("@group(0) @binding({binding})")),
                "missing skin binding {binding}"
            );
        }
        assert!(
            !SKIN_SHADER.contains(&format!("@group(0) @binding({})", SKIN_SAMPLER_BINDING + 1))
        );
        assert_eq!(
            SKIN_SHADER.matches(": texture_2d<f32>;").count(),
            SKIN_TEXTURE_COUNT
        );
        assert_eq!(SKIN_SHADER.matches("var skin_sampler: sampler;").count(), 1);

        let sampler_declaration = SKIN_SHADER
            .split("@group(0) @binding(")
            .find(|section| section.contains("var skin_sampler: sampler;"))
            .expect("the shader declares a sampler");
        let declared: u32 = sampler_declaration
            .split(')')
            .next()
            .and_then(|number| number.trim().parse().ok())
            .expect("the sampler's binding is a number");
        assert_eq!(
            declared, SKIN_SAMPLER_BINDING,
            "the shader samples at {declared} and Rust binds the sampler at {SKIN_SAMPLER_BINDING}"
        );

        assert!(SKIN_SAMPLER_BINDING as usize > SKIN_TEXTURE_COUNT);
    }

    #[test]
    fn mouth_surface_atlas_uses_fixed_tiles_and_half_texel_insets() {
        fn atlas_uv(uv: [f32; 2], tile: u32, dimensions: [f32; 2]) -> [f32; 2] {
            let tile_size = [dimensions[0] * 0.5, dimensions[1] * 0.5];
            let origin = [
                (tile % 2) as f32 * tile_size[0],
                (tile / 2) as f32 * tile_size[1],
            ];
            [
                (origin[0] + 0.5 + uv[0].clamp(0.0, 1.0) * (tile_size[0] - 1.0)) / dimensions[0],
                (origin[1] + 0.5 + uv[1].clamp(0.0, 1.0) * (tile_size[1] - 1.0)) / dimensions[1],
            ]
        }

        let dimensions = [8.0, 6.0];
        assert_eq!(atlas_uv([0.0, 0.0], 0, dimensions), [0.5 / 8.0, 0.5 / 6.0]);
        assert_eq!(atlas_uv([1.0, 1.0], 0, dimensions), [3.5 / 8.0, 2.5 / 6.0]);
        assert_eq!(atlas_uv([0.0, 0.0], 1, dimensions), [4.5 / 8.0, 0.5 / 6.0]);
        assert_eq!(atlas_uv([0.0, 0.0], 2, dimensions), [0.5 / 8.0, 3.5 / 6.0]);
        assert_eq!(atlas_uv([0.0, 0.0], 3, dimensions), [4.5 / 8.0, 3.5 / 6.0]);
        for tile in 0..4 {
            let lower = atlas_uv([0.0, 0.0], tile, dimensions);
            let upper = atlas_uv([1.0, 1.0], tile, dimensions);
            assert_ne!(lower[0], 0.5);
            assert_ne!(upper[0], 0.5);
            assert_ne!(lower[1], 0.5);
            assert_ne!(upper[1], 0.5);
        }
        for (channel, tile) in [(9, 0), (10, 1), (11, 2), (12, 3)] {
            assert!(SKIN_SHADER.contains(&format!("input.channel == {channel}u")));
            assert!(SKIN_SHADER.contains(&format!("mouth_surface_uv(input.uv, {tile}u)")));
        }
        assert!(SKIN_SHADER.contains("let local_texel = vec2<f32>(0.5)"));
    }

    #[test]
    fn eye_surface_maps_feed_only_the_opaque_eye_materials() {
        for sample in [
            "textureSample(sclera_surface_texture, skin_sampler, input.uv)",
            "textureSample(iris_surface_texture, skin_sampler, input.uv)",
            "textureSample(lacrimal_surface_texture, skin_sampler, input.uv)",
        ] {
            assert!(SKIN_SHADER.contains(sample));
        }
        let transparent = SKIN_SHADER
            .split("fn fs_skin_transparent")
            .nth(1)
            .expect("transparent shader entry point");
        assert!(!transparent.contains("surface_texture"));
    }

    #[test]
    fn eye_materials_get_scoped_environment_fill_without_double_darkening_pupils() {
        assert!(SKIN_SHADER.contains("fn material_environment_fill("));
        assert!(SKIN_SHADER.contains("channel_scale = vec3<f32>(1.60, 1.54, 1.45)"));
        assert!(SKIN_SHADER.contains("channel_scale = vec3<f32>(1.12, 1.10, 1.06)"));
        assert!(SKIN_SHADER.contains("channel_scale = vec3<f32>(0.48, 0.48, 0.47)"));
        assert!(SKIN_SHADER.contains("channel_scale = vec3<f32>(1.22, 1.20, 1.16)"));
        assert!(SKIN_SHADER.contains("environment_radiance(normal)"));
        assert!(SKIN_SHADER.contains("lighting_exposure()"));
        assert!(SKIN_SHADER.contains(
            "let ambient = material_environment_fill(input.channel, base_color, normal);"
        ));

        let pupil_branch = SKIN_SHADER
            .split("input.channel == 4u")
            .nth(1)
            .and_then(|tail| tail.split("} else if").next())
            .expect("pupil albedo branch");
        assert!(
            pupil_branch
                .contains("albedo = textureSample(sclera_texture, skin_sampler, input.uv);")
        );
        assert!(!pupil_branch.contains("textureSample(iris_texture"));
        assert!(!pupil_branch.contains("albedo.rgb *"));
        assert!(!pupil_branch.contains("* 0.16"));
    }

    #[test]
    fn preview_lighting_exposes_five_color_presets_and_gloss_check() {
        assert!(SHADER.contains("fn lighting_preset() -> u32"));
        assert!(SHADER.contains("scene.lighting.z"));
        assert!(SHADER.contains("preset == 4u"));
        for source in [SHADER, SKIN_SHADER] {
            assert!(source.contains("lighting_exposure()"));
            assert!(source.contains("scene.key_light.rgb"));
            assert!(source.contains("scene.fill_light.rgb"));
            assert!(source.contains("scene.environment_top.rgb"));
            assert!(source.contains("scene.environment_bottom.rgb"));
        }
        assert!(SKIN_SHADER.contains("fn key_radiance()"));
        assert!(SKIN_SHADER.contains("fn environment_radiance(direction: vec3<f32>)"));
        assert!(SKIN_SHADER.contains("fn environment_specular("));
        assert!(SKIN_SHADER.contains("environment_specular_strength()"));
        assert!(SKIN_SHADER.contains("environment_grazing_strength()"));

        for preset in LightingPreset::ALL {
            let expected = preset.profile();
            let actual = lighting_uniform_data(preset, 1.0);
            assert_eq!(&actual.key_light[..3], &expected.key_radiance);
            assert_eq!(&actual.fill_light[..3], &expected.fill_radiance);
            assert_eq!(&actual.environment_top[..3], &expected.environment_top);
            assert_eq!(
                &actual.environment_bottom[..3],
                &expected.environment_bottom
            );
            assert_eq!(actual.environment_top[3], expected.specular_strength);
            assert_eq!(actual.environment_bottom[3], expected.grazing_strength);
        }
    }

    #[test]
    fn environment_specular_reveals_authored_roughness_and_specular_level() {
        let studio = LightingPreset::Studio.profile();
        let smooth = environment_specular_weight(
            0.18,
            0.45,
            0.72,
            studio.specular_strength,
            studio.grazing_strength,
        );
        let rough = environment_specular_weight(
            0.82,
            0.45,
            0.72,
            studio.specular_strength,
            studio.grazing_strength,
        );
        let low_specular = environment_specular_weight(
            0.18,
            0.05,
            0.72,
            studio.specular_strength,
            studio.grazing_strength,
        );
        assert!(smooth > rough * 3.0, "smooth={smooth} rough={rough}");
        assert!(
            smooth > low_specular,
            "specular map must affect reflected energy"
        );

        let gloss = LightingPreset::Gloss.profile();
        let gloss_check = environment_specular_weight(
            0.18,
            0.45,
            0.72,
            gloss.specular_strength,
            gloss.grazing_strength,
        );
        assert!(
            gloss_check > smooth,
            "Gloss Check must make roughness easier to inspect"
        );
    }

    #[test]
    fn final_color_transfer_matches_srgb_and_linear_targets_at_middle_gray() {
        let linear_middle_gray = 0.18;
        let srgb_attachment_value = display_component(linear_middle_gray, true);
        let unorm_attachment_value = display_component(linear_middle_gray, false);

        assert!((srgb_attachment_value - 0.18).abs() < 1.0e-6);
        assert!((unorm_attachment_value - 0.461).abs() < 0.002);
        assert!(
            (srgb_to_linear_component(unorm_attachment_value) - srgb_attachment_value).abs()
                < 1.0e-6,
            "both target formats must reach the same displayed luminance"
        );

        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert!(display_component(invalid, true).is_finite());
            assert!(display_component(invalid, false).is_finite());
            assert_eq!(display_component(invalid, true), 0.0);
            assert_eq!(display_component(invalid, false), 0.0);
        }

        for source in [SHADER, SKIN_SHADER] {
            assert!(source.contains("fn graded_display("), "preamble missing");
            assert!(
                source.contains("fn sanitize_radiance("),
                "NaN guard missing"
            );
            assert!(source.contains("fn display_color(linear_color: vec3<f32>)"));
            assert!(
                source.contains("scene.lighting.w > 0.5"),
                "sRGB lane not forwarded"
            );
            assert!(
                source.contains("scene.grading.x > 0.5"),
                "curve lane not forwarded"
            );
        }
        assert!(wgpu::TextureFormat::Bgra8UnormSrgb.is_srgb());
        assert!(!wgpu::TextureFormat::Bgra8Unorm.is_srgb());
    }

    #[test]
    fn the_mesh_and_skin_shaders_compile_on_an_available_adapter() {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            }))
        else {
            return;
        };
        let Ok((device, _queue)) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("vkit.mesh-shader-test"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            }))
        else {
            return;
        };
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        for (label, source) in [
            ("mesh", SHADER),
            ("skin", SKIN_SHADER),
            ("depth-reset", DEPTH_RESET_SHADER),
            ("mip-blit", MIP_BLIT_SHADER),
        ] {
            let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        }

        let _skin = SkinRenderResources::new(
            &device,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            MSAA_SAMPLES,
            DEPTH_FORMAT,
        );
        let failure = pollster::block_on(scope.pop());
        assert!(failure.is_none(), "shader failed to compile: {failure:?}");
    }

    fn wrapped_diffuse(facing: f32, scatter: f32) -> (f32, f32) {
        let lambert = facing.max(0.0);
        let wrapped = ((facing + scatter) / (1.0 + scatter)).clamp(0.0, 1.0);
        (lambert, (wrapped - lambert).max(0.0))
    }

    #[test]
    fn subsurface_scatter_is_a_no_op_on_everything_that_is_not_flesh() {
        for step in -100..=100 {
            let facing = step as f32 / 100.0;
            let (lambert, scattered) = wrapped_diffuse(facing, 0.0);
            assert_eq!(scattered, 0.0, "facing {facing} leaked scatter at zero");
            assert_eq!(lambert, facing.max(0.0));
        }
    }

    #[test]
    fn subsurface_scatter_softens_the_terminator_without_lifting_a_full_on_surface() {
        const SCATTER: f32 = 0.36;

        let (lambert, scattered) = wrapped_diffuse(1.0, SCATTER);
        assert!((lambert - 1.0).abs() < 1.0e-6);
        assert!(scattered < 1.0e-6, "a full-on surface gained {scattered}");

        let (lambert, scattered) = wrapped_diffuse(0.0, SCATTER);
        assert_eq!(lambert, 0.0);
        assert!(scattered > 0.2, "terminator only gained {scattered}");

        assert!(wrapped_diffuse(-0.2, SCATTER).1 > 0.0);
        assert_eq!(wrapped_diffuse(-0.5, SCATTER).1, 0.0);
    }

    #[test]
    fn only_flesh_scatters_in_the_skin_shader() {
        assert!(SKIN_SHADER.contains("fn material_scatter(channel: u32)"));

        assert!(
            SKIN_SHADER.contains("if (channel == 0u || channel == 1u) { return SKIN_SCATTER; }")
        );
        assert!(SKIN_SHADER.contains(
            "if (channel == 9u || channel == 11u || channel == 12u) { return SKIN_SCATTER; }"
        ));
        assert!(SKIN_SHADER.contains("SKIN_SCATTER_TINT"));
    }

    #[test]
    fn cpu_color_constants_enter_the_shader_in_linear_space() {
        let middle = rgba8_srgb_to_linear([128, 128, 128, 128]);
        assert!((middle[0] - 0.215_86).abs() < 1.0e-4);
        assert_eq!(middle[0], middle[1]);
        assert_eq!(middle[1], middle[2]);
        assert!((middle[3] - 128.0 / 255.0).abs() < 1.0e-6);

        let floating = rgba_srgb_to_linear([0.5, 0.25, 1.0, 0.28]);
        assert!((floating[0] - 0.214_04).abs() < 1.0e-4);
        assert!((floating[1] - 0.050_88).abs() < 1.0e-4);
        assert_eq!(floating[2], 1.0);
        assert_eq!(floating[3], 0.28, "alpha is coverage, not an sRGB color");
    }

    #[test]
    fn transparent_eye_shells_preserve_center_view_energy() {
        let cornea_center = clear_shell_alpha(1.0, 0.020, 0.16);
        let reflection_center = clear_shell_alpha(1.0, 0.012, 0.12);
        let tear_center = clear_shell_alpha(1.0, 0.020, 0.20);
        assert!((cornea_center - 0.020).abs() < 1.0e-6);
        assert!((reflection_center - 0.012).abs() < 1.0e-6);
        assert!((tear_center - 0.020).abs() < 1.0e-6);

        let stacked_eye_alpha = 1.0 - (1.0 - cornea_center) * (1.0 - reflection_center);
        assert!(
            stacked_eye_alpha < 0.035,
            "clear eye shells must not veil the iris"
        );
        assert_eq!(clear_shell_alpha(0.0, 0.020, 0.16), 0.16);
        assert_eq!(clear_shell_alpha(0.0, 0.012, 0.12), 0.12);
        assert_eq!(clear_shell_alpha(0.0, 0.020, 0.20), 0.20);

        let stacked_with_tear = 1.0 - (1.0 - stacked_eye_alpha) * (1.0 - tear_center);
        assert!(
            stacked_with_tear < 0.052,
            "even all three front-facing clear layers must not veil the eye"
        );

        let underlying = 0.18;
        let reflected = 0.75;
        let composited = reflected * stacked_with_tear + underlying * (1.0 - stacked_with_tear);
        assert!(composited > underlying && composited < 0.21);
        assert!(composited.is_finite());
    }

    #[test]
    fn every_embedded_wgsl_shader_parses_before_runtime() {
        for (name, source) in [
            ("mesh", SHADER),
            ("skin", SKIN_SHADER),
            ("skin-hdr", SKIN_SHADER_HDR),
            ("depth-reset", DEPTH_RESET_SHADER),
            ("mip-blit", MIP_BLIT_SHADER),
            ("hair", crate::hair_renderer::HAIR_SHADER),
            ("hair-hdr", crate::hair_renderer::HAIR_SHADER_HDR),
            ("hair-physics", crate::hair_physics::HAIR_PHYSICS_SHADER),
            ("scalp", crate::hair_renderer::SCALP_SHADER),
            ("scalp-hdr", crate::hair_renderer::SCALP_SHADER_HDR),
            ("bloom", crate::bloom::BLOOM_WGSL),
        ] {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("{name} WGSL failed to parse: {error}"));

            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{name} WGSL failed to validate: {error:?}"));
        }
    }

    #[test]
    fn the_positioned_rig_scales_with_what_the_camera_frames() {
        let profile = LightingPreset::Portrait.profile();
        assert!(profile.punctual_count > 0, "the preset must carry lights");
        let (unit_meta, unit) = punctual_uniform_data(&profile, 1.0);
        let (wide_meta, wide) = punctual_uniform_data(&profile, 3.0);
        assert_eq!(unit_meta, wide_meta, "scale cannot change the light count");

        for light in 0..profile.punctual_count as usize {
            for axis in 0..4 {
                assert!(
                    (wide[light * 3][axis] - unit[light * 3][axis] * 3.0).abs() < 1.0e-4,
                    "light {light} lane 0 axis {axis} is not a length"
                );
            }

            assert_eq!(wide[light * 3 + 1], unit[light * 3 + 1]);
            assert_eq!(wide[light * 3 + 2][3], unit[light * 3 + 2][3]);
            for channel in 0..3 {
                assert!(
                    (wide[light * 3 + 2][channel] - unit[light * 3 + 2][channel] * 9.0).abs()
                        < 1.0e-3,
                    "light {light} channel {channel} does not follow the inverse square"
                );
            }
        }

        let (_, degenerate) = punctual_uniform_data(&profile, 0.0);
        assert_eq!(degenerate, unit);
    }

    #[test]
    fn the_directional_presets_carry_no_positioned_lights() {
        for preset in LightingPreset::ALL {
            if preset == LightingPreset::Portrait {
                continue;
            }
            let (meta, lanes) = punctual_uniform_data(&preset.profile(), 2.0);
            assert_eq!(meta[0], 0.0, "{preset:?} gained a light");
            assert!(lanes.iter().flatten().all(|lane| *lane == 0.0));
        }
    }

    #[test]
    fn the_scene_uniform_is_the_same_size_on_both_sides() {
        for (name, source) in [("mesh", SHADER), ("skin", SKIN_SHADER)] {
            let module = naga::front::wgsl::parse_str(source).expect("shader parses");
            let mut layouter = naga::proc::Layouter::default();
            layouter
                .update(module.to_ctx())
                .expect("shader types have a layout");
            let (handle, _) = module
                .types
                .iter()
                .find(|(_, declared)| declared.name.as_deref() == Some("SceneUniform"))
                .unwrap_or_else(|| panic!("{name} declares no SceneUniform"));
            assert_eq!(
                layouter[handle].size as usize,
                std::mem::size_of::<SceneUniform>(),
                "{name} reads a SceneUniform of a different size than the one written"
            );
        }
    }
}
