use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use bytemuck::{Pod, Zeroable};
use egui::{Rect, epaint};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use crate::{
    hair_physics::{HairPhysicsPipelines, HairPhysicsScene},
    hair_preview::HairPreview,
    lighting::{LightingPreset, sanitize_brightness},
    renderer::{DEPTH_FORMAT, SceneTarget},
    scene::SurfaceMesh,
};

macro_rules! scalp_shader_source {
    ($grading:expr) => {
        concat!(
            $grading,
            r#"
        struct ScalpUniform {
            view_projection: mat4x4<f32>,
            model: mat4x4<f32>,
            tint: vec4<f32>,
            specular_tint: vec4<f32>,

            flags: vec4<f32>,

            map_flags: vec4<f32>,

            map_flags_2: vec4<f32>,
            eye: vec4<f32>,
            lighting: vec4<f32>,
            key_light: vec4<f32>,
            fill_light: vec4<f32>,
            environment_top: vec4<f32>,
            environment_bottom: vec4<f32>,

            grading: vec4<f32>,
        };
        @group(0) @binding(0) var<uniform> scene: ScalpUniform;
        @group(0) @binding(1) var diffuse_map: texture_2d<f32>;
        @group(0) @binding(2) var alpha_map: texture_2d<f32>;
        @group(0) @binding(3) var normal_map: texture_2d<f32>;
        @group(0) @binding(4) var specular_map: texture_2d<f32>;
        @group(0) @binding(5) var gloss_map: texture_2d<f32>;
        @group(0) @binding(6) var map_sampler: sampler;

        struct VertexInput {
            @location(0) position: vec3<f32>,
            @location(1) uv: vec2<f32>,
        };
        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) uv: vec2<f32>,
            @location(1) world_position: vec3<f32>,
        };

        @vertex
        fn vs_main(input: VertexInput) -> VertexOutput {
            var output: VertexOutput;
            let world = scene.model * vec4<f32>(input.position, 1.0);
            output.clip_position = scene.view_projection * world;
            output.uv = input.uv;
            output.world_position = world.xyz;
            return output;
        }

        fn rotated_light(base: vec3<f32>) -> vec3<f32> {
            let cosine = cos(scene.lighting.x);
            let sine = sin(scene.lighting.x);
            return normalize(vec3<f32>(
                cosine * base.x + sine * base.z,
                base.y,
                -sine * base.x + cosine * base.z,
            ));
        }

        fn environment_radiance(direction: vec3<f32>) -> vec3<f32> {
            let hemisphere = smoothstep(-0.55, 0.75, normalize(direction).y);
            return mix(scene.environment_bottom.rgb, scene.environment_top.rgb, hemisphere);
        }

        fn display_color(linear_color: vec3<f32>) -> vec3<f32> {

            return graded_display(
                linear_color,
                clamp(scene.lighting.y, 0.35, 2.0),
                scene.grading.x > 0.5,
                scene.lighting.w > 0.5,
            );
        }

        fn distribution_ggx(normal: vec3<f32>, halfway: vec3<f32>, roughness: f32) -> f32 {
            let alpha = roughness * roughness;
            let alpha2 = alpha * alpha;
            let n_dot_h = max(dot(normal, halfway), 0.0);
            let denominator = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
            return alpha2 / max(3.14159265 * denominator * denominator, 0.0001);
        }

        fn geometry_schlick(n_dot_direction: f32, roughness: f32) -> f32 {
            let k = ((roughness + 1.0) * (roughness + 1.0)) / 8.0;
            return n_dot_direction / max(n_dot_direction * (1.0 - k) + k, 0.0001);
        }

        fn pbr_light(
            normal: vec3<f32>,
            view_direction: vec3<f32>,
            light_direction: vec3<f32>,
            albedo: vec3<f32>,
            roughness: f32,
            base_reflectance: vec3<f32>,
            specular_weight: f32,
            radiance: vec3<f32>,
        ) -> vec3<f32> {
            let halfway = normalize(view_direction + light_direction);
            let n_dot_l = max(dot(normal, light_direction), 0.0);
            let n_dot_v = max(dot(normal, view_direction), 0.0);
            let fresnel = (base_reflectance + (vec3<f32>(1.0) - base_reflectance)
                * pow(1.0 - max(dot(halfway, view_direction), 0.0), 5.0))
                * specular_weight;
            let specular = distribution_ggx(normal, halfway, roughness)
                * geometry_schlick(n_dot_v, roughness)
                * geometry_schlick(n_dot_l, roughness)
                * fresnel / max(4.0 * n_dot_v * n_dot_l, 0.0001);
            return ((vec3<f32>(1.0) - fresnel) * albedo / 3.14159265 + specular)
                * radiance * n_dot_l;
        }

        fn tangent_space_normal(
            input: VertexOutput,
            encoded: vec3<f32>,
            front_facing: bool,
        ) -> vec3<f32> {
            var geometric = normalize(cross(dpdx(input.world_position), dpdy(input.world_position)));
            if (!front_facing) { geometric = -geometric; }
            let position_dx = dpdx(input.world_position);
            let position_dy = dpdy(input.world_position);
            let uv_dx = dpdx(input.uv);
            let uv_dy = dpdy(input.uv);
            let determinant = uv_dx.x * uv_dy.y - uv_dx.y * uv_dy.x;
            let uv_area_scale = max(length(uv_dx) * length(uv_dy), 1.0e-20);
            if (abs(determinant) <= uv_area_scale * 1.0e-4) { return geometric; }
            let tangent_vector = position_dx * uv_dy.y - position_dy * uv_dx.y;
            let bitangent_vector = -position_dx * uv_dy.x + position_dy * uv_dx.x;
            if (
                dot(tangent_vector, tangent_vector) <= 1.0e-20
                || dot(bitangent_vector, bitangent_vector) <= 1.0e-20
            ) {
                return geometric;
            }
            let tangent = normalize(tangent_vector / determinant);
            let bitangent = normalize(bitangent_vector / determinant);
            let tangent_normal = normalize(encoded * 2.0 - vec3<f32>(1.0));
            return normalize(
                tangent * tangent_normal.x
                + bitangent * tangent_normal.y
                + geometric * tangent_normal.z,
            );
        }

        fn map_luminance(color: vec3<f32>) -> f32 {
            return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
        }

        @fragment
        fn fs_main(
            input: VertexOutput,
            @builtin(front_facing) front_facing: bool,
        ) -> @location(0) vec4<f32> {
            var albedo = srgb_to_linear(scene.tint.rgb);
            if (scene.map_flags.x > 0.5) {
                albedo = albedo * textureSample(diffuse_map, map_sampler, input.uv).rgb;
            }
            var coverage = scene.tint.a;
            if (scene.map_flags.y > 0.5) {

                let sampled = textureSample(alpha_map, map_sampler, input.uv);
                coverage = coverage * min(sampled.a, min(sampled.r, min(sampled.g, sampled.b)));
            }
            coverage = clamp(coverage + scene.map_flags_2.y, 0.0, 1.0);
            if (coverage < scene.flags.x) {
                discard;
            }
            var normal = normalize(cross(dpdx(input.world_position), dpdy(input.world_position)));
            if (!front_facing) { normal = -normal; }
            if (scene.map_flags.z > 0.5) {
                normal = tangent_space_normal(
                    input,
                    textureSample(normal_map, map_sampler, input.uv).rgb,
                    front_facing,
                );
            }
            let view_direction = normalize(scene.eye.xyz - input.world_position);
            var roughness = clamp(scene.flags.y, 0.08, 0.95);
            if (scene.map_flags_2.x > 0.5) {
                let gloss = map_luminance(textureSample(gloss_map, map_sampler, input.uv).rgb);
                roughness = clamp(roughness + (0.5 - gloss) * 0.55, 0.08, 0.95);
            }
            var specular_level = 1.0;
            if (scene.map_flags.w > 0.5) {
                specular_level = map_luminance(textureSample(specular_map, map_sampler, input.uv).rgb);
            }
            let base_reflectance = clamp(
                srgb_to_linear(scene.specular_tint.rgb),
                vec3<f32>(0.0),
                vec3<f32>(0.85),
            );
            let specular_weight = clamp(scene.flags.z * specular_level, 0.0, 4.0);
            let key = pbr_light(
                normal,
                view_direction,
                rotated_light(vec3<f32>(-0.42, 0.66, 0.61)),
                albedo,
                roughness,
                base_reflectance,
                specular_weight,
                scene.key_light.rgb,
            );
            let fill = pbr_light(
                normal,
                view_direction,
                rotated_light(vec3<f32>(0.67, 0.22, 0.31)),
                albedo,
                min(roughness + 0.08, 1.0),
                base_reflectance,
                specular_weight,
                scene.fill_light.rgb,
            );
            let reflection_direction = reflect(-view_direction, normal);
            let n_dot_v = max(dot(normal, view_direction), 0.0);
            let environment_fresnel = (base_reflectance
                + (vec3<f32>(1.0) - base_reflectance)
                    * pow(1.0 - n_dot_v, 5.0) * scene.flags.w)
                * specular_weight;
            let environment_specular = environment_radiance(reflection_direction)
                * environment_fresnel
                * (0.08 + (1.0 - roughness) * scene.environment_top.w);
            let ambient = environment_radiance(normal) * albedo * 0.34;
            return vec4<f32>(
                display_color(ambient + key + fill + environment_specular),
                coverage,
            );
        }
"#
        )
    };
}

pub(crate) const SCALP_SHADER: &str =
    scalp_shader_source!(crate::shader_color::color_grading_wgsl!());
pub(crate) const SCALP_SHADER_HDR: &str =
    scalp_shader_source!(crate::shader_color::color_grading_hdr_wgsl!());

macro_rules! hair_shader_source {
    ($grading:expr) => {
        concat!(
            $grading,
            r#"
        struct HairUniform {
            view_projection: mat4x4<f32>,
            model: mat4x4<f32>,
            eye: vec4<f32>,

            lighting: vec4<f32>,
            key_light: vec4<f32>,
            fill_light: vec4<f32>,

            environment_top: vec4<f32>,
            environment_bottom: vec4<f32>,

            grading: vec4<f32>,
        };
        @group(0) @binding(0) var<uniform> scene: HairUniform;

        const MIN_STRAND_HALF_PIXELS: f32 = 0.35;

        const MAX_STRAND_WIDENING: f32 = 8.0;

        fn ribbon_half_pixels(
            world_centre: vec3<f32>,
            offset: vec3<f32>,
            viewport_pixels: vec2<f32>,
        ) -> f32 {
            let centre = scene.view_projection * vec4<f32>(world_centre, 1.0);
            let edge = scene.view_projection * vec4<f32>(world_centre + offset, 1.0);
            if (centre.w <= 1.0e-6 || edge.w <= 1.0e-6) {
                return 0.0;
            }
            let delta = edge.xy / edge.w - centre.xy / centre.w;
            return length(delta * 0.5 * viewport_pixels);
        }

        struct Particle {
            position: vec4<f32>,
            previous: vec4<f32>,

            inner: vec4<f32>,
            velocity: vec4<f32>,
        };
        struct RenderSegment {

            particles: vec4<u32>,

            weights: vec4<f32>,
        };
        struct RenderPart {

            root_color: vec4<f32>,

            tip_color: vec4<f32>,

            specular: vec4<f32>,

            lobes: vec4<f32>,

            variation: vec4<f32>,

            width: vec4<f32>,

            waviness_a: vec4<f32>,

            waviness_b: vec4<f32>,

            waviness_c: vec4<f32>,

            spread_a: vec4<f32>,

            spread_b: vec4<f32>,

            lengths: vec4<f32>,
        };
        @group(0) @binding(1) var<storage, read> particles: array<Particle>;
        @group(0) @binding(2) var<storage, read> segments: array<RenderSegment>;
        @group(0) @binding(3) var<storage, read> parts: array<RenderPart>;

        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) world_position: vec3<f32>,
            @location(1) world_tangent: vec3<f32>,
            @location(2) strand_t: f32,
            @location(3) ribbon_side: f32,
            @location(4) @interpolate(flat) part_index: u32,
            @location(5) @interpolate(flat) strand_noise: f32,

            @location(6) @interpolate(flat) half_pixels: f32,
        };

        fn max_render_subdivisions() -> u32 {
            return max(u32(scene.grading.w), 1u);
        }

        fn catmull_position(
            before: vec3<f32>,
            start: vec3<f32>,
            end: vec3<f32>,
            after: vec3<f32>,
            f: f32,
        ) -> vec3<f32> {
            return 0.5
                * (2.0 * start
                    + (end - before) * f
                    + (2.0 * before - 5.0 * start + 4.0 * end - after) * f * f
                    + (3.0 * start - before - 3.0 * end + after) * f * f * f);
        }

        fn catmull_tangent(
            before: vec3<f32>,
            start: vec3<f32>,
            end: vec3<f32>,
            after: vec3<f32>,
            f: f32,
        ) -> vec3<f32> {
            return 0.5
                * ((end - before)
                    + 2.0 * (2.0 * before - 5.0 * start + 4.0 * end - after) * f
                    + 3.0 * (3.0 * start - before - 3.0 * end + after) * f * f);
        }

        fn hash_u32(value: u32) -> u32 {
            var hash = value;
            hash = (hash ^ 61u) ^ (hash >> 16u);
            hash = hash * 9u;
            hash = hash ^ (hash >> 4u);
            hash = hash * 0x27d4eb2du;
            return hash ^ (hash >> 15u);
        }

        fn strand_random(segment: RenderSegment, segment_count: u32) -> f32 {
            let local_segment = u32(round(segment.weights.w * f32(segment_count)));
            let roots = segment.particles.xyz - vec3<u32>(local_segment);
            let weights = vec3<u32>(
                u32(clamp(segment.weights.x, 0.0, 1.0) * 65535.0),
                u32(clamp(segment.weights.y, 0.0, 1.0) * 65535.0),
                u32(clamp(segment.weights.z, 0.0, 1.0) * 65535.0),
            );
            let seed = roots.x
                ^ (roots.y * 0x9e3779b9u)
                ^ (roots.z * 0x85ebca6bu)
                ^ (weights.x * 0xc2b2ae35u)
                ^ (weights.y * 0x27d4eb2du)
                ^ (weights.z * 0x165667b1u);
            return f32(hash_u32(seed) & 0x00ffffffu) / 16777215.0;
        }

        fn waviness_envelope(t: f32, part: RenderPart) -> f32 {
            let midpoint = max(part.waviness_c.w, 0.001);
            if (t <= midpoint) {
                let eased = pow(1.0 - t / midpoint, part.waviness_b.w);
                return mix(part.waviness_c.y, part.waviness_c.x, eased);
            }
            let eased = pow((t - midpoint) / max(1.0 - midpoint, 0.001), part.waviness_b.w);
            return mix(part.waviness_c.y, part.waviness_c.z, eased);
        }

        fn waviness_offset(t: f32, part: RenderPart, strand_rand: f32) -> vec3<f32> {
            let amplitude = part.waviness_a.w;
            if (amplitude <= 0.0) {
                return vec3<f32>(0.0);
            }
            let r_scale = fract(strand_rand * 61.8034);
            let r_frequency = fract(strand_rand * 137.036);
            let r_direction = fract(strand_rand * 261.8034);
            let amp_random = max(0.0, 1.0 + part.waviness_b.y * (r_scale - 0.5));
            let freq_random = max(0.0, 1.0 + part.waviness_b.z * (r_frequency - 0.5));
            let flags = u32(part.width.w);
            var axis = part.waviness_a.xyz;
            if ((flags & 2u) != 0u && r_direction > 0.5) {
                axis = -axis;
            }
            var direction = 1.0;
            if ((flags & 1u) != 0u && fract(r_direction * 2.0) < 0.5) {
                direction = -1.0;
            }

            let phase = t * part.waviness_b.x * freq_random * direction * 6.2831853;
            return axis * (sin(phase) * amplitude * amp_random * waviness_envelope(t, part));
        }

        fn guide_random(root: u32) -> f32 {
            return f32(hash_u32(root * 0x9e3779b9u + 0x7f4a7c15u) & 0x00ffffffu) / 16777215.0;
        }

        fn guide_blend(
            roots: vec3<i32>,
            weights: vec3<f32>,
            point: i32,
            segment_count: u32,
            part: RenderPart,
        ) -> vec3<f32> {
            let index = clamp(point, 0, i32(segment_count));
            let t = f32(index) / f32(segment_count);
            let x = u32(roots.x + index);
            let y = u32(roots.y + index);
            let z = u32(roots.z + index);
            return (particles[x].position.xyz
                    + waviness_offset(t, part, guide_random(u32(roots.x))))
                    * weights.x
                + (particles[y].position.xyz
                    + waviness_offset(t, part, guide_random(u32(roots.y))))
                    * weights.y
                + (particles[z].position.xyz
                    + waviness_offset(t, part, guide_random(u32(roots.z))))
                    * weights.z;
        }

        fn spread_deviation(t: f32, part: RenderPart) -> f32 {
            let midpoint = max(part.spread_a.w, 0.001);
            var followed: f32;
            if (t <= midpoint) {
                let eased = pow(1.0 - t / midpoint, part.spread_b.x);
                followed = mix(part.spread_a.y, part.spread_a.x, eased);
            } else {
                let eased = pow((t - midpoint) / max(1.0 - midpoint, 0.001), part.spread_b.x);
                followed = mix(part.spread_a.y, part.spread_a.z, eased);
            }
            return clamp(1.0 - followed, 0.0, 1.0);
        }

        fn leaned_weights(seed: f32) -> vec3<f32> {
            let pick = hash_u32(bitcast<u32>(seed * 8191.0 + 7.0)) % 3u;
            if (pick == 0u) { return vec3<f32>(1.0, 0.0, 0.0); }
            if (pick == 1u) { return vec3<f32>(0.0, 1.0, 0.0); }
            return vec3<f32>(0.0, 0.0, 1.0);
        }

        @vertex
        fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
            var output: VertexOutput;
            let quad_index = vertex_index / 6u;
            let stride = max_render_subdivisions();
            let segment = segments[quad_index / stride];
            let sub = quad_index % stride;
            let corner = vertex_index % 6u;
            let use_end = corner == 2u || corner == 3u || corner == 5u;
            let positive_side = corner == 1u || corner == 4u || corner == 5u;
            let packed = segment.particles.w;
            let part_index = packed & 0xffffu;
            let segment_count = max(packed >> 16u, 1u);
            let part = parts[part_index];
            let noise = strand_random(segment, segment_count);

            let local_segment = i32(round(segment.weights.w * f32(segment_count)));
            let roots = vec3<i32>(segment.particles.xyz) - vec3<i32>(local_segment);
            let subdivisions = clamp(u32(part.spread_b.w), 1u, stride);
            let f = (f32(min(sub, subdivisions)) + select(0.0, 1.0, use_end))
                / f32(subdivisions);

            let t = (f32(local_segment) + f) / f32(segment_count);

            let reach = clamp(dot(segment.weights.xyz, part.lengths.xyz), 0.05, 1.0);
            let travel = t * reach * f32(segment_count);

            let cell = clamp(i32(floor(travel)), 0, i32(segment_count) - 1);
            let frac = clamp(travel - f32(cell), 0.0, 1.0);

            var weights = segment.weights.xyz;
            let deviation = spread_deviation(t, part);
            if (deviation > 0.0 && weights.x < 0.999) {
                let lean = leaned_weights(noise);
                let blend_here = guide_blend(roots, weights, cell, segment_count, part);
                let lean_here = guide_blend(roots, lean, cell, segment_count, part);
                let reach = length(lean_here - blend_here) * deviation;
                var allowed = deviation;
                if (reach > part.spread_b.y && reach > 1.0e-6) {
                    allowed = deviation * (part.spread_b.y / reach);
                }
                weights = mix(weights, lean, allowed);
            }

            let before = guide_blend(roots, weights, cell - 1, segment_count, part);
            let start = guide_blend(roots, weights, cell, segment_count, part);
            let end = guide_blend(roots, weights, cell + 1, segment_count, part);
            let after = guide_blend(roots, weights, cell + 2, segment_count, part);
            let position = catmull_position(before, start, end, after, frac);
            let world = scene.model * vec4<f32>(position, 1.0);

            let direction = catmull_tangent(before, start, end, after, frac);
            let along = safe_normalize(
                (scene.model * vec4<f32>(direction, 0.0)).xyz,
                vec3<f32>(0.0, 1.0, 0.0),
            );
            let towards_eye = normalize(scene.eye.xyz - world.xyz);
            let across = cross(along, towards_eye);
            let span = length(across);
            var side = vec3<f32>(1.0, 0.0, 0.0);
            if (span > 1.0e-4) {
                side = across / span;
            } else {

                let fallback = select(
                    vec3<f32>(0.0, 1.0, 0.0),
                    vec3<f32>(1.0, 0.0, 0.0),
                    abs(along.y) > 0.9,
                );
                side = normalize(cross(along, fallback));
            }

            let taper = mix(0.06, 1.0, pow(1.0 - t, 0.55));
            let authored_half_width = part.width.x * taper * 0.5;
            var half_width = authored_half_width;
            let viewport_pixels = scene.grading.yz;
            let half_pixels =
                ribbon_half_pixels(world.xyz, side * authored_half_width, viewport_pixels);

            if (half_pixels > 0.02 && half_pixels < MIN_STRAND_HALF_PIXELS) {
                let widening = min(MIN_STRAND_HALF_PIXELS / half_pixels, MAX_STRAND_WIDENING);
                half_width = authored_half_width * widening;
            }
            let drawn_half_pixels = clamp(
                half_pixels * (half_width / max(authored_half_width, 1.0e-9)),
                0.05,
                64.0,
            );
            let signed_width = select(-half_width, half_width, positive_side);
            let ribbon_position = world.xyz + side * signed_width;
            output.clip_position = scene.view_projection * vec4<f32>(ribbon_position, 1.0);
            output.world_position = ribbon_position;
            output.world_tangent = along;
            output.strand_t = t;
            output.ribbon_side = select(-1.0, 1.0, positive_side);
            output.part_index = part_index;
            output.strand_noise = noise;
            output.half_pixels = drawn_half_pixels;
            return output;
        }

        fn safe_normalize(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
            let magnitude = length(value);
            return select(fallback, value / magnitude, magnitude > 1.0e-6);
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

        fn environment_radiance(direction: vec3<f32>) -> vec3<f32> {
            let hemisphere = smoothstep(-0.55, 0.75, normalize(direction).y);
            return mix(scene.environment_bottom.rgb, scene.environment_top.rgb, hemisphere);
        }

        fn display_color(linear_color: vec3<f32>) -> vec3<f32> {
            let finite_color = select(
                linear_color,
                vec3<f32>(0.0),
                (linear_color != linear_color) | (abs(linear_color) > vec3<f32>(1.0e20)),
            );
            let exposed = max(finite_color, vec3<f32>(0.0))
                * clamp(scene.lighting.y, 0.35, 2.0) * (1.0 / 0.82);
            let mapped = exposed / (vec3<f32>(1.0) + exposed);
            if (scene.lighting.w > 0.5) { return mapped; }
            return linear_to_srgb(mapped);
        }

        fn fibre_lighting(
            tangent: vec3<f32>,
            normal: vec3<f32>,
            view_direction: vec3<f32>,
            light_direction: vec3<f32>,
            radiance: vec3<f32>,
            albedo: vec3<f32>,
            part: RenderPart,
        ) -> vec3<f32> {
            let halfway = safe_normalize(view_direction + light_direction, normal);
            let diffuse_sine = sqrt(max(1.0 - dot(tangent, light_direction)
                * dot(tangent, light_direction), 0.0));
            let diffuse_exponent = mix(1.65, 0.55, clamp(part.tip_color.w, 0.0, 1.0));
            let diffuse = pow(diffuse_sine, diffuse_exponent) * albedo / 3.14159265;

            let shifted_primary = safe_normalize(
                tangent + normal * part.specular.w,
                tangent,
            );
            let shifted_secondary = safe_normalize(
                tangent + normal * (part.specular.w - 0.04),
                tangent,
            );
            let primary_sine = sqrt(max(1.0 - dot(shifted_primary, halfway)
                * dot(shifted_primary, halfway), 0.0));
            let primary_sharpness = clamp(part.lobes.x, 1.0, 1024.0);
            let primary = pow(primary_sine, primary_sharpness)
                * sqrt((primary_sharpness + 1.0) / 6.2831853);
            var secondary = 0.0;
            if (part.width.z > 0.5 && part.width.z < 3.5) {
                let secondary_sine = sqrt(max(1.0 - dot(shifted_secondary, halfway)
                    * dot(shifted_secondary, halfway), 0.0));
                let secondary_sharpness = clamp(part.lobes.y, 1.0, 1024.0);
                secondary = pow(secondary_sine, secondary_sharpness)
                    * sqrt((secondary_sharpness + 1.0) / 6.2831853) * 0.38;
            }
            let n_dot_v = clamp(abs(dot(normal, view_direction)), 0.0, 1.0);
            let fresnel = 0.035 + clamp(part.lobes.w, 0.0, 4.0)
                * pow(1.0 - n_dot_v, clamp(part.lobes.z, 0.25, 32.0));

            let view_sine = sqrt(max(
                1.0 - dot(tangent, view_direction) * dot(tangent, view_direction),
                0.0,
            ));
            let specular = srgb_to_linear(part.specular.rgb)
                * (primary + secondary) * clamp(fresnel, 0.0, 2.0) * view_sine;
            return (diffuse + specular) * radiance * diffuse_sine;
        }

        @fragment
        fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
            let part = parts[input.part_index];
            let tangent = safe_normalize(input.world_tangent, vec3<f32>(0.0, 1.0, 0.0));
            let view_direction = safe_normalize(
                scene.eye.xyz - input.world_position,
                vec3<f32>(0.0, 0.0, 1.0),
            );
            let side_axis = safe_normalize(
                cross(tangent, view_direction),
                vec3<f32>(1.0, 0.0, 0.0),
            );
            var normal = safe_normalize(cross(side_axis, tangent), view_direction);
            let random_angle = (input.strand_noise * 2.0 - 1.0)
                * clamp(part.variation.w, 0.0, 1.0) * 0.9;
            normal = safe_normalize(
                normal * cos(random_angle) + side_axis * sin(random_angle),
                normal,
            );

            let color_t = pow(
                clamp(input.strand_t, 0.0, 1.0),
                clamp(part.root_color.w, 0.05, 16.0),
            );
            var albedo = srgb_to_linear(mix(part.root_color.rgb, part.tip_color.rgb, color_t));
            let random_value = pow(
                clamp(input.strand_noise, 0.0001, 1.0),
                clamp(part.variation.x, 0.05, 16.0),
            );
            let random_gain = max(
                0.2,
                1.0 + (random_value - 0.5) * 2.0 * clamp(part.variation.y, -1.0, 1.0),
            );
            albedo = albedo * random_gain;

            let direct = fibre_lighting(
                tangent,
                normal,
                view_direction,
                rotated_key_direction(),
                scene.key_light.rgb,
                albedo,
                part,
            ) + fibre_lighting(
                tangent,
                normal,
                view_direction,
                rotated_fill_direction(),
                scene.fill_light.rgb,
                albedo,
                part,
            );
            let reflection_direction = reflect(-view_direction, normal);
            let n_dot_v = clamp(abs(dot(normal, view_direction)), 0.0, 1.0);
            let environment_fresnel = 0.035
                + scene.environment_bottom.w * pow(1.0 - n_dot_v, 5.0);
            let quality_ibl = select(
                0.35,
                1.0,
                part.width.z > 0.5 && part.width.z < 3.5,
            );

            let view_sine = sqrt(max(
                1.0 - dot(tangent, view_direction) * dot(tangent, view_direction),
                0.0,
            ));
            let ibl = environment_radiance(reflection_direction)
                * srgb_to_linear(part.specular.rgb)
                * (0.12 + scene.environment_top.w)
                * environment_fresnel
                * clamp(part.variation.z, 0.0, 4.0) * quality_ibl * view_sine;
            let ambient = environment_radiance(normal) * albedo * 0.34;

            let softness = clamp((input.half_pixels - 1.0) / 3.0, 0.0, 1.0);
            let feather_start = mix(0.995, 0.72, softness);
            let edge_coverage = 1.0 - smoothstep(feather_start, 1.0, abs(input.ribbon_side));
            if (edge_coverage < 0.02) { discard; }
            return vec4<f32>(
                display_color(ambient + direct + ibl),
                part.width.y * edge_coverage,
            );
        }
"#
        )
    };
}

pub(crate) const HAIR_SHADER: &str =
    hair_shader_source!(crate::shader_color::color_grading_wgsl!());
pub(crate) const HAIR_SHADER_HDR: &str =
    hair_shader_source!(crate::shader_color::color_grading_hdr_wgsl!());

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HairUniform {
    view_projection: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    eye: [f32; 4],
    lighting: [f32; 4],
    key_light: [f32; 4],
    fill_light: [f32; 4],
    environment_top: [f32; 4],
    environment_bottom: [f32; 4],

    grading: [f32; 4],
}

fn sanitize_light_yaw(value: f32) -> f32 {
    if value.is_finite() {
        value.rem_euclid(std::f32::consts::TAU)
    } else {
        0.0
    }
}

fn render_storage_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScalpUniform {
    view_projection: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    specular_tint: [f32; 4],
    flags: [f32; 4],
    map_flags: [f32; 4],
    map_flags_2: [f32; 4],
    eye: [f32; 4],
    lighting: [f32; 4],
    key_light: [f32; 4],
    fill_light: [f32; 4],
    environment_top: [f32; 4],
    environment_bottom: [f32; 4],
    grading: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScalpVertex {
    position: [f32; 3],
    uv: [f32; 2],
}

const SCALP_ALPHA_CUTOFF: f32 = 0.45;

#[derive(Clone)]
pub struct ScalpPaintCallback {
    pub scene_key: u64,

    pub head: Arc<SurfaceMesh>,
    pub part: crate::hair_preview::HairScalpPart,
    pub view_projection: Mat4,
    pub model: Mat4,
    pub eye: Vec3,
    pub light_yaw_radians: f32,
    pub light_preset: LightingPreset,
    pub light_brightness: f32,

    pub tone_mapping: crate::shader_color::ToneMapping,
}

impl ScalpPaintCallback {
    pub fn paint_callback(self, rect: Rect) -> epaint::PaintCallback {
        Callback::new_paint_callback(rect, self)
    }
}

impl CallbackTrait for ScalpPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let kept = if let Some(resources) = callback_resources.get_mut::<ScalpRenderResources>() {
            resources.prepare(device, queue, self);

            resources.scenes.contains_key(&self.scene_key)
        } else {
            false
        };

        if kept && let Some(bloom) = callback_resources.get_mut::<crate::bloom::BloomResources>() {
            bloom.record(crate::bloom::HdrDraw::Scalp(self.scene_key));
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(resources) = callback_resources.get::<ScalpRenderResources>() {
            resources.paint(render_pass, self.scene_key, SceneTarget::Screen);
        }
    }
}

const SCALP_SCENE_CACHE_CAP: usize = 8;

struct ScalpGpuScene {
    signature: (usize, u64, u64, u64, u64, u64, u64),
    last_used: u64,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub(crate) struct ScalpRenderResources {
    pipeline: wgpu::RenderPipeline,

    hdr_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    blank: wgpu::TextureView,
    scenes: BTreeMap<u64, ScalpGpuScene>,

    logged_scalps: BTreeSet<ScalpLogIdentity>,
    use_counter: u64,
    target_is_srgb: bool,
}

type ScalpLogIdentity = (u64, usize, u64, u64, u64, u64, u64);

impl ScalpRenderResources {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let module = |label: &'static str, source: &'static str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
            })
        };
        let shader = module("vkit.scalp.shader", SCALP_SHADER);
        let hdr_shader = module("vkit.scalp.shader.hdr", SCALP_SHADER_HDR);
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.scalp.bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.scalp.pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let build = |target_format: wgpu::TextureFormat, target: SceneTarget| {
            let shader = match target {
                SceneTarget::Screen => &shader,
                SceneTarget::Hdr => &hdr_shader,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vkit.scalp.pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<ScalpVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 12,
                                shader_location: 1,
                            },
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,

                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: true,
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vkit.scalp.sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let blank = upload_scalp_texture(device, queue, None, wgpu::TextureFormat::Rgba8Unorm);
        Self {
            pipeline: build(target_format, SceneTarget::Screen),
            hdr_pipeline: build(crate::hdr_target::HDR_FORMAT, SceneTarget::Hdr),
            bind_group_layout,
            sampler,
            blank,
            scenes: BTreeMap::new(),
            logged_scalps: BTreeSet::new(),
            use_counter: 0,
            target_is_srgb: target_format.is_srgb(),
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        callback: &ScalpPaintCallback,
    ) {
        self.use_counter += 1;
        let use_stamp = self.use_counter;
        let part = &callback.part;
        let profile = callback.light_preset.profile();
        let material = part.material;
        let uniform = ScalpUniform {
            view_projection: callback.view_projection.to_cols_array_2d(),
            model: callback.model.to_cols_array_2d(),
            tint: [
                material.diffuse_color[0],
                material.diffuse_color[1],
                material.diffuse_color[2],
                1.0,
            ],
            specular_tint: [
                material.specular_color[0],
                material.specular_color[1],
                material.specular_color[2],
                1.0,
            ],
            flags: [
                SCALP_ALPHA_CUTOFF,
                material.roughness(),
                material.specular_intensity,
                material.specular_fresnel,
            ],
            map_flags: [
                f32::from(u8::from(part.diffuse.is_some())),
                f32::from(u8::from(part.alpha.is_some())),
                f32::from(u8::from(part.normal.is_some())),
                f32::from(u8::from(part.specular.is_some())),
            ],
            map_flags_2: [
                f32::from(u8::from(part.gloss.is_some())),
                material.alpha_adjust,
                0.0,
                0.0,
            ],
            eye: callback.eye.extend(1.0).to_array(),
            lighting: [
                sanitize_light_yaw(callback.light_yaw_radians),
                sanitize_brightness(callback.light_brightness),
                callback.light_preset.id() as f32,
                if self.target_is_srgb { 1.0 } else { 0.0 },
            ],
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
            grading: [callback.tone_mapping.shader_flag(), 0.0, 0.0, 0.0],
        };

        let signature = (
            Arc::as_ptr(&part.anchors) as usize,
            callback.head.revision,
            part.diffuse.as_ref().map_or(0, |image| image.revision),
            part.alpha.as_ref().map_or(0, |image| image.revision),
            part.normal.as_ref().map_or(0, |image| image.revision),
            part.specular.as_ref().map_or(0, |image| image.revision),
            part.gloss.as_ref().map_or(0, |image| image.revision),
        );
        if let Some(scene) = self.scenes.get_mut(&callback.scene_key)
            && scene.signature == signature
        {
            scene.last_used = use_stamp;
            queue.write_buffer(&scene.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
            return;
        }

        let vertices = part
            .anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| ScalpVertex {
                position: anchored_position(&callback.head, anchor).to_array(),

                uv: part
                    .uvs
                    .get(index)
                    .map_or([0.0, 0.0], |uv| [uv[0], 1.0 - uv[1]]),
            })
            .collect::<Vec<_>>();
        let indices = part
            .triangles
            .iter()
            .flat_map(|triangle| triangle.iter().copied())
            .collect::<Vec<u32>>();
        if vertices.is_empty() || indices.is_empty() {
            self.scenes.remove(&callback.scene_key);
            return;
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.scalp.vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.scalp.indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.scalp.uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let diffuse = part.diffuse.as_deref().map(|image| {
            upload_scalp_texture(
                device,
                queue,
                Some(image),
                wgpu::TextureFormat::Rgba8UnormSrgb,
            )
        });
        let alpha = part.alpha.as_deref().map(|image| {
            upload_scalp_texture(device, queue, Some(image), wgpu::TextureFormat::Rgba8Unorm)
        });
        let normal = part.normal.as_deref().map(|image| {
            upload_scalp_texture(device, queue, Some(image), wgpu::TextureFormat::Rgba8Unorm)
        });
        let specular = part.specular.as_deref().map(|image| {
            upload_scalp_texture(device, queue, Some(image), wgpu::TextureFormat::Rgba8Unorm)
        });
        let gloss = part.gloss.as_deref().map(|image| {
            upload_scalp_texture(device, queue, Some(image), wgpu::TextureFormat::Rgba8Unorm)
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.scalp.bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        diffuse.as_ref().unwrap_or(&self.blank),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        alpha.as_ref().unwrap_or(&self.blank),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        normal.as_ref().unwrap_or(&self.blank),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        specular.as_ref().unwrap_or(&self.blank),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        gloss.as_ref().unwrap_or(&self.blank),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let identity = (
            callback.scene_key,
            Arc::as_ptr(&part.anchors) as usize,
            part.diffuse.as_ref().map_or(0, |image| image.revision),
            part.alpha.as_ref().map_or(0, |image| image.revision),
            part.normal.as_ref().map_or(0, |image| image.revision),
            part.specular.as_ref().map_or(0, |image| image.revision),
            part.gloss.as_ref().map_or(0, |image| image.revision),
        );
        if self.logged_scalps.insert(identity) {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Debug,
                "renderer",
                "scalp_scene_uploaded",
                &format!(
                    "key={:#x}; vertices={}; indices={}; diffuse={}; alpha={}; normal={}; specular={}; gloss={}; diffuse_color={:?}",
                    callback.scene_key,
                    vertices.len(),
                    indices.len(),
                    part.diffuse.is_some(),
                    part.alpha.is_some(),
                    part.normal.is_some(),
                    part.specular.is_some(),
                    part.gloss.is_some(),
                    material.diffuse_color,
                ),
            );
        }
        self.scenes.insert(
            callback.scene_key,
            ScalpGpuScene {
                signature,
                last_used: use_stamp,
                vertex_buffer,
                index_buffer,
                index_count: indices.len() as u32,
                uniform_buffer,
                bind_group,
            },
        );
        crate::renderer::evict_lru_scenes(
            &mut self.scenes,
            callback.scene_key,
            SCALP_SCENE_CACHE_CAP,
            |scene| scene.last_used,
        );
    }

    const fn pipeline_for(&self, target: SceneTarget) -> &wgpu::RenderPipeline {
        match target {
            SceneTarget::Screen => &self.pipeline,
            SceneTarget::Hdr => &self.hdr_pipeline,
        }
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

        if scene.index_count == 0 {
            return;
        }
        let _keep_uniform_alive = &scene.uniform_buffer;
        render_pass.set_pipeline(self.pipeline_for(target));
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, scene.vertex_buffer.slice(..));
        render_pass.set_index_buffer(scene.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..scene.index_count, 0, 0..1);
    }
}

fn anchored_position(head: &SurfaceMesh, anchor: &crate::hair_preview::ScalpAnchor) -> Vec3 {
    let corner = |index: u32| {
        head.mesh
            .vertices
            .get(index as usize)
            .map(|point| Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32))
    };
    let [Some(a), Some(b), Some(c)] = anchor.triangle.map(corner) else {
        return Vec3::ZERO;
    };
    let weights = anchor.barycentric;
    let surface = a * weights[0] + b * weights[1] + c * weights[2];
    let normal = (b - a).cross(c - a).try_normalize().unwrap_or(Vec3::Y);
    surface + normal * anchor.normal_offset
}

fn upload_scalp_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: Option<&crate::skin_preview::SkinImage>,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let (width, height, pixels) = image.map_or((1, 1, vec![255_u8; 4]), |image| {
        (image.width, image.height, image.rgba8.as_ref().clone())
    });
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let mip_level_count = 32 - size.width.max(size.height).leading_zeros();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vkit.scalp.texture"),
        size,
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level_pixels = pixels;
    let (mut level_width, mut level_height) = (size.width, size.height);
    for mip_level in 0..mip_level_count {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(level_width * 4),
                rows_per_image: Some(level_height),
            },
            wgpu::Extent3d {
                width: level_width,
                height: level_height,
                depth_or_array_layers: 1,
            },
        );
        if mip_level + 1 == mip_level_count {
            break;
        }
        let view = vkit_core::pixels::RgbaView::new(&level_pixels, level_width, level_height)
            .expect("scalp mip source dimensions match its pixels");
        (level_pixels, level_width, level_height) = vkit_core::pixels::halve_rgba_box(view);
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

#[derive(Clone)]
pub struct HairPaintCallback {
    pub scene_key: u64,
    pub mesh: Arc<SurfaceMesh>,
    pub preview: Arc<HairPreview>,
    pub view_projection: Mat4,
    pub model: Mat4,
    pub eye: Vec3,
    pub light_yaw_radians: f32,
    pub light_preset: LightingPreset,
    pub light_brightness: f32,

    pub tone_mapping: crate::shader_color::ToneMapping,
    pub time_seconds: f64,

    pub settle_gravity: f32,

    pub viewport_pixels: [f32; 2],
}

impl HairPaintCallback {
    pub fn paint_callback(self, rect: Rect) -> epaint::PaintCallback {
        Callback::new_paint_callback(rect, self)
    }
}

impl CallbackTrait for HairPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let kept = if let Some(resources) = callback_resources.get_mut::<HairRenderResources>() {
            resources.prepare(device, queue, egui_encoder, self);

            resources.scenes.contains_key(&self.scene_key)
        } else {
            false
        };

        if kept && let Some(bloom) = callback_resources.get_mut::<crate::bloom::BloomResources>() {
            bloom.record(crate::bloom::HdrDraw::Hair(self.scene_key));
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(resources) = callback_resources.get::<HairRenderResources>() {
            resources.paint(render_pass, self.scene_key, SceneTarget::Screen);
        }
    }
}

struct HairGpuScene {
    physics: HairPhysicsScene,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub(crate) struct HairRenderResources {
    pipeline: wgpu::RenderPipeline,

    hdr_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    physics_pipelines: HairPhysicsPipelines,
    scenes: BTreeMap<u64, HairGpuScene>,
    target_is_srgb: bool,
}

impl HairRenderResources {
    pub(crate) fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let module = |label: &'static str, source: &'static str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
            })
        };
        let shader = module("vkit.hair.shader", HAIR_SHADER);
        let hdr_shader = module("vkit.hair.shader.hdr", HAIR_SHADER_HDR);
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.hair.scene-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<HairUniform>() as u64
                        ),
                    },
                    count: None,
                },
                render_storage_entry(1, wgpu::ShaderStages::VERTEX),
                render_storage_entry(2, wgpu::ShaderStages::VERTEX),
                render_storage_entry(3, wgpu::ShaderStages::VERTEX_FRAGMENT),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.hair.pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let build = |target_format: wgpu::TextureFormat, target: SceneTarget| {
            let shader = match target {
                SceneTarget::Screen => &shader,
                SceneTarget::Hdr => &hdr_shader,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vkit.hair.pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,

                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: true,
                },
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            pipeline: build(target_format, SceneTarget::Screen),
            hdr_pipeline: build(crate::hdr_target::HDR_FORMAT, SceneTarget::Hdr),
            bind_group_layout,
            physics_pipelines: HairPhysicsPipelines::new(device),
            scenes: BTreeMap::new(),
            target_is_srgb: target_format.is_srgb(),
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        callback: &HairPaintCallback,
    ) {
        let profile = callback.light_preset.profile();
        let mut uniform = HairUniform {
            view_projection: callback.view_projection.to_cols_array_2d(),
            model: callback.model.to_cols_array_2d(),
            eye: callback.eye.extend(1.0).to_array(),
            lighting: [
                sanitize_light_yaw(callback.light_yaw_radians),
                sanitize_brightness(callback.light_brightness),
                callback.light_preset.id() as f32,
                if self.target_is_srgb { 1.0 } else { 0.0 },
            ],
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
            grading: [
                callback.tone_mapping.shader_flag(),
                callback.viewport_pixels[0],
                callback.viewport_pixels[1],
                0.0,
            ],
        };
        if let Some(scene) = self.scenes.get_mut(&callback.scene_key)
            && scene.physics.matches(&callback.preview, &callback.mesh)
        {
            uniform.grading[3] = scene.physics.render_subdivisions() as f32;
            queue.write_buffer(&scene.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
            scene
                .physics
                .update_head_if_needed(queue, Arc::clone(&callback.mesh));
            scene.physics.step(
                queue,
                encoder,
                &self.physics_pipelines,
                callback.time_seconds,
                callback.settle_gravity,
            );
            return;
        }
        let Some(physics) = HairPhysicsScene::new(
            device,
            Arc::clone(&callback.preview),
            Arc::clone(&callback.mesh),
            &self.physics_pipelines,
        ) else {
            self.scenes.remove(&callback.scene_key);
            return;
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.hair.uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.hair.bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: physics.particle_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: physics.render_segment_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: physics.render_part_buffer().as_entire_binding(),
                },
            ],
        });
        self.scenes.insert(
            callback.scene_key,
            HairGpuScene {
                physics,
                uniform_buffer,
                bind_group,
            },
        );
    }

    const fn pipeline_for(&self, target: SceneTarget) -> &wgpu::RenderPipeline {
        match target {
            SceneTarget::Screen => &self.pipeline,
            SceneTarget::Hdr => &self.hdr_pipeline,
        }
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
        let _keep_uniform_alive = &scene.uniform_buffer;
        render_pass.set_pipeline(self.pipeline_for(target));
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.draw(0..scene.physics.render_vertex_count(), 0..1);
    }
}

#[cfg(test)]
mod tests {
    use vkit_core::{
        formats::Mesh,
        vam::{HairOpticalSettings, HairPhysicsSettings},
    };

    use super::*;
    use crate::hair_preview::{
        HairPreviewGuide, HairPreviewPart, HairPreviewStrand, HairRootBinding, HairStrandSource,
    };

    fn wgsl_struct_size(name: &str, source: &str, declared: &str) -> usize {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{name} WGSL failed to parse: {error}"));
        let mut layouter = naga::proc::Layouter::default();
        layouter
            .update(module.to_ctx())
            .expect("shader types have a layout");
        let (handle, _) = module
            .types
            .iter()
            .find(|(_, declared_type)| declared_type.name.as_deref() == Some(declared))
            .unwrap_or_else(|| panic!("{name} declares no {declared}"));
        layouter[handle].size as usize
    }

    #[test]
    fn the_hair_and_scalp_uniforms_are_the_same_size_on_both_sides() {
        for (name, source, declared, expected) in [
            (
                "hair",
                HAIR_SHADER,
                "HairUniform",
                std::mem::size_of::<HairUniform>(),
            ),
            (
                "hair-hdr",
                HAIR_SHADER_HDR,
                "HairUniform",
                std::mem::size_of::<HairUniform>(),
            ),
            (
                "scalp",
                SCALP_SHADER,
                "ScalpUniform",
                std::mem::size_of::<ScalpUniform>(),
            ),
            (
                "scalp-hdr",
                SCALP_SHADER_HDR,
                "ScalpUniform",
                std::mem::size_of::<ScalpUniform>(),
            ),
        ] {
            assert_eq!(
                wgsl_struct_size(name, source, declared),
                expected,
                "{name}: {declared} differs between Rust and WGSL"
            );
        }
    }

    #[test]
    fn hair_and_scalp_uniforms_keep_wgsl_vec4_alignment() {
        assert_eq!(std::mem::size_of::<HairUniform>(), 240);
        assert_eq!(std::mem::size_of::<ScalpUniform>(), 320);
        assert_eq!(std::mem::size_of::<HairUniform>() % 16, 0);
        assert_eq!(std::mem::size_of::<ScalpUniform>() % 16, 0);
        assert!(SCALP_SHADER.contains("uv_area_scale"));
        assert!(!SCALP_SHADER.contains("abs(determinant) < 1.0e-7"));
    }

    #[test]
    fn hair_render_and_compute_pipelines_validate_on_an_available_adapter() {
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
        let Ok((device, queue)) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("vkit.hair-pipeline-test"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            }))
        else {
            return;
        };
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut resources =
            HairRenderResources::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb, 1);
        let _scalp_resources =
            ScalpRenderResources::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb, 1);
        let mesh = Arc::new(
            SurfaceMesh::new(
                Mesh::new(
                    vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]],
                    vec![[0, 1, 2]],
                )
                .expect("test mesh"),
            )
            .expect("test surface"),
        );
        let preview = Arc::new(HairPreview {
            preset_id: "pipeline-test".to_owned(),
            parts: vec![HairPreviewPart {
                curve_density: 4,
                guides: Arc::new(vec![HairPreviewGuide {
                    binding: HairRootBinding {
                        triangle: [0, 1, 2],
                        barycentric: [0.5, 0.25, 0.25],
                        normal_offset: 0.0,
                        base_tangent: [1.0, 0.0, 0.0],
                        base_bitangent: [0.0, 1.0, 0.0],
                        base_normal: [0.0, 0.0, 1.0],
                    },
                    local_points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.2]],
                    painted_rigidity: vec![1.0, 1.0],
                }]),
                strands: Arc::new(vec![HairPreviewStrand {
                    point_count: 2,
                    source: HairStrandSource::Guide(0),
                }]),
                root_color: [0.1, 0.05, 0.02],
                tip_color: [0.2, 0.1, 0.04],
                width: 0.01,
                metres_to_template: 100.0,
                optics: HairOpticalSettings::default(),
                physics: HairPhysicsSettings::default(),
                waviness: Default::default(),
                spread: Default::default(),
                strand_length_m: 0.3,
                nearby_joints: Vec::new(),
            }],
            scalps: Vec::new(),
            skipped_parts: Vec::new(),
            body_capsules: Vec::new(),
        });
        let mut callback = HairPaintCallback {
            scene_key: 1,
            mesh,
            preview,
            view_projection: Mat4::IDENTITY,
            model: Mat4::IDENTITY,
            eye: Vec3::new(0.0, 0.0, 2.0),
            light_yaw_radians: 0.0,
            light_preset: LightingPreset::Studio,
            light_brightness: 1.0,
            tone_mapping: crate::shader_color::ToneMapping::default(),
            time_seconds: 0.0,
            settle_gravity: 0.0,
            viewport_pixels: [1280.0, 800.0],
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vkit.hair-pipeline-test.encoder"),
        });
        resources.prepare(&device, &queue, &mut encoder, &callback);
        callback.time_seconds = 1.0 / 30.0;
        resources.prepare(&device, &queue, &mut encoder, &callback);
        queue.submit(Some(encoder.finish()));
        let error = pollster::block_on(error_scope.pop());
        assert!(
            error.is_none(),
            "hair GPU pipeline validation failed: {error:?}"
        );
    }
}
