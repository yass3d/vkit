use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use egui::{Rect, epaint};
use egui_wgpu::{
    Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu,
    winit::Painter as EguiPainter,
};
use glam::{Mat4, Vec3};
use thiserror::Error;
use vkit_core::{
    formats::Mesh,
    surface_smoothing::{SurfaceSmoothingScratch, SurfaceSmoothingTopology},
};
use wgpu::util::DeviceExt as _;

use crate::{
    hair_renderer::{HairRenderResources, ScalpRenderResources},
    lighting::MAX_PUNCTUAL_LIGHTS,
    lighting::{LightingPreset, sanitize_brightness},
    scene::SurfaceMesh,
    skin_preview::{SkinChannel, SkinPreview, SkinUvOrientation},
};

pub const MSAA_SAMPLES: u32 = 4;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
pub const DEFAULT_LIGHT_YAW_RADIANS: f32 = 0.0;

#[cfg(test)]
const BASE_KEY_DIRECTION: Vec3 = Vec3::new(-0.42, 0.66, 0.61);
#[cfg(test)]
const BASE_FILL_DIRECTION: Vec3 = Vec3::new(0.67, 0.22, 0.31);

const SHADER: &str = concat!(
    crate::shader_color::color_grading_wgsl!(),
    crate::shader_scene::scene_uniform_wgsl!(),
    r#"

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world = scene.model * vec4<f32>(input.position, 1.0);
    output.clip_position = scene.view_projection * world;
    output.world_position = world.xyz;
    output.world_normal = normalize((scene.normal_matrix * vec4<f32>(input.normal, 0.0)).xyz);
    return output;
}

fn lighting_preset() -> u32 {

    return u32(clamp(scene.lighting.z + 0.5, 0.0, 5.0));
}

fn lighting_exposure() -> f32 {
    return clamp(scene.lighting.y, 0.35, 2.0);
}

fn display_color(linear_color: vec3<f32>) -> vec3<f32> {

    return graded_display(
        linear_color,
        1.0,
        scene.grading.x > 0.5,
        scene.lighting.w > 0.5,
    );
}

fn environment_radiance(direction: vec3<f32>) -> vec3<f32> {
    let hemisphere = smoothstep(-0.55, 0.75, normalize(direction).y);
    return mix(scene.environment_bottom.rgb, scene.environment_top.rgb, hemisphere);
}

@fragment
fn fs_main(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var normal = normalize(input.world_normal);
    if (!front_facing) {
        normal = -normal;
    }
    let cosine = cos(scene.lighting.x);
    let sine = sin(scene.lighting.x);
    let key_base = vec3<f32>(-0.42, 0.66, 0.61);
    let fill_base = vec3<f32>(0.67, 0.22, 0.31);
    let key_direction = normalize(vec3<f32>(
        cosine * key_base.x + sine * key_base.z,
        key_base.y,
        -sine * key_base.x + cosine * key_base.z,
    ));
    let fill_direction = normalize(vec3<f32>(
        cosine * fill_base.x + sine * fill_base.z,
        fill_base.y,
        -sine * fill_base.x + cosine * fill_base.z,
    ));
    let view_direction = normalize(scene.eye.xyz - input.world_position);
    let key = max(dot(normal, key_direction), 0.0);
    let fill = max(dot(normal, fill_direction), 0.0);
    let rim = pow(1.0 - max(dot(normal, view_direction), 0.0), 2.4);
    let preset = lighting_preset();
    let key_strength = select(0.28, 0.36, preset == 4u);
    let fill_strength = select(0.18, 0.10, preset == 4u);
    let rim_strength = select(0.10, 0.22, preset == 4u);

    var punctual = vec3<f32>(0.0);
    let lights = punctual_count();
    for (var index = 0u; index < lights; index = index + 1u) {
        let sampled = punctual_sample(index, input.world_position);
        punctual = punctual + sampled.radiance * max(dot(normal, sampled.direction), 0.0);
    }
    let light = environment_radiance(normal) * 0.85
        + scene.key_light.rgb * key * key_strength
        + scene.fill_light.rgb * fill * fill_strength
        + punctual
        + environment_radiance(reflect(-view_direction, normal)) * rim * rim_strength;
    let linear_color = scene.color.rgb * light * lighting_exposure();
    return vec4<f32>(display_color(linear_color), scene.color.a);
}

@fragment
fn fs_depth_only() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

"#
);

macro_rules! skin_shader_source {
    ($grading:expr) => {
        concat!(
            $grading,
            crate::shader_scene::scene_uniform_wgsl!(),
            r#"
        @group(0) @binding(1) var face_texture: texture_2d<f32>;
        @group(0) @binding(2) var torso_texture: texture_2d<f32>;
        @group(0) @binding(3) var sclera_texture: texture_2d<f32>;
        @group(0) @binding(4) var iris_texture: texture_2d<f32>;
        @group(0) @binding(5) var lacrimal_texture: texture_2d<f32>;
        @group(0) @binding(6) var inner_mouth_texture: texture_2d<f32>;
        @group(0) @binding(7) var teeth_texture: texture_2d<f32>;
        @group(0) @binding(8) var gums_texture: texture_2d<f32>;
        @group(0) @binding(9) var tongue_texture: texture_2d<f32>;
        @group(0) @binding(10) var eyelashes_texture: texture_2d<f32>;
        @group(0) @binding(11) var face_surface_texture: texture_2d<f32>;
        @group(0) @binding(12) var torso_surface_texture: texture_2d<f32>;
        @group(0) @binding(13) var mouth_surface_texture: texture_2d<f32>;
        @group(0) @binding(14) var sclera_surface_texture: texture_2d<f32>;
        @group(0) @binding(15) var iris_surface_texture: texture_2d<f32>;
        @group(0) @binding(16) var lacrimal_surface_texture: texture_2d<f32>;
        @group(0) @binding(17) var skin_sampler: sampler;

        struct VertexInput {
            @location(0) position: vec3<f32>,
            @location(1) normal: vec3<f32>,
            @location(2) uv: vec2<f32>,
            @location(3) tangent_uv: vec2<f32>,
            @location(4) channel: u32,
            @location(5) tint: vec4<f32>,
        };

        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) world_position: vec3<f32>,
            @location(1) world_normal: vec3<f32>,
            @location(2) uv: vec2<f32>,
            @location(3) tangent_uv: vec2<f32>,
            @location(4) @interpolate(flat) channel: u32,
            @location(5) @interpolate(flat) tint: vec4<f32>,
        };

        @vertex
        fn vs_skin(input: VertexInput) -> VertexOutput {
            var output: VertexOutput;
            let world = scene.model * vec4<f32>(input.position, 1.0);
            output.clip_position = scene.view_projection * world;
            output.world_position = world.xyz;
            output.world_normal = normalize((scene.normal_matrix * vec4<f32>(input.normal, 0.0)).xyz);

            output.uv = input.uv;

            output.tangent_uv = input.tangent_uv;
            output.channel = input.channel;
            output.tint = input.tint;
            return output;
        }

        fn rotated_key_direction() -> vec3<f32> {
            let cosine = cos(scene.lighting.x);
            let sine = sin(scene.lighting.x);
            let base = vec3<f32>(-0.42, 0.66, 0.61);
            return normalize(vec3<f32>(
                cosine * base.x + sine * base.z,
                base.y,
                -sine * base.x + cosine * base.z,
            ));
        }

        fn rotated_fill_direction() -> vec3<f32> {
            let cosine = cos(scene.lighting.x);
            let sine = sin(scene.lighting.x);
            let base = vec3<f32>(0.67, 0.22, 0.31);
            return normalize(vec3<f32>(
                cosine * base.x + sine * base.z,
                base.y,
                -sine * base.x + cosine * base.z,
            ));
        }

        fn lighting_exposure() -> f32 {
            return clamp(scene.lighting.y, 0.35, 2.0);
        }

        fn display_color(linear_color: vec3<f32>) -> vec3<f32> {

            return graded_display(
                linear_color,
                1.0,
                scene.grading.x > 0.5,
                scene.lighting.w > 0.5,
            );
        }

        fn key_radiance() -> vec3<f32> {
            return scene.key_light.rgb;
        }

        fn fill_radiance() -> vec3<f32> {
            return scene.fill_light.rgb;
        }

        fn environment_radiance(direction: vec3<f32>) -> vec3<f32> {
            let hemisphere = smoothstep(-0.55, 0.75, normalize(direction).y);
            return mix(scene.environment_bottom.rgb, scene.environment_top.rgb, hemisphere);
        }

        fn environment_specular_strength() -> f32 {
            return max(scene.environment_top.w, 0.0);
        }

        fn environment_grazing_strength() -> f32 {
            return max(scene.environment_bottom.w, 0.0);
        }

        fn tangent_space_normal(input: VertexOutput, encoded_xy: vec2<f32>, front_facing: bool) -> vec3<f32> {
            var normal = normalize(input.world_normal);
            if (!front_facing) { normal = -normal; }
            let position_dx = dpdx(input.world_position);
            let position_dy = dpdy(input.world_position);
            let uv_dx = dpdx(input.tangent_uv);
            let uv_dy = dpdy(input.tangent_uv);
            let determinant = uv_dx.x * uv_dy.y - uv_dx.y * uv_dy.x;

            let uv_area_scale = max(length(uv_dx) * length(uv_dy), 1.0e-20);
            if (abs(determinant) <= uv_area_scale * 1.0e-4) { return normal; }
            let tangent_vector = position_dx * uv_dy.y - position_dy * uv_dx.y;
            let bitangent_vector = -position_dx * uv_dy.x + position_dy * uv_dx.x;
            if (
                dot(tangent_vector, tangent_vector) <= 1.0e-20
                || dot(bitangent_vector, bitangent_vector) <= 1.0e-20
            ) {
                return normal;
            }
            let tangent = normalize(tangent_vector / determinant);

            let bitangent = normalize(bitangent_vector / determinant);

            let uv_footprint = max(length(uv_dx), length(uv_dy));
            let minification_strength = mix(
                1.0,
                0.22,
                smoothstep(0.0012, 0.0060, uv_footprint),
            );
            let mapped_xy = (encoded_xy * 2.0 - vec2<f32>(1.0)) * minification_strength;
            let mapped = normalize(vec3<f32>(
                mapped_xy,
                sqrt(max(1.0 - dot(mapped_xy, mapped_xy), 0.0)),
            ));
            return normalize(tangent * mapped.x + bitangent * mapped.y + normal * mapped.z);
        }

        struct PackedSurface {
            normal: vec3<f32>,
            specular_level: f32,
            roughness: f32,
        };

        fn unpack_surface(
            input: VertexOutput,
            packed: vec4<f32>,
            front_facing: bool,
        ) -> PackedSurface {
            var surface: PackedSurface;
            surface.normal = tangent_space_normal(input, packed.rg, front_facing);
            surface.specular_level = packed.b;
            surface.roughness = 1.0 - packed.a;
            return surface;
        }

        fn mouth_surface_uv(uv: vec2<f32>, tile: u32) -> vec2<f32> {
            let dimensions = vec2<f32>(textureDimensions(mouth_surface_texture));
            let tile_size = dimensions * 0.5;
            let tile_origin = vec2<f32>(f32(tile % 2u), f32(tile / 2u)) * tile_size;

            let local_texel = vec2<f32>(0.5) + clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0))
                * max(tile_size - vec2<f32>(1.0), vec2<f32>(0.0));
            return (tile_origin + local_texel) / dimensions;
        }

        fn distribution_ggx(normal: vec3<f32>, halfway: vec3<f32>, roughness: f32) -> f32 {
            let alpha = roughness * roughness;
            let alpha2 = alpha * alpha;
            let n_dot_h = max(dot(normal, halfway), 0.0);
            let denominator = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
            return alpha2 / max(3.14159265 * denominator * denominator, 0.0001);
        }

        fn geometry_schlick_ggx(n_dot_direction: f32, roughness: f32) -> f32 {
            let k = ((roughness + 1.0) * (roughness + 1.0)) / 8.0;
            return n_dot_direction / max(n_dot_direction * (1.0 - k) + k, 0.0001);
        }

        fn fresnel_schlick(cosine: f32, base_reflectance: vec3<f32>) -> vec3<f32> {
            return base_reflectance + (vec3<f32>(1.0) - base_reflectance) * pow(1.0 - cosine, 5.0);
        }

        fn dielectric_f0(specular_level: f32) -> vec3<f32> {
            return vec3<f32>(0.018 + clamp(specular_level, 0.0, 1.0) * 0.07);
        }

        const SKIN_SCATTER: f32 = 0.36;

        fn material_scatter(channel: u32) -> f32 {
            if (channel == 0u || channel == 1u) { return SKIN_SCATTER; }

            if (channel == 9u || channel == 11u || channel == 12u) { return SKIN_SCATTER; }
            return 0.0;
        }

        const SKIN_SCATTER_TINT: vec3<f32> = vec3<f32>(1.0, 0.38, 0.26);

        const MAX_CLEAR_SHELL_COVERAGE: f32 = 0.86;

        const EYE_CATCHLIGHT_GAIN: f32 = 3.2;

        const EYE_LAMP_COS_INNER: f32 = 0.98902;
        const EYE_LAMP_COS_OUTER: f32 = 0.98629;
        const EYE_SPARKLE_COS_INNER: f32 = 0.99731;
        const EYE_SPARKLE_COS_OUTER: f32 = 0.99649;

        fn eye_lamp_direction(base: vec3<f32>, view_direction: vec3<f32>) -> vec3<f32> {
            return normalize(
                base * 0.55 + view_direction * 0.45 + vec3<f32>(0.0, -0.35, 0.0),
            );
        }

        fn lamp_disk(cos_angle: f32, cos_outer: f32, cos_inner: f32) -> f32 {
            return smoothstep(cos_outer, cos_inner, cos_angle);
        }

        const EYE_MIRROR_GAIN: f32 = 1.5;

        fn pbr_light(
            normal: vec3<f32>,
            view_direction: vec3<f32>,
            light_direction: vec3<f32>,
            albedo: vec3<f32>,
            roughness: f32,
            specular_level: f32,
            radiance: vec3<f32>,

            scatter: f32,
        ) -> vec3<f32> {
            let halfway = normalize(view_direction + light_direction);
            let facing = dot(normal, light_direction);
            let n_dot_l = max(facing, 0.0);
            let n_dot_v = max(dot(normal, view_direction), 0.0);
            let distribution = distribution_ggx(normal, halfway, roughness);
            let geometry = geometry_schlick_ggx(n_dot_v, roughness) *
                geometry_schlick_ggx(n_dot_l, roughness);
            let f0 = dielectric_f0(specular_level);
            let fresnel = fresnel_schlick(max(dot(halfway, view_direction), 0.0), f0);
            var specular = distribution * geometry * fresnel / max(4.0 * n_dot_v * n_dot_l, 0.0001);

            if (scatter > 0.0) {
                let sheen_roughness = clamp(roughness * 2.4, 0.35, 1.0);
                let sheen = distribution_ggx(normal, halfway, sheen_roughness)
                    * geometry_schlick_ggx(n_dot_v, sheen_roughness)
                    * geometry_schlick_ggx(n_dot_l, sheen_roughness);
                specular = specular
                    + sheen * fresnel * 0.22 / max(4.0 * n_dot_v * n_dot_l, 0.0001);
            }
            let diffuse = (vec3<f32>(1.0) - fresnel) * albedo / 3.14159265;

            let wrapped = clamp((facing + scatter) / (1.0 + scatter), 0.0, 1.0);
            let scattered = max(wrapped - n_dot_l, 0.0);
            let diffuse_response = vec3<f32>(n_dot_l) + scattered * SKIN_SCATTER_TINT;

            return diffuse * radiance * diffuse_response + specular * radiance * n_dot_l;
        }

        fn environment_specular(
            normal: vec3<f32>,
            view_direction: vec3<f32>,
            roughness: f32,
            specular_level: f32,
        ) -> vec3<f32> {
            let n_dot_v = max(dot(normal, view_direction), 0.0);
            let smoothness = 1.0 - clamp(roughness, 0.0, 1.0);
            let f0 = dielectric_f0(specular_level);
            let grazing_limit = max(vec3<f32>(smoothness), f0);
            let fresnel = f0 + (grazing_limit - f0) * pow(1.0 - n_dot_v, 5.0);
            let lobe_visibility = 0.12
                + smoothness * smoothness * environment_specular_strength();
            let reflection_direction = reflect(-view_direction, normal);
            let grazing_boost = 1.0
                + environment_grazing_strength() * pow(1.0 - n_dot_v, 2.0);
            return environment_radiance(reflection_direction)
                * fresnel * lobe_visibility * grazing_boost * lighting_exposure();
        }

        fn material_environment_fill(
            channel: u32,
            base_color: vec3<f32>,
            normal: vec3<f32>,
        ) -> vec3<f32> {
            let environment = environment_radiance(normal);
            var channel_scale = vec3<f32>(1.0);
            if (channel == 2u) {
                channel_scale = vec3<f32>(1.60, 1.54, 1.45);
            } else if (channel == 3u) {
                channel_scale = vec3<f32>(1.12, 1.10, 1.06);
            } else if (channel == 4u) {
                channel_scale = vec3<f32>(0.48, 0.48, 0.47);
            } else if (channel == 7u) {
                channel_scale = vec3<f32>(1.22, 1.20, 1.16);
            }
            return base_color * environment * channel_scale * lighting_exposure();
        }

        @fragment
        fn fs_skin_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
            if (input.channel == 5u || input.channel == 6u || input.channel == 8u) {
                discard;
            }
            var albedo: vec4<f32>;
            var output_alpha = 1.0;
            if (input.channel == 0u) {
                albedo = textureSample(face_texture, skin_sampler, input.uv);
            } else if (input.channel == 1u) {
                albedo = textureSample(torso_texture, skin_sampler, input.uv);
            } else if (input.channel == 2u) {
                albedo = textureSample(sclera_texture, skin_sampler, input.uv);
            } else if (input.channel == 3u) {
                albedo = textureSample(iris_texture, skin_sampler, input.uv);
            } else if (input.channel == 4u) {

                albedo = textureSample(sclera_texture, skin_sampler, input.uv);
            } else if (input.channel == 7u) {
                albedo = textureSample(lacrimal_texture, skin_sampler, input.uv);
            } else if (input.channel == 9u) {
                albedo = textureSample(inner_mouth_texture, skin_sampler, input.uv);
            } else if (input.channel == 10u) {
                albedo = textureSample(teeth_texture, skin_sampler, input.uv);
            } else if (input.channel == 11u) {
                albedo = textureSample(gums_texture, skin_sampler, input.uv);
            } else if (input.channel == 12u) {
                albedo = textureSample(tongue_texture, skin_sampler, input.uv);
            } else if (input.channel == 13u) {
                albedo = textureSample(eyelashes_texture, skin_sampler, input.uv);
                if (albedo.a < 0.25) { discard; }
                output_alpha = albedo.a;
            } else {
                albedo = vec4<f32>(srgb_to_linear(vec3<f32>(0.45, 0.34, 0.30)), 1.0);
            }
            let base_color = albedo.rgb * input.tint.rgb;
            var roughness = 0.42;
            var specular_level = 0.35;
            var normal = normalize(input.world_normal);
            if (!front_facing) { normal = -normal; }
            if (input.channel == 0u) {
                let surface = unpack_surface(
                    input,
                    textureSample(face_surface_texture, skin_sampler, input.uv),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 1u) {
                let surface = unpack_surface(
                    input,
                    textureSample(torso_surface_texture, skin_sampler, input.uv),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 2u) {
                let surface = unpack_surface(
                    input,
                    textureSample(sclera_surface_texture, skin_sampler, input.uv),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 3u) {
                let surface = unpack_surface(
                    input,
                    textureSample(iris_surface_texture, skin_sampler, input.uv),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 4u) {
                roughness = 0.28;
                specular_level = 0.30;
            } else if (input.channel == 7u) {
                let surface = unpack_surface(
                    input,
                    textureSample(lacrimal_surface_texture, skin_sampler, input.uv),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 9u) {
                let surface = unpack_surface(
                    input,
                    textureSample(mouth_surface_texture, skin_sampler, mouth_surface_uv(input.uv, 0u)),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 10u) {
                let surface = unpack_surface(
                    input,
                    textureSample(mouth_surface_texture, skin_sampler, mouth_surface_uv(input.uv, 1u)),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 11u) {
                let surface = unpack_surface(
                    input,
                    textureSample(mouth_surface_texture, skin_sampler, mouth_surface_uv(input.uv, 2u)),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 12u) {
                let surface = unpack_surface(
                    input,
                    textureSample(mouth_surface_texture, skin_sampler, mouth_surface_uv(input.uv, 3u)),
                    front_facing,
                );
                normal = surface.normal;
                specular_level = surface.specular_level;
                roughness = surface.roughness;
            } else if (input.channel == 13u) {
                roughness = 0.55;
                specular_level = 0.15;
            }
            roughness = clamp(roughness, 0.08, 0.95);
            let view_direction = normalize(scene.eye.xyz - input.world_position);
            let scatter = material_scatter(input.channel);
            let key = pbr_light(
                normal,
                view_direction,
                rotated_key_direction(),
                base_color,
                roughness,
                specular_level,
                key_radiance() * lighting_exposure(),
                scatter,
            );
            let fill = pbr_light(
                normal,
                view_direction,
                rotated_fill_direction(),
                base_color,
                min(roughness + 0.08, 1.0),
                specular_level,
                fill_radiance() * lighting_exposure(),
                scatter,
            );

            var punctual = vec3<f32>(0.0);
            let lights = punctual_count();
            for (var index = 0u; index < lights; index = index + 1u) {
                let sampled = punctual_sample(index, input.world_position);
                punctual = punctual + pbr_light(
                    normal,
                    view_direction,
                    sampled.direction,
                    base_color,
                    roughness,
                    specular_level,
                    sampled.radiance * lighting_exposure(),
                    scatter,
                );
            }
            let ambient = material_environment_fill(input.channel, base_color, normal);
            let indirect_specular = environment_specular(
                normal,
                view_direction,
                roughness,
                specular_level,
            );
            return vec4<f32>(
                display_color(ambient + key + fill + punctual + indirect_specular),
                output_alpha,
            );
        }

        @fragment
        fn fs_skin_transparent(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {

            if (!front_facing) { return vec4<f32>(0.0); }
            let view_direction = normalize(scene.eye.xyz - input.world_position);
            var normal = normalize(input.world_normal);
            let n_dot_v = max(dot(normal, view_direction), 0.0);
            let reflection_direction = reflect(-view_direction, normal);

            var f0 = 0.025;
            var catchlight_gain = EYE_CATCHLIGHT_GAIN;
            var lamp_inner = EYE_LAMP_COS_INNER;
            var lamp_outer = EYE_LAMP_COS_OUTER;
            var tint = vec3<f32>(1.0, 1.0, 1.0);
            if (input.channel == 6u) {
                f0 = 0.020;
                catchlight_gain = 0.0;
            } else if (input.channel == 8u) {
                f0 = 0.020;
                catchlight_gain = EYE_CATCHLIGHT_GAIN * 0.6;
                lamp_inner = EYE_SPARKLE_COS_INNER;
                lamp_outer = EYE_SPARKLE_COS_OUTER;
                tint = vec3<f32>(0.92, 0.97, 1.0);
            } else if (input.channel != 5u) {

                return vec4<f32>(0.0, 0.0, 0.0, 0.0);
            }

            let key_lamp = eye_lamp_direction(rotated_key_direction(), view_direction);
            var mirrored = key_radiance()
                * lamp_disk(dot(reflection_direction, key_lamp), lamp_outer, lamp_inner)
                + fill_radiance()
                * lamp_disk(dot(reflection_direction, rotated_fill_direction()), EYE_SPARKLE_COS_OUTER, EYE_SPARKLE_COS_INNER)
                * 0.35;
            let lights = punctual_count();
            for (var index = 0u; index < lights; index = index + 1u) {
                let sampled = punctual_sample(index, input.world_position);
                mirrored = mirrored + sampled.radiance
                    * lamp_disk(dot(reflection_direction, sampled.direction), lamp_outer, lamp_inner);
            }
            mirrored = mirrored * catchlight_gain * lighting_exposure();

            let environment = environment_radiance(reflection_direction)
                * lighting_exposure()
                * EYE_MIRROR_GAIN;

            let fresnel = f0 + (1.0 - f0) * pow(1.0 - n_dot_v, 5.0);
            let highlight = clamp(
                max(mirrored.r, max(mirrored.g, mirrored.b)),
                0.0,
                1.0,
            );
            let coverage = min(fresnel + highlight, MAX_CLEAR_SHELL_COVERAGE);
            let reflected = (environment * fresnel + mirrored) * tint / max(coverage, 1.0e-4);
            return vec4<f32>(
                display_color(clamp(reflected, vec3<f32>(0.0), vec3<f32>(8.0))),
                coverage,
            );
        }
"#
        )
    };
}

const SKIN_SHADER: &str = skin_shader_source!(crate::shader_color::color_grading_wgsl!());
const SKIN_SHADER_HDR: &str = skin_shader_source!(crate::shader_color::color_grading_hdr_wgsl!());

const DEPTH_RESET_SHADER: &str = r#"
struct DepthResetOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_depth_reset(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[vertex_index], 1.0, 1.0);
}

@fragment
fn fs_depth_reset() -> DepthResetOutput {
    var output: DepthResetOutput;
    output.depth = 1.0;
    output.color = vec4<f32>(0.0);
    return output;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SkinRenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    tangent_uv: [f32; 2],
    channel: u32,
    tint: [f32; 4],
}

impl SkinRenderVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x2, 4 => Uint32, 5 => Float32x4];

    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

impl RenderVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

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
pub enum RenderStyle {
    #[default]
    Solid,
    Wire,
    Xray,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MeshPassKind {
    SolidOpaque,
    TranslucentDepthPrepass,
    TranslucentColor,
    Wire,
    Xray,
}

const fn mesh_pass_sequence(style: RenderStyle, translucent: bool) -> &'static [MeshPassKind] {
    match (style, translucent) {
        (RenderStyle::Solid, false) => &[MeshPassKind::SolidOpaque],
        (RenderStyle::Solid, true) => &[
            MeshPassKind::TranslucentDepthPrepass,
            MeshPassKind::TranslucentColor,
        ],
        (RenderStyle::Wire, _) => &[MeshPassKind::Wire],
        (RenderStyle::Xray, _) => &[MeshPassKind::Xray],
    }
}

fn mesh_color_is_translucent(color: [f32; 4]) -> bool {
    color[3] < 1.0
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

#[derive(Clone)]
pub struct MeshPaintCallback {
    pub scene_key: u64,
    pub mesh: Arc<SurfaceMesh>,
    pub view_projection: Mat4,
    pub model: Mat4,
    pub eye: Vec3,
    pub color: [f32; 4],
    pub style: RenderStyle,
    pub depth_scope: RenderDepthScope,

    pub light_yaw_radians: f32,
    pub light_preset: LightingPreset,

    pub frame_radius: f32,
    pub light_brightness: f32,

    pub tone_mapping: crate::shader_color::ToneMapping,

    pub smooth_passes: u8,
}

impl MeshPaintCallback {
    pub fn paint_callback(self, rect: Rect) -> epaint::PaintCallback {
        Callback::new_paint_callback(rect, self)
    }
}

impl CallbackTrait for MeshPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(resources) = callback_resources.get_mut::<MeshRenderResources>() else {
            return Vec::new();
        };
        resources.prepare_scene(device, queue, self);
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<MeshRenderResources>() else {
            return;
        };
        if self.depth_scope.resets_before_draw() {
            resources.reset_depth(render_pass);
        }
        resources.paint(
            render_pass,
            self.scene_key,
            self.style,
            mesh_color_is_translucent(self.color),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SkinVisibilityGroup {
    HeadSkin = 1 << 0,
    Eyes = 1 << 1,
    Tear = 1 << 2,
    TeethTongue = 1 << 3,
    Eyelashes = 1 << 4,
    InnerMouth = 1 << 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkinVisibilityGroups(u8);

impl SkinVisibilityGroups {
    pub const NONE: Self = Self(0);
    #[cfg(test)]
    pub const HEAD_SKIN: Self = Self(SkinVisibilityGroup::HeadSkin as u8);
    #[cfg(test)]
    pub const EYES: Self = Self(SkinVisibilityGroup::Eyes as u8);
    #[cfg(test)]
    pub const EYELASHES: Self = Self(SkinVisibilityGroup::Eyelashes as u8);
    pub const ALL: Self = Self(
        SkinVisibilityGroup::HeadSkin as u8
            | SkinVisibilityGroup::Eyes as u8
            | SkinVisibilityGroup::Tear as u8
            | SkinVisibilityGroup::TeethTongue as u8
            | SkinVisibilityGroup::Eyelashes as u8
            | SkinVisibilityGroup::InnerMouth as u8,
    );

    pub const fn contains(self, group: SkinVisibilityGroup) -> bool {
        self.0 & group as u8 != 0
    }

    pub const fn with(self, group: SkinVisibilityGroup) -> Self {
        Self(self.0 | group as u8)
    }

    pub fn channel_mask(self) -> u32 {
        let mut mask = 0;
        if self.contains(SkinVisibilityGroup::HeadSkin) {
            mask |= channel_bit(SkinChannel::Face) | channel_bit(SkinChannel::Torso);
        }
        if self.contains(SkinVisibilityGroup::Eyes) {
            for channel in [
                SkinChannel::Sclera,
                SkinChannel::Iris,
                SkinChannel::Pupil,
                SkinChannel::Cornea,
                SkinChannel::EyeReflection,
            ] {
                mask |= channel_bit(channel);
            }
        }
        if self.contains(SkinVisibilityGroup::Tear) {
            mask |= channel_bit(SkinChannel::Lacrimal) | channel_bit(SkinChannel::Tear);
        }
        if self.contains(SkinVisibilityGroup::InnerMouth) {
            mask |= channel_bit(SkinChannel::InnerMouth);
        }
        if self.contains(SkinVisibilityGroup::TeethTongue) {
            for channel in [SkinChannel::Teeth, SkinChannel::Gums, SkinChannel::Tongue] {
                mask |= channel_bit(channel);
            }
        }
        if self.contains(SkinVisibilityGroup::Eyelashes) {
            mask |= channel_bit(SkinChannel::Eyelashes);
        }
        mask
    }
}

impl Default for SkinVisibilityGroups {
    fn default() -> Self {
        Self::ALL
    }
}

#[derive(Clone)]
pub struct SkinPaintCallback {
    pub scene_key: u64,
    pub mesh: Arc<SurfaceMesh>,
    pub skin: Arc<SkinPreview>,
    pub view_projection: Mat4,
    pub model: Mat4,
    pub eye: Vec3,
    pub light_yaw_radians: f32,
    pub light_preset: LightingPreset,

    pub frame_radius: f32,
    pub light_brightness: f32,

    pub tone_mapping: crate::shader_color::ToneMapping,

    pub depth_scope: RenderDepthScope,
    pub skin_visibility: SkinVisibilityGroups,
    pub show_tear_lacrimals: bool,
    pub show_eyelashes: bool,

    pub smooth_passes: u8,
}

impl SkinPaintCallback {
    pub fn paint_callback(self, rect: Rect) -> epaint::PaintCallback {
        Callback::new_paint_callback(rect, self)
    }
}

impl CallbackTrait for SkinPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(resources) = callback_resources.get_mut::<SkinRenderResources>() else {
            return Vec::new();
        };
        resources.prepare_scene(device, queue, self);

        let kept = resources.scenes.contains_key(&self.scene_key);

        if kept && let Some(bloom) = callback_resources.get_mut::<crate::bloom::BloomResources>() {
            bloom.record(crate::bloom::HdrDraw::Skin(self.scene_key));
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if self.depth_scope.resets_before_draw()
            && let Some(mesh_resources) = callback_resources.get::<MeshRenderResources>()
        {
            mesh_resources.reset_depth(render_pass);
        }
        if let Some(resources) = callback_resources.get::<SkinRenderResources>() {
            resources.paint(render_pass, self.scene_key, SceneTarget::Screen);
        }
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
        &render_state.queue,
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

const MESH_SCENE_CACHE_CAP: usize = 64;

const SKIN_SCENE_CACHE_CAP: usize = 12;

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

fn fill_render_vertices(
    scratch: &mut Vec<RenderVertex>,
    normal_scratch: &mut Vec<Vec3>,
    smoothing_cache: &mut SmoothedPositionCache,
    mesh: &SurfaceMesh,
    smooth_passes: u8,
) {
    scratch.clear();
    if smooth_passes == 0 {
        scratch.extend(mesh.mesh.vertices.iter().zip(mesh.normals.iter()).map(
            |(position, &normal)| RenderVertex {
                position: [position[0] as f32, position[1] as f32, position[2] as f32],
                normal,
            },
        ));
        return;
    }
    let positions = smoothing_cache
        .positions(mesh, smooth_passes)
        .unwrap_or(mesh.mesh.vertices.as_slice());
    fill_render_normals(normal_scratch, positions, &mesh.render_triangles);
    scratch.extend(
        positions
            .iter()
            .zip(normal_scratch.iter())
            .map(|(position, &normal)| RenderVertex {
                position: [position[0] as f32, position[1] as f32, position[2] as f32],
                normal: normal.to_array(),
            }),
    );
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

fn fill_render_normals(scratch: &mut Vec<Vec3>, positions: &[[f64; 3]], triangles: &[[u32; 3]]) {
    scratch.clear();
    scratch.resize(positions.len(), Vec3::ZERO);
    for &[a, b, c] in triangles {
        let Some((&a_position, &b_position, &c_position)) = positions
            .get(a as usize)
            .zip(positions.get(b as usize))
            .zip(positions.get(c as usize))
            .map(|((a, b), c)| (a, b, c))
        else {
            continue;
        };
        let a_position = Vec3::from_array(a_position.map(|axis| axis as f32));
        let b_position = Vec3::from_array(b_position.map(|axis| axis as f32));
        let c_position = Vec3::from_array(c_position.map(|axis| axis as f32));
        let face_normal = (b_position - a_position).cross(c_position - a_position);
        if !face_normal.is_finite() || face_normal.length_squared() <= 1.0e-20 {
            continue;
        }
        scratch[a as usize] += face_normal;
        scratch[b as usize] += face_normal;
        scratch[c as usize] += face_normal;
    }
    for normal in scratch {
        *normal = normal.try_normalize().unwrap_or(Vec3::Y);
    }
}

struct GpuScene {
    mesh_revision: u64,
    topology_revision: u64,
    smooth_passes: u8,

    vertex_capacity: usize,

    last_used: u64,
    vertex_buffer: wgpu::Buffer,
    triangle_index_buffer: wgpu::Buffer,
    wire_index_buffer: wgpu::Buffer,
    triangle_index_count: u32,
    wire_index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct MeshRenderResources {
    solid_pipeline: wgpu::RenderPipeline,
    wire_pipeline: wgpu::RenderPipeline,
    xray_pipeline: wgpu::RenderPipeline,
    translucent_prepass_pipeline: wgpu::RenderPipeline,
    translucent_color_pipeline: wgpu::RenderPipeline,
    depth_reset_pipeline: wgpu::RenderPipeline,
    target_is_srgb: bool,
    bind_group_layout: wgpu::BindGroupLayout,
    scenes: BTreeMap<u64, GpuScene>,

    vertex_scratch: Vec<RenderVertex>,
    normal_scratch: Vec<Vec3>,
    smoothing_cache: SmoothedPositionCache,
    use_counter: u64,
}

struct GpuSkinScene {
    mesh_revision: u64,
    topology_revision: u64,
    vertex_key: SkinVertexKey,
    geometry_revision: u64,
    texture_key: SkinTextureKey,
    visibility_mask: u32,
    smooth_passes: u8,

    last_used: u64,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

const SKIN_TEXTURE_COUNT: usize = 16;

const COLOUR_TEXTURE_COUNT: usize = 10;

const SKIN_SAMPLER_BINDING: u32 = SKIN_TEXTURE_COUNT as u32 + 1;

struct GpuSkinTextures {
    views: [Arc<wgpu::TextureView>; SKIN_TEXTURE_COUNT],
}

const MIP_BLIT_SHADER: &str = r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {

    var out: VertexOutput;
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    out.uv = uv;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(source, source_sampler, input.uv);
}
"#;

struct MipBlit {
    layout: wgpu::BindGroupLayout,
    srgb_pipeline: wgpu::RenderPipeline,
    linear_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl MipBlit {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.skin.mip-blit.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(MIP_BLIT_SHADER)),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.skin.mip-blit.layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.skin.mip-blit.pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |format: wgpu::TextureFormat, label: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            srgb_pipeline: pipeline(
                wgpu::TextureFormat::Rgba8UnormSrgb,
                "vkit.skin.mip-blit.srgb",
            ),
            linear_pipeline: pipeline(wgpu::TextureFormat::Rgba8Unorm, "vkit.skin.mip-blit.linear"),
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vkit.skin.mip-blit.sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }),
        }
    }

    fn generate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        mip_level_count: u32,
        srgb: bool,
    ) {
        if mip_level_count <= 1 {
            return;
        }
        let level_view = |level: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vkit.skin.mip-blit.encoder"),
        });
        for level in 1..mip_level_count {
            let parent = level_view(level - 1);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vkit.skin.mip-blit.bind-group"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&parent),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let target = level_view(level);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vkit.skin.mip-blit.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if srgb {
                &self.srgb_pipeline
            } else {
                &self.linear_pipeline
            });
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
    }
}

struct CachedSkinUpload {
    view: Arc<wgpu::TextureView>,
    last_used: u64,
}

const SKIN_UPLOAD_CACHE_CAPACITY: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SkinTextureKey([u64; SKIN_TEXTURE_COUNT]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SkinVertexKey {
    orientations: [SkinUvOrientation; 8],
    auxiliary_colors: [[u8; 4]; 8],
    auxiliary_textured: [bool; 8],
}

fn skin_vertex_key(skin: &SkinPreview) -> SkinVertexKey {
    let channels = [
        SkinChannel::Sclera,
        SkinChannel::Iris,
        SkinChannel::Lacrimal,
        SkinChannel::InnerMouth,
        SkinChannel::Teeth,
        SkinChannel::Gums,
        SkinChannel::Tongue,
        SkinChannel::Eyelashes,
    ];
    SkinVertexKey {
        orientations: channels.map(|channel| skin.uv_orientation(channel)),
        auxiliary_colors: skin.auxiliary_colors,
        auxiliary_textured: skin.auxiliary_textured,
    }
}

pub(crate) struct SkinRenderResources {
    screen: SkinPipelines,

    hdr: SkinPipelines,
    target_is_srgb: bool,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    scenes: BTreeMap<u64, GpuSkinScene>,
    textures: BTreeMap<SkinTextureKey, GpuSkinTextures>,

    uploads: BTreeMap<(u64, bool), CachedSkinUpload>,

    mip_blit: MipBlit,

    vertex_scratch: Vec<SkinRenderVertex>,
    normal_scratch: Vec<Vec3>,
    smoothing_cache: SmoothedPositionCache,
    use_counter: u64,
}

#[derive(Clone, Copy, Debug)]
struct SkinSceneKeys {
    texture: SkinTextureKey,
    vertex: SkinVertexKey,
    visibility_mask: u32,
    use_stamp: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneTarget {
    Screen,

    Hdr,
}

#[derive(Clone, Copy)]
struct SkinAttachment {
    format: wgpu::TextureFormat,
    sample_count: u32,
}

struct SkinPipelines {
    opaque: wgpu::RenderPipeline,
    transparent: wgpu::RenderPipeline,
}

impl SkinRenderResources {
    fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let module = |label: &'static str, source: &'static str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
            })
        };
        let shader = module("vkit.skin.shader", SKIN_SHADER);
        let hdr_shader = module("vkit.skin.shader.hdr", SKIN_SHADER_HDR);
        let mut bind_entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<SceneUniform>() as u64),
            },
            count: None,
        }];
        bind_entries.extend((1..=SKIN_TEXTURE_COUNT as u32).map(|binding| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }
        }));
        bind_entries.push(wgpu::BindGroupLayoutEntry {
            binding: SKIN_SAMPLER_BINDING,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.skin.scene_layout"),
            entries: &bind_entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.skin.pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipelines = |format: wgpu::TextureFormat, target: SceneTarget| {
            let attachment = SkinAttachment {
                format,
                sample_count,
            };
            let module = match target {
                SceneTarget::Screen => &shader,
                SceneTarget::Hdr => &hdr_shader,
            };
            SkinPipelines {
                opaque: create_skin_pipeline(
                    device,
                    module,
                    &layout,
                    attachment,
                    depth_format,
                    false,
                ),
                transparent: create_skin_pipeline(
                    device,
                    module,
                    &layout,
                    attachment,
                    depth_format,
                    true,
                ),
            }
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vkit.skin.sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            screen: pipelines(target_format, SceneTarget::Screen),
            hdr: pipelines(crate::hdr_target::HDR_FORMAT, SceneTarget::Hdr),
            target_is_srgb: target_format.is_srgb(),
            bind_group_layout,
            sampler,
            scenes: BTreeMap::new(),
            textures: BTreeMap::new(),
            uploads: BTreeMap::new(),
            mip_blit: MipBlit::new(device),
            vertex_scratch: Vec::new(),
            normal_scratch: Vec::new(),
            smoothing_cache: SmoothedPositionCache::default(),
            use_counter: 0,
        }
    }

    fn evict_stale_skin_uploads(&mut self, frame: u64) {
        if self.uploads.len() <= SKIN_UPLOAD_CACHE_CAPACITY {
            return;
        }
        let mut stale = self
            .uploads
            .iter()
            .filter(|(_, cached)| cached.last_used != frame)
            .map(|(slot, cached)| (cached.last_used, *slot))
            .collect::<Vec<_>>();
        stale.sort_unstable();
        let excess = self.uploads.len() - SKIN_UPLOAD_CACHE_CAPACITY;
        for (_, slot) in stale.into_iter().take(excess) {
            self.uploads.remove(&slot);
        }
    }

    fn prepare_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        callback: &SkinPaintCallback,
    ) {
        let images = [
            callback.skin.face.as_ref(),
            callback.skin.torso.as_ref(),
            callback.skin.sclera.as_ref(),
            callback.skin.iris.as_ref(),
            callback.skin.lacrimal.as_ref(),
            callback.skin.inner_mouth.as_ref(),
            callback.skin.teeth.as_ref(),
            callback.skin.gums.as_ref(),
            callback.skin.tongue.as_ref(),
            callback.skin.eyelashes.as_ref(),
            callback.skin.face_surface.packed.as_ref(),
            callback.skin.torso_surface.packed.as_ref(),
            callback.skin.mouth_surface_atlas.packed.as_ref(),
            callback.skin.sclera_surface.packed.as_ref(),
            callback.skin.iris_surface.packed.as_ref(),
            callback.skin.lacrimal_surface.packed.as_ref(),
        ];
        let texture_key = SkinTextureKey(images.map(|image| image.revision));
        let vertex_key = skin_vertex_key(&callback.skin);
        if !self.textures.contains_key(&texture_key) {
            let labels = [
                "vkit.skin.face",
                "vkit.skin.torso",
                "vkit.skin.sclera",
                "vkit.skin.iris",
                "vkit.skin.lacrimal",
                "vkit.skin.inner_mouth",
                "vkit.skin.teeth",
                "vkit.skin.gums",
                "vkit.skin.tongue",
                "vkit.skin.eyelashes",
                "vkit.skin.face_surface",
                "vkit.skin.torso_surface",
                "vkit.skin.mouth_surface_atlas",
                "vkit.skin.sclera_surface",
                "vkit.skin.iris_surface",
                "vkit.skin.lacrimal_surface",
            ];

            let frame = self.use_counter;
            let views = std::array::from_fn(|index| {
                let srgb = index < COLOUR_TEXTURE_COUNT;
                let slot = (images[index].revision, srgb);
                if let Some(cached) = self.uploads.get_mut(&slot) {
                    cached.last_used = frame;
                    return Arc::clone(&cached.view);
                }
                let view = Arc::new(
                    upload_skin_texture(
                        device,
                        queue,
                        &self.mip_blit,
                        labels[index],
                        images[index],
                        srgb,
                    )
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                );
                self.uploads.insert(
                    slot,
                    CachedSkinUpload {
                        view: Arc::clone(&view),
                        last_used: frame,
                    },
                );
                view
            });
            self.evict_stale_skin_uploads(frame);

            self.textures.clear();
            self.textures.insert(texture_key, GpuSkinTextures { views });
        }
        let visibility_mask = skin_visibility_mask(
            callback.skin_visibility,
            callback.show_tear_lacrimals,
            callback.show_eyelashes,
        );
        self.use_counter = self.use_counter.wrapping_add(1);

        let structural_reuse = self.scenes.get(&callback.scene_key).is_some_and(|scene| {
            scene.geometry_revision == callback.skin.geometry.revision
                && scene.topology_revision == callback.mesh.topology_revision
                && scene.visibility_mask == visibility_mask
        });
        let vertex_refresh = structural_reuse
            && self.scenes.get(&callback.scene_key).is_some_and(|scene| {
                scene.mesh_revision != callback.mesh.revision
                    || scene.vertex_key != vertex_key
                    || scene.smooth_passes != callback.smooth_passes
            });
        let mut refreshed_in_place = structural_reuse;
        if vertex_refresh {
            refreshed_in_place = self.build_vertices(callback, visibility_mask)
                && self
                    .scenes
                    .get(&callback.scene_key)
                    .is_some_and(|scene| self.vertex_scratch.len() == scene.vertex_count as usize);
            if refreshed_in_place && let Some(scene) = self.scenes.get_mut(&callback.scene_key) {
                queue.write_buffer(
                    &scene.vertex_buffer,
                    0,
                    bytemuck::cast_slice(self.vertex_scratch.as_slice()),
                );
                scene.mesh_revision = callback.mesh.revision;
                scene.vertex_key = vertex_key;
                scene.smooth_passes = callback.smooth_passes;
            }
        }
        if refreshed_in_place
            && let Some(textures) = self.textures.get(&texture_key)
            && let Some(scene) = self.scenes.get_mut(&callback.scene_key)
            && scene.texture_key != texture_key
        {
            scene.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.sampler,
                textures,
                &scene.uniform_buffer,
            );
            scene.texture_key = texture_key;
        }
        if !refreshed_in_place {
            let built = self.build_vertices(callback, visibility_mask);
            let scene = built
                .then(|| self.textures.get(&texture_key))
                .flatten()
                .and_then(|textures| {
                    Self::upload_scene(
                        device,
                        callback,
                        &self.bind_group_layout,
                        &self.sampler,
                        textures,
                        self.vertex_scratch.as_slice(),
                        SkinSceneKeys {
                            texture: texture_key,
                            vertex: vertex_key,
                            visibility_mask,
                            use_stamp: self.use_counter,
                        },
                    )
                });
            if let Some(scene) = scene {
                self.scenes.insert(callback.scene_key, scene);
                evict_lru_scenes(
                    &mut self.scenes,
                    callback.scene_key,
                    SKIN_SCENE_CACHE_CAP,
                    |scene| scene.last_used,
                );
            } else {
                self.scenes.remove(&callback.scene_key);
            }
        }
        let use_stamp = self.use_counter;
        let Some(scene) = self.scenes.get_mut(&callback.scene_key) else {
            return;
        };
        scene.last_used = use_stamp;
        let scene = &*scene;
        let light = lighting_uniform_data(callback.light_preset, callback.frame_radius);
        let uniform = SceneUniform {
            view_projection: callback.view_projection.to_cols_array(),
            model: callback.model.to_cols_array(),
            normal_matrix: normal_matrix(callback.model).to_cols_array(),
            color: [1.0; 4],
            eye: callback.eye.extend(1.0).to_array(),
            lighting: [
                sanitized_light_yaw(callback.light_yaw_radians),
                sanitize_brightness(callback.light_brightness),
                callback.light_preset.id() as f32,
                if self.target_is_srgb { 1.0 } else { 0.0 },
            ],
            key_light: light.key_light,
            fill_light: light.fill_light,
            environment_top: light.environment_top,
            environment_bottom: light.environment_bottom,
            grading: [callback.tone_mapping.shader_flag(), 0.0, 0.0, 0.0],
            punctual_meta: light.punctual_meta,
            punctual: light.punctual,
        };
        queue.write_buffer(&scene.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn build_vertices(&mut self, callback: &SkinPaintCallback, visibility_mask: u32) -> bool {
        let Self {
            vertex_scratch,
            normal_scratch,
            smoothing_cache,
            ..
        } = self;
        let positions = if callback.smooth_passes == 0 {
            callback.mesh.mesh.vertices.as_slice()
        } else {
            smoothing_cache
                .positions(&callback.mesh, callback.smooth_passes)
                .unwrap_or(callback.mesh.mesh.vertices.as_slice())
        };
        build_skin_vertices(
            vertex_scratch,
            normal_scratch,
            positions,
            callback,
            visibility_mask,
        )
    }

    fn upload_scene(
        device: &wgpu::Device,
        callback: &SkinPaintCallback,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        textures: &GpuSkinTextures,
        vertices: &[SkinRenderVertex],
        keys: SkinSceneKeys,
    ) -> Option<GpuSkinScene> {
        let SkinSceneKeys {
            texture: texture_key,
            vertex: vertex_key,
            visibility_mask,
            use_stamp,
        } = keys;

        if vertices.is_empty() {
            return None;
        }
        let vertex_count = u32::try_from(vertices.len()).ok()?;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.skin.vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vkit.skin.scene_uniform"),
            size: std::mem::size_of::<SceneUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = Self::create_bind_group(
            device,
            bind_group_layout,
            sampler,
            textures,
            &uniform_buffer,
        );
        Some(GpuSkinScene {
            mesh_revision: callback.mesh.revision,
            topology_revision: callback.mesh.topology_revision,
            vertex_key,
            geometry_revision: callback.skin.geometry.revision,
            texture_key,
            visibility_mask,
            smooth_passes: callback.smooth_passes,
            last_used: use_stamp,
            vertex_buffer,
            vertex_count,
            uniform_buffer,
            bind_group,
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        textures: &GpuSkinTextures,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let mut bind_entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }];
        bind_entries.extend(textures.views.iter().enumerate().map(|(index, view)| {
            wgpu::BindGroupEntry {
                binding: index as u32 + 1,
                resource: wgpu::BindingResource::TextureView(view.as_ref()),
            }
        }));
        bind_entries.push(wgpu::BindGroupEntry {
            binding: SKIN_SAMPLER_BINDING,
            resource: wgpu::BindingResource::Sampler(sampler),
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.skin.bind_group"),
            layout: bind_group_layout,
            entries: &bind_entries,
        })
    }

    pub(crate) fn paint(
        &self,
        render_pass: &mut wgpu::RenderPass<'static>,
        scene_key: u64,
        target: SceneTarget,
    ) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };

        if scene.vertex_count == 0 {
            return;
        }
        let pipelines = match target {
            SceneTarget::Screen => &self.screen,
            SceneTarget::Hdr => &self.hdr,
        };
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, scene.vertex_buffer.slice(..));
        render_pass.set_pipeline(&pipelines.opaque);
        render_pass.draw(0..scene.vertex_count, 0..1);
        render_pass.set_pipeline(&pipelines.transparent);
        render_pass.draw(0..scene.vertex_count, 0..1);
    }
}

const fn channel_bit(channel: SkinChannel) -> u32 {
    1_u32 << channel as u32
}

fn build_skin_vertices(
    scratch: &mut Vec<SkinRenderVertex>,
    accumulated_normals: &mut Vec<Vec3>,
    positions: &[[f64; 3]],
    callback: &SkinPaintCallback,
    visibility_mask: u32,
) -> bool {
    scratch.clear();
    accumulated_normals.clear();
    accumulated_normals.resize(positions.len(), Vec3::ZERO);
    for triangle in callback.skin.geometry.triangles.iter() {
        if callback.mesh.editable_triangle_ids.is_empty()
            && callback
                .mesh
                .visible_triangle_ids
                .binary_search(&triangle.source_triangle_id)
                .is_err()
        {
            continue;
        }
        if visibility_mask & channel_bit(triangle.channel) == 0 {
            continue;
        }
        let positions = triangle.corners.map(|corner| {
            positions
                .get(corner.vertex_id as usize)
                .copied()
                .map(|value| Vec3::from_array(value.map(|axis| axis as f32)))
        });
        let [Some(a), Some(b), Some(c)] = positions else {
            return false;
        };
        let face_normal = (b - a).cross(c - a);
        if face_normal.is_finite() && face_normal.length_squared() > 1.0e-20 {
            for corner in triangle.corners {
                let Some(normal) = accumulated_normals.get_mut(corner.vertex_id as usize) else {
                    return false;
                };
                *normal += face_normal;
            }
        }
    }
    for normal in accumulated_normals.iter_mut() {
        *normal = normal.try_normalize().unwrap_or(Vec3::Y);
    }
    scratch.reserve(callback.skin.geometry.triangles.len() * 3);
    for triangle in callback.skin.geometry.triangles.iter() {
        if callback.mesh.editable_triangle_ids.is_empty()
            && callback
                .mesh
                .visible_triangle_ids
                .binary_search(&triangle.source_triangle_id)
                .is_err()
        {
            continue;
        }
        if visibility_mask & channel_bit(triangle.channel) == 0 {
            continue;
        }
        for corner in triangle.corners {
            let Some(position) = positions.get(corner.vertex_id as usize) else {
                return false;
            };
            let Some(normal) = accumulated_normals.get(corner.vertex_id as usize) else {
                return false;
            };
            let (uv, tangent_uv) = skin_render_uvs(
                triangle.channel,
                corner.uv,
                callback.skin.uv_orientation(triangle.channel),
            );
            scratch.push(SkinRenderVertex {
                position: position.map(|value| value as f32),
                normal: normal.to_array(),
                uv,
                tangent_uv,
                channel: triangle.channel as u32,
                tint: skin_channel_tint(&callback.skin, triangle.channel),
            });
        }
    }
    true
}

fn skin_render_uvs(
    channel: SkinChannel,
    uv: [f32; 2],
    source_orientation: SkinUvOrientation,
) -> ([f32; 2], [f32; 2]) {
    (channel.texture_uv(uv, source_orientation), uv)
}

fn skin_channel_tint(skin: &SkinPreview, channel: SkinChannel) -> [f32; 4] {
    skin_channel_tint_data(&skin.auxiliary_colors, &skin.auxiliary_textured, channel)
}

fn skin_channel_tint_data(
    auxiliary_colors: &[[u8; 4]; 8],
    auxiliary_textured: &[bool; 8],
    channel: SkinChannel,
) -> [f32; 4] {
    if channel == SkinChannel::Pupil && !auxiliary_textured[0] {
        return [0.002, 0.002, 0.002, 1.0];
    }
    let index = match channel {
        SkinChannel::Sclera | SkinChannel::Pupil => Some(0),
        SkinChannel::Iris => Some(1),
        SkinChannel::Lacrimal => Some(2),
        SkinChannel::InnerMouth => Some(3),
        SkinChannel::Teeth => Some(4),
        SkinChannel::Gums => Some(5),
        SkinChannel::Tongue => Some(6),
        SkinChannel::Eyelashes => Some(7),
        SkinChannel::Face
        | SkinChannel::Torso
        | SkinChannel::Cornea
        | SkinChannel::EyeReflection
        | SkinChannel::Tear => None,
    };
    index.map_or([1.0; 4], |index| {
        if channel != SkinChannel::Eyelashes && auxiliary_textured[index] {
            [1.0; 4]
        } else {
            rgba8_srgb_to_linear(auxiliary_colors[index])
        }
    })
}

fn skin_visibility_mask(
    groups: SkinVisibilityGroups,
    show_tear_lacrimals: bool,
    show_eyelashes: bool,
) -> u32 {
    let mut mask = groups.channel_mask();
    if !show_tear_lacrimals {
        mask &= !channel_bit(SkinChannel::Lacrimal);
        mask &= !channel_bit(SkinChannel::Tear);
    }
    if !show_eyelashes {
        mask &= !channel_bit(SkinChannel::Eyelashes);
    }
    mask
}

fn upload_skin_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mip_blit: &MipBlit,
    label: &'static str,
    image: &crate::skin_preview::SkinImage,
    srgb: bool,
) -> wgpu::Texture {
    let mip_level_count = 32 - image.width.max(image.height).max(1).leading_zeros();
    let size = wgpu::Extent3d {
        width: image.width,
        height: image.height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        image.rgba8.as_ref(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width.saturating_mul(4)),
            rows_per_image: Some(image.height),
        },
        size,
    );
    mip_blit.generate(device, queue, &texture, mip_level_count, srgb);
    texture
}

impl MeshRenderResources {
    fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.mesh.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });
        let depth_reset_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.mesh.depth_reset_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(DEPTH_RESET_SHADER)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.mesh.scene_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<SceneUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.mesh.pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let mesh_pipeline = |kind: MeshPassKind| {
            create_mesh_pipeline(
                device,
                &shader,
                &layout,
                target_format,
                sample_count,
                depth_format,
                kind,
            )
        };
        let solid_pipeline = mesh_pipeline(MeshPassKind::SolidOpaque);
        let wire_pipeline = mesh_pipeline(MeshPassKind::Wire);
        let xray_pipeline = mesh_pipeline(MeshPassKind::Xray);
        let translucent_prepass_pipeline = mesh_pipeline(MeshPassKind::TranslucentDepthPrepass);
        let translucent_color_pipeline = mesh_pipeline(MeshPassKind::TranslucentColor);
        let depth_reset_pipeline = create_depth_reset_pipeline(
            device,
            &depth_reset_shader,
            target_format,
            sample_count,
            depth_format,
        );

        Self {
            solid_pipeline,
            wire_pipeline,
            xray_pipeline,
            translucent_prepass_pipeline,
            translucent_color_pipeline,
            depth_reset_pipeline,
            target_is_srgb: target_format.is_srgb(),
            bind_group_layout,
            scenes: BTreeMap::new(),
            vertex_scratch: Vec::new(),
            normal_scratch: Vec::new(),
            smoothing_cache: SmoothedPositionCache::default(),
            use_counter: 0,
        }
    }

    fn prepare_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        callback: &MeshPaintCallback,
    ) {
        self.use_counter = self.use_counter.wrapping_add(1);
        let mesh = &callback.mesh;
        let reusable = self.scenes.get(&callback.scene_key).is_some_and(|scene| {
            scene.topology_revision == mesh.topology_revision
                && scene.vertex_capacity == mesh.mesh.vertices.len()
        });
        if !reusable {
            let scene = self.upload_mesh(device, mesh, callback.smooth_passes);
            self.scenes.insert(callback.scene_key, scene);
            evict_lru_scenes(
                &mut self.scenes,
                callback.scene_key,
                MESH_SCENE_CACHE_CAP,
                |scene| scene.last_used,
            );
        } else if self.scenes.get(&callback.scene_key).is_some_and(|scene| {
            scene.mesh_revision != mesh.revision || scene.smooth_passes != callback.smooth_passes
        }) {
            fill_render_vertices(
                &mut self.vertex_scratch,
                &mut self.normal_scratch,
                &mut self.smoothing_cache,
                mesh,
                callback.smooth_passes,
            );
            let Some(scene) = self.scenes.get_mut(&callback.scene_key) else {
                return;
            };
            queue.write_buffer(
                &scene.vertex_buffer,
                0,
                bytemuck::cast_slice(self.vertex_scratch.as_slice()),
            );
            scene.mesh_revision = mesh.revision;
            scene.smooth_passes = callback.smooth_passes;
        }
        let use_stamp = self.use_counter;
        let Some(scene) = self.scenes.get_mut(&callback.scene_key) else {
            return;
        };
        scene.last_used = use_stamp;
        let scene = &*scene;
        let light = lighting_uniform_data(callback.light_preset, callback.frame_radius);
        let uniform = SceneUniform {
            view_projection: callback.view_projection.to_cols_array(),
            model: callback.model.to_cols_array(),
            normal_matrix: normal_matrix(callback.model).to_cols_array(),
            color: rgba_srgb_to_linear(callback.color),
            eye: callback.eye.extend(1.0).to_array(),
            lighting: [
                sanitized_light_yaw(callback.light_yaw_radians),
                sanitize_brightness(callback.light_brightness),
                callback.light_preset.id() as f32,
                if self.target_is_srgb { 1.0 } else { 0.0 },
            ],
            key_light: light.key_light,
            fill_light: light.fill_light,
            environment_top: light.environment_top,
            environment_bottom: light.environment_bottom,
            grading: [callback.tone_mapping.shader_flag(), 0.0, 0.0, 0.0],
            punctual_meta: light.punctual_meta,
            punctual: light.punctual,
        };
        queue.write_buffer(&scene.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn upload_mesh(
        &mut self,
        device: &wgpu::Device,
        mesh: &SurfaceMesh,
        smooth_passes: u8,
    ) -> GpuScene {
        fill_render_vertices(
            &mut self.vertex_scratch,
            &mut self.normal_scratch,
            &mut self.smoothing_cache,
            mesh,
            smooth_passes,
        );
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.mesh.vertices"),
            contents: bytemuck::cast_slice(self.vertex_scratch.as_slice()),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let triangle_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.mesh.triangles"),
            contents: bytemuck::cast_slice(mesh.render_triangles.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        });
        let wire_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.mesh.edges"),
            contents: bytemuck::cast_slice(mesh.wire_indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vkit.mesh.scene_uniform"),
            size: std::mem::size_of::<SceneUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.mesh.scene_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        GpuScene {
            mesh_revision: mesh.revision,
            topology_revision: mesh.topology_revision,
            smooth_passes,
            vertex_capacity: mesh.mesh.vertices.len(),
            last_used: self.use_counter,
            vertex_buffer,
            triangle_index_buffer,
            wire_index_buffer,
            triangle_index_count: (mesh.render_triangles.len().saturating_mul(3))
                .min(u32::MAX as usize) as u32,
            wire_index_count: mesh.wire_indices.len().min(u32::MAX as usize) as u32,
            uniform_buffer,
            bind_group,
        }
    }

    const fn pipeline_for(&self, kind: MeshPassKind) -> &wgpu::RenderPipeline {
        match kind {
            MeshPassKind::SolidOpaque => &self.solid_pipeline,
            MeshPassKind::TranslucentDepthPrepass => &self.translucent_prepass_pipeline,
            MeshPassKind::TranslucentColor => &self.translucent_color_pipeline,
            MeshPassKind::Wire => &self.wire_pipeline,
            MeshPassKind::Xray => &self.xray_pipeline,
        }
    }

    fn paint(
        &self,
        render_pass: &mut wgpu::RenderPass<'static>,
        scene_key: u64,
        style: RenderStyle,
        translucent: bool,
    ) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };

        if scene.triangle_index_count == 0 && scene.wire_index_count == 0 {
            return;
        }
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, scene.vertex_buffer.slice(..));
        for kind in mesh_pass_sequence(style, translucent) {
            render_pass.set_pipeline(self.pipeline_for(*kind));
            match kind {
                MeshPassKind::Wire => {
                    if scene.wire_index_count == 0 {
                        continue;
                    }
                    render_pass.set_index_buffer(
                        scene.wire_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    render_pass.draw_indexed(0..scene.wire_index_count, 0, 0..1);
                }
                MeshPassKind::SolidOpaque
                | MeshPassKind::TranslucentDepthPrepass
                | MeshPassKind::TranslucentColor
                | MeshPassKind::Xray => {
                    if scene.triangle_index_count == 0 {
                        continue;
                    }
                    render_pass.set_index_buffer(
                        scene.triangle_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    render_pass.draw_indexed(0..scene.triangle_index_count, 0, 0..1);
                }
            }
        }
    }

    fn reset_depth(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        render_pass.set_pipeline(&self.depth_reset_pipeline);
        render_pass.draw(0..3, 0..1);
    }
}

struct MeshPipelineConfig {
    topology: wgpu::PrimitiveTopology,
    depth_write_enabled: bool,
    fragment_entry: &'static str,
    blend: Option<wgpu::BlendState>,
    write_mask: wgpu::ColorWrites,
    label: &'static str,
}

const fn mesh_pipeline_config(kind: MeshPassKind) -> MeshPipelineConfig {
    match kind {
        MeshPassKind::SolidOpaque => MeshPipelineConfig {
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_write_enabled: true,
            fragment_entry: "fs_main",
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
            label: "vkit.mesh.solid",
        },
        MeshPassKind::TranslucentDepthPrepass => MeshPipelineConfig {
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_write_enabled: true,
            fragment_entry: "fs_depth_only",
            blend: None,
            write_mask: wgpu::ColorWrites::empty(),
            label: "vkit.mesh.translucent_depth_prepass",
        },
        MeshPassKind::TranslucentColor => MeshPipelineConfig {
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_write_enabled: false,
            fragment_entry: "fs_main",
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
            label: "vkit.mesh.translucent_color",
        },
        MeshPassKind::Wire => MeshPipelineConfig {
            topology: wgpu::PrimitiveTopology::LineList,
            depth_write_enabled: true,
            fragment_entry: "fs_main",
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
            label: "vkit.mesh.wire",
        },
        MeshPassKind::Xray => MeshPipelineConfig {
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_write_enabled: false,
            fragment_entry: "fs_main",
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
            label: "vkit.mesh.xray",
        },
    }
}

fn create_mesh_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
    depth_format: wgpu::TextureFormat,
    kind: MeshPassKind,
) -> wgpu::RenderPipeline {
    let config = mesh_pipeline_config(kind);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(config.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[RenderVertex::layout()],
        },
        primitive: wgpu::PrimitiveState {
            topology: config.topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(config.depth_write_enabled),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(config.fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: config.blend,
                write_mask: config.write_mask,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_skin_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    attachment: SkinAttachment,
    depth_format: wgpu::TextureFormat,
    transparent: bool,
) -> wgpu::RenderPipeline {
    let SkinAttachment {
        format: target_format,
        sample_count,
    } = attachment;
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vkit.skin.solid"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_skin"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[SkinRenderVertex::layout()],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(!transparent),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: !transparent,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if transparent {
                "fs_skin_transparent"
            } else {
                "fs_skin_opaque"
            }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: transparent.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_depth_reset_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
    depth_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vkit.mesh.depth_reset_layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vkit.mesh.depth_reset"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_depth_reset"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_depth_reset"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
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
            ("ao", crate::ambient_occlusion::AO_WGSL),
            ("ao-blur", crate::ambient_occlusion::AO_BLUR_WGSL),
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
