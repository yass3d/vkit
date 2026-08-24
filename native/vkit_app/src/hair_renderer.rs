use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use bytemuck::{Pod, Zeroable};
use egui::epaint;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use crate::{
    hair_physics::{HairPhysicsPipelines, HairPhysicsScene},
    hair_preview::HairPreview,
    lighting::{LightingPreset, sanitize_brightness},
    renderer::DEPTH_FORMAT,
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
            @location(2) normal: vec3<f32>,
        };
        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) uv: vec2<f32>,
            @location(1) world_position: vec3<f32>,
            @location(2) normal: vec3<f32>,
        };

        @vertex
        fn vs_main(input: VertexInput) -> VertexOutput {
            var output: VertexOutput;
            let world = scene.model * vec4<f32>(input.position, 1.0);
            output.clip_position = scene.view_projection * world;
            output.uv = input.uv;
            output.world_position = world.xyz;
            output.normal = (scene.model * vec4<f32>(input.normal, 0.0)).xyz;
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
            var sheet = vec3<f32>(1.0);
            if (scene.map_flags.x > 0.5) {
                sheet = textureSample(diffuse_map, map_sampler, input.uv).rgb;
            }
            let albedo = srgb_to_linear(scene.tint.rgb)
                * clamp(
                    sheet + vec3<f32>(scene.map_flags_2.z),
                    vec3<f32>(0.0),
                    vec3<f32>(1.0),
                );
            var coverage = scene.tint.a;
            if (scene.map_flags.y > 0.5) {
                let sampled = textureSample(alpha_map, map_sampler, input.uv);
                let masked = select(
                    map_luminance(sampled.rgb),
                    sampled.a,
                    scene.map_flags_2.w > 0.5,
                );
                coverage = coverage * masked;
            }
            coverage = clamp(coverage + scene.map_flags_2.y, 0.0, 1.0);
            if (coverage < scene.flags.x) {
                discard;
            }
            var normal = normalize(cross(dpdx(input.world_position), dpdy(input.world_position)));
            if (dot(input.normal, input.normal) > 1.0e-8) {
                normal = normalize(input.normal);
            }
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

        struct Particle {
            position: vec4<f32>,
            previous: vec4<f32>,

            inner: vec4<f32>,
            velocity: vec4<f32>,
        };
        struct RenderSegment {

            particles: vec4<u32>,

            weights: vec4<f32>,
            slot: vec4<f32>,
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
            waviness_d: vec4<f32>,

            spread_a: vec4<f32>,

            spread_b: vec4<f32>,

            lengths: vec4<f32>,
        };
        @group(0) @binding(1) var<storage, read> particles: array<Particle>;
        @group(0) @binding(2) var<storage, read> segments: array<RenderSegment>;
        @group(0) @binding(3) var<storage, read> parts: array<RenderPart>;
        struct GuideData {
            normal_phase: vec4<f32>,
            rand: vec4<f32>,
        };
        @group(0) @binding(4) var<storage, read> guide_data: array<GuideData>;

        @group(0) @binding(5) var<storage, read> runs: array<vec4<u32>>;

        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) world_position: vec3<f32>,
            @location(1) world_tangent: vec3<f32>,
            @location(2) strand_t: f32,
            @location(3) ribbon_side: f32,
            @location(4) @interpolate(flat) part_index: u32,
            @location(5) @interpolate(flat) strand_noise: f32,

            @location(7) @interpolate(flat) light_centre: vec3<f32>,

            @location(8) radiance: vec3<f32>,
        };

        fn max_render_subdivisions() -> u32 {
            return max(u32(scene.grading.w), 1u);
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

        fn envelope_ease(base: f32, power: f32) -> f32 {
            if (base <= 0.0) {
                return select(0.0, 1.0, power <= 0.0);
            }
            return pow(base, power);
        }

        fn waviness_envelope(t: f32, part: RenderPart) -> f32 {
            let midpoint = max(part.waviness_c.w, 0.001);
            if (t <= midpoint) {
                let eased = envelope_ease(1.0 - t / midpoint, part.waviness_b.w);
                return mix(part.waviness_c.y, part.waviness_c.x, eased);
            }
            let eased = envelope_ease(
                (t - midpoint) / max(1.0 - midpoint, 0.001),
                part.waviness_b.w,
            );
            return mix(part.waviness_c.y, part.waviness_c.z, eased);
        }

        fn strand_spine(root: u32, segment_count: u32) -> vec3<f32> {
            return particles[root + segment_count].position.xyz
                - particles[root].position.xyz;
        }

        struct GuideCurl {
            spine: vec3<f32>,
            root_normal: vec3<f32>,
            rand: vec3<f32>,
            phase: f32,

            rest_chord: f32,
        };

        fn guide_curl(root: u32, segment_count: u32) -> GuideCurl {
            var curl: GuideCurl;
            curl.spine = strand_spine(root, segment_count);
            curl.root_normal = guide_data[root].normal_phase.xyz;
            curl.rand = guide_data[root].rand.xyz;
            curl.phase = guide_data[root].normal_phase.w;
            curl.rest_chord = guide_data[root].rand.w;
            return curl;
        }

        fn baked_envelope(travel: f32, segment_count: u32, part: RenderPart) -> f32 {
            let cells = f32(segment_count);
            let along = clamp(travel, 0.0, cells);
            let cell = floor(min(along, cells - 1.0e-4));
            let k = along - cell;
            return mix(
                waviness_envelope(cell / cells, part),
                waviness_envelope((cell + 1.0) / cells, part),
                k,
            );
        }

        fn waviness_offset(
            t: f32,
            travel: f32,
            segment_count: u32,
            part: RenderPart,
            curl: GuideCurl,
        ) -> vec3<f32> {
            let vector = part.waviness_a.xyz;
            let scale = part.waviness_a.w;
            if (scale <= 0.0) {
                return vec3<f32>(0.0);
            }
            let chord = length(curl.spine);
            if (chord <= 1.0e-6) {
                return vec3<f32>(0.0);
            }
            let spine = curl.spine / chord;

            let amp_random = 1.0 + part.waviness_b.y * (curl.rand.z - 0.5);
            let freq_random = 1.0 + part.waviness_b.z * (curl.rand.x - 0.5);
            let sign = select(1.0, -1.0, curl.rand.y < 0.5);

            let flags = u32(part.width.w);
            var flipped = vector;
            if ((flags & 2u) != 0u) {
                flipped = flipped * sign;
            }
            var winding = 1.0;
            if ((flags & 1u) != 0u) {
                winding = sign;
            }

            let angle = winding
                * (curl.phase + t * curl.rest_chord * part.waviness_b.x * freq_random);

            let cosine = cos(angle);
            let sine = sin(angle);
            let rotated = flipped * cosine
                + cross(spine, flipped) * sine
                + spine * dot(spine, flipped) * (1.0 - cosine);

            let adjusted = rotated + curl.root_normal * part.waviness_d.x;

            return adjusted
                * (scale * amp_random * baked_envelope(travel, segment_count, part));
        }

        fn guide_spline(root: u32, travel: f32, segment_count: u32) -> vec3<f32> {
            let along = max(travel, 0.0);
            let cell = i32(floor(along));
            let k = along - f32(cell);
            let last = i32(segment_count);
            let p0 = particles[root + u32(clamp(cell - 1, 0, last))].position.xyz;
            let p1 = particles[root + u32(clamp(cell, 0, last))].position.xyz;
            let p2 = particles[root + u32(clamp(cell + 1, 0, last))].position.xyz;
            let inv = 1.0 - k;
            return (p0 + p1) * (0.5 * inv * inv)
                + p1 * (2.0 * k * inv)
                + (p1 + p2) * (0.5 * k * k);
        }

        fn strand_sample(
            roots: vec3<u32>,
            bary: vec3<f32>,
            travel: f32,
            segment_count: u32,
            part: RenderPart,
            curl_x: GuideCurl,
            curl_y: GuideCurl,
            curl_z: GuideCurl,
        ) -> vec3<f32> {
            let t = clamp(travel / f32(segment_count), 0.0, 1.0);
            let g0 = guide_spline(roots.x, travel, segment_count)
                + waviness_offset(t, travel, segment_count, part, curl_x);
            var g1 = guide_spline(roots.y, travel, segment_count)
                + waviness_offset(t, travel, segment_count, part, curl_y);
            var g2 = guide_spline(roots.z, travel, segment_count)
                + waviness_offset(t, travel, segment_count, part, curl_z);
            let max_spread = part.spread_b.y;
            let d1 = g1 - g0;
            let len1 = length(d1);
            g1 = g0 + d1 * (min(len1, max_spread) / max(len1, 1.0e-4));
            let d2 = g2 - g0;
            let len2 = length(d2);
            g2 = g0 + d2 * (min(len2, max_spread) / max(len2, 1.0e-4));
            let blended = g0 * bary.x + g1 * bary.y + g2 * bary.z;
            let centre = (g0 + g1 + g2) / 3.0;
            return mix(blended, centre, spread_gather(t, part));
        }

        fn spread_gather(t: f32, part: RenderPart) -> f32 {
            let midpoint = max(part.spread_a.w, 0.001);
            var followed: f32;
            if (t <= midpoint) {
                let eased = envelope_ease(1.0 - t / midpoint, part.spread_b.x);
                followed = mix(part.spread_a.y, part.spread_a.x, eased);
            } else {
                let eased = envelope_ease(
                    (t - midpoint) / max(1.0 - midpoint, 0.001),
                    part.spread_b.x,
                );
                followed = mix(part.spread_a.y, part.spread_a.z, eased);
            }
            return clamp(1.0 - followed, 0.0, 1.0);
        }

        fn tess_travel(corner: f32, segment_count: u32, density: f32) -> f32 {
            return corner * (f32(segment_count) + 1.0) / max(density, 1.0);
        }

        @vertex
        fn vs_main(
            @builtin(vertex_index) vertex_index: u32,
            @builtin(instance_index) run_index: u32,
        ) -> VertexOutput {
            var output: VertexOutput;
            let quad_index = vertex_index / 4u;
            let run = runs[run_index];
            let stride = max(run.x, 1u);
            let segment = segments[run.y + quad_index / stride];
            let sub = quad_index % stride;
            let corner = vertex_index % 4u;
            let use_end = corner >= 2u;
            let positive_side = (corner & 1u) == 1u;
            let packed = segment.particles.w;
            let part_index = packed & 0xffffu;
            let segment_count = max(packed >> 16u, 1u);
            let part = parts[part_index];
            let noise = strand_random(segment, segment_count);

            let local_segment = i32(round(segment.weights.w * f32(segment_count)));
            let roots = vec3<i32>(segment.particles.xyz) - vec3<i32>(local_segment);
            let subdivisions = stride;
            let f = (f32(sub) + select(0.0, 1.0, use_end)) / f32(subdivisions);

            let t = (f32(local_segment) + f) / f32(segment_count);

            let weights = segment.weights.xyz;
            let uroots = vec3<u32>(roots);
            let curl_x = guide_curl(uroots.x, segment_count);
            let curl_y = guide_curl(uroots.y, segment_count);
            let curl_z = guide_curl(uroots.z, segment_count);

            let density = max(part.spread_b.w, 2.0);
            let length_factor = clamp(dot(segment.slot.xyz, part.lengths.xyz), 0.0, 1.0);
            let tess_point = min(floor(t * (density - 0.001) * length_factor), density - 1.0);

            let position = strand_sample(
                uroots, weights, tess_travel(tess_point, segment_count, density),
                segment_count, part, curl_x, curl_y, curl_z,
            );
            let world = scene.model * vec4<f32>(position, 1.0);

            var neighbour_point = tess_point - 1.0;
            var neighbour_sign = 1.0;
            if (neighbour_point < 0.0) {
                neighbour_point = min(tess_point + 1.0, density - 1.0);
                neighbour_sign = -1.0;
            }
            let neighbour = strand_sample(
                uroots, weights, tess_travel(neighbour_point, segment_count, density),
                segment_count, part, curl_x, curl_y, curl_z,
            );
            let direction = (position - neighbour) * neighbour_sign;
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

            let taper = 1.0 - saturate((t - 0.9) * 10.0);
            let half_width = part.width.x * taper;
            let signed_width = select(-half_width, half_width, positive_side);
            let ribbon_position = world.xyz + side * signed_width;
            output.clip_position = scene.view_projection * vec4<f32>(ribbon_position, 1.0);
            output.world_position = ribbon_position;
            output.world_tangent = along;
            output.strand_t = t;
            output.ribbon_side = select(-1.0, 1.0, positive_side);
            output.part_index = part_index;
            output.strand_noise = noise;

            let root = strand_sample(
                uroots, weights, 0.0, segment_count, part, curl_x, curl_y, curl_z,
            );
            let root_world = (scene.model * vec4<f32>(root, 1.0)).xyz;
            let root_normal = safe_normalize(
                (scene.model * vec4<f32>(
                    curl_x.root_normal * weights.x
                        + curl_y.root_normal * weights.y
                        + curl_z.root_normal * weights.z,
                    0.0,
                )).xyz,
                vec3<f32>(0.0, 1.0, 0.0),
            );
            let lanes = arrayLength(&guide_data);
            let last_lane = f32(max(lanes, 1u) - 1u);
            let pick = u32(clamp(last_lane * curl_x.rand.x, 0.0, last_lane));
            let other_normal = safe_normalize(
                (scene.model * vec4<f32>(guide_data[pick].normal_phase.xyz, 0.0)).xyz,
                root_normal,
            );
            let light_normal = mix(
                root_normal,
                other_normal,
                clamp(part.variation.w, 0.0, 1.0),
            );
            output.light_centre = root_world - light_normal * part.lengths.w;
            output.radiance = strand_radiance(
                output.world_position,
                output.world_tangent,
                output.strand_t,
                noise,
                output.light_centre,
                part_index,
            );
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

        fn sine_between(a: vec3<f32>, b: vec3<f32>) -> f32 {
            let cosine = dot(a, b);
            return sqrt(max(1.0 - cosine * cosine, 0.0));
        }

        fn fibre_lighting(
            tangent: vec3<f32>,
            pseudo_normal: vec3<f32>,
            ribbon_normal: vec3<f32>,
            view_direction: vec3<f32>,
            light_direction: vec3<f32>,
            radiance: vec3<f32>,
            albedo: vec3<f32>,
            strand_t: f32,
            shift: f32,
            part: RenderPart,
        ) -> vec3<f32> {
            let lift = clamp(part.tip_color.w, 0.0, 1.0);
            let hair_normal = safe_normalize(
                light_direction - tangent * dot(tangent, light_direction),
                ribbon_normal,
            );

            let primary_axis = safe_normalize(
                tangent + hair_normal * (shift - part.specular.w),
                tangent,
            );
            let secondary_axis = safe_normalize(
                tangent + hair_normal * (shift + part.specular.w),
                tangent,
            );
            let halfway = safe_normalize(view_direction + light_direction, ribbon_normal);
            let primary = pow(
                sine_between(primary_axis, halfway),
                clamp(part.lobes.x, 1.0, 1024.0),
            );
            var secondary = 0.0;
            if (part.width.z > 0.5 && part.width.z < 3.5) {
                secondary = pow(
                    sine_between(secondary_axis, halfway),
                    clamp(part.lobes.y, 1.0, 1024.0),
                );
            }

            let sphere_dot_light = clamp(dot(pseudo_normal, light_direction), 0.0, 1.0);
            let sphere_dot_view = clamp(dot(pseudo_normal, view_direction), 0.0, 1.0);
            let specular = srgb_to_linear(part.specular.rgb)
                * (primary + secondary)
                * sphere_dot_light
                * sphere_dot_view
                * clamp(2.0 * strand_t, 0.0, 1.0);

            let diffuse = albedo
                * clamp(
                    clamp(dot(hair_normal, light_direction), 0.0, 1.0) * (1.0 - lift) + lift,
                    0.0,
                    1.0,
                );

            let rim = pow(1.0 - sphere_dot_view, clamp(part.lobes.z, 0.25, 32.0))
                * clamp(part.lobes.w, 0.0, 1.0);
            let gate = clamp(sphere_dot_light * (1.0 - lift) + lift + rim, 0.0, 1.0);
            return (diffuse + specular) * radiance * gate;
        }

        fn strand_radiance(
            world_position: vec3<f32>,
            world_tangent: vec3<f32>,
            strand_t: f32,
            strand_noise: f32,
            light_centre: vec3<f32>,
            part_index: u32,
        ) -> vec3<f32> {
            let part = parts[part_index];
            let tangent = safe_normalize(world_tangent, vec3<f32>(0.0, 1.0, 0.0));
            let view_direction = safe_normalize(
                scene.eye.xyz - world_position,
                vec3<f32>(0.0, 0.0, 1.0),
            );
            let side_axis = safe_normalize(
                cross(tangent, view_direction),
                vec3<f32>(1.0, 0.0, 0.0),
            );
            let normal = safe_normalize(cross(side_axis, tangent), view_direction);

            let color_t = pow(
                clamp(strand_t, 0.0, 1.0),
                clamp(part.root_color.w, 0.05, 16.0),
            );
            let ramp = mix(part.root_color.rgb, part.tip_color.rgb, color_t);
            var albedo = srgb_to_linear(ramp);
            let random_gain = max(
                0.0,
                1.0
                    + clamp(part.variation.x, 0.0, 4.0)
                        * (strand_noise + clamp(part.variation.y, -1.0, 1.0) - 1.0),
            );
            albedo = albedo * random_gain;

            let pseudo_normal = safe_normalize(
                world_position - light_centre,
                normal,
            );
            let shift = clamp(ramp.r - 0.5, 0.0, 1.0);
            let direct = fibre_lighting(
                tangent,
                pseudo_normal,
                normal,
                view_direction,
                rotated_key_direction(),
                scene.key_light.rgb,
                albedo,
                strand_t,
                shift,
                part,
            ) + fibre_lighting(
                tangent,
                pseudo_normal,
                normal,
                view_direction,
                rotated_fill_direction(),
                scene.fill_light.rgb,
                albedo,
                strand_t,
                shift,
                part,
            );

            let ambient = albedo
                * (environment_radiance(pseudo_normal) * clamp(part.variation.z, 0.0, 1.0)
                    + scene.environment_bottom.rgb);
            return ambient + direct;
        }

        @fragment
        fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
            return vec4<f32>(display_color(input.radiance), 1.0);
        }
"#
        )
    };
}

pub(crate) const HAIR_SHADER: &str =
    hair_shader_source!(crate::shader_color::color_grading_wgsl!());
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
    normal: [f32; 3],
}

const SCALP_ALPHA_FLOOR: f32 = 1.0 / 255.0;

#[derive(Clone)]
pub struct ScalpPaintCallback {
    pub spot: crate::renderer::SceneSpot,
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
    pub fn paint_callback(self) -> epaint::PaintCallback {
        Callback::new_paint_callback(self.spot.rect, self)
    }
}

impl CallbackTrait for ScalpPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        crate::renderer::sync_scene_samples(device, queue, callback_resources);
        if let Some(resources) = callback_resources.get_mut::<ScalpRenderResources>() {
            resources.prepare(device, queue, self);
        }
        let Some(mut pass) = crate::renderer::begin_scene_layer(
            device,
            egui_encoder,
            callback_resources,
            screen_descriptor,
            self.spot,
        ) else {
            return Vec::new();
        };
        if let Some(resources) = callback_resources.get::<ScalpRenderResources>() {
            resources.paint(&mut pass, self.scene_key);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        crate::renderer::blit_scene(render_pass, callback_resources);
    }
}

const SCALP_SCENE_CACHE_CAP: usize = 8;

const HAIR_SCENE_CACHE_CAP: usize = 24;

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

    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    blank: wgpu::TextureView,
    scenes: BTreeMap<u64, ScalpGpuScene>,
    texture_views: std::collections::HashMap<(u64, wgpu::TextureFormat), wgpu::TextureView>,

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
        let build = |target_format: wgpu::TextureFormat| {
            let shader = &shader;
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
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 20,
                                shader_location: 2,
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

                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
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
            pipeline: build(target_format),
            bind_group_layout,
            sampler,
            blank,
            scenes: BTreeMap::new(),
            texture_views: std::collections::HashMap::new(),
            logged_scalps: BTreeSet::new(),
            use_counter: 0,
            target_is_srgb: target_format.is_srgb(),
        }
    }

    fn cached_scalp_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &crate::skin_preview::SkinImage,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureView {
        const CACHE_CAP: usize = 48;
        let key = (image.revision, format);
        if let Some(view) = self.texture_views.get(&key) {
            return view.clone();
        }
        if self.texture_views.len() >= CACHE_CAP {
            self.texture_views.clear();
        }
        let view = upload_scalp_texture(device, queue, Some(image), format);
        self.texture_views.insert(key, view.clone());
        view
    }

    pub(crate) fn prepare(
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
                SCALP_ALPHA_FLOOR,
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
                material.diffuse_offset,
                f32::from(u8::from(
                    part.alpha.as_deref().is_some_and(mask_lives_in_alpha),
                )),
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

        let positions = part
            .anchors
            .iter()
            .map(|anchor| anchored_position(&callback.head, anchor))
            .collect::<Vec<_>>();
        let mut normals = vec![Vec3::ZERO; positions.len()];
        for triangle in part.triangles.iter() {
            let [a, b, c] = triangle.map(|index| positions.get(index as usize).copied());
            let (Some(a), Some(b), Some(c)) = (a, b, c) else {
                continue;
            };
            let face = (b - a).cross(c - a);
            for index in triangle {
                if let Some(normal) = normals.get_mut(*index as usize) {
                    *normal += face;
                }
            }
        }
        let vertices = positions
            .iter()
            .enumerate()
            .map(|(index, position)| ScalpVertex {
                position: position.to_array(),

                uv: part
                    .uvs
                    .get(index)
                    .map_or([0.0, 0.0], |uv| [uv[0], 1.0 - uv[1]]),
                normal: normals
                    .get(index)
                    .and_then(|normal| normal.try_normalize())
                    .unwrap_or(Vec3::Y)
                    .to_array(),
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
        let diffuse = part.diffuse.clone().map(|image| {
            self.cached_scalp_texture(device, queue, &image, wgpu::TextureFormat::Rgba8UnormSrgb)
        });
        let alpha = part.alpha.clone().map(|image| {
            self.cached_scalp_texture(device, queue, &image, wgpu::TextureFormat::Rgba8Unorm)
        });
        let normal = part.normal.clone().map(|image| {
            self.cached_scalp_texture(device, queue, &image, wgpu::TextureFormat::Rgba8Unorm)
        });
        let specular = part.specular.clone().map(|image| {
            self.cached_scalp_texture(device, queue, &image, wgpu::TextureFormat::Rgba8Unorm)
        });
        let gloss = part.gloss.clone().map(|image| {
            self.cached_scalp_texture(device, queue, &image, wgpu::TextureFormat::Rgba8Unorm)
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

    pub(crate) fn paint(&self, render_pass: &mut wgpu::RenderPass<'static>, scene_key: u64) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };

        if scene.index_count == 0 {
            return;
        }
        let _keep_uniform_alive = &scene.uniform_buffer;
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, scene.vertex_buffer.slice(..));
        render_pass.set_index_buffer(scene.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..scene.index_count, 0, 0..1);
    }
}

pub(crate) fn anchored_position(
    head: &SurfaceMesh,
    anchor: &crate::hair_preview::ScalpAnchor,
) -> Vec3 {
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

#[must_use]
fn mask_lives_in_alpha(image: &crate::skin_preview::SkinImage) -> bool {
    let pixels = image.rgba8.as_ref();
    let mut lowest = 255_u8;
    let mut highest = 0_u8;
    for alpha in pixels.iter().skip(3).step_by(4 * 97) {
        lowest = lowest.min(*alpha);
        highest = highest.max(*alpha);
    }
    highest.saturating_sub(lowest) > 8
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
    pub spot: crate::renderer::SceneSpot,
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

    pub simulate_hair: crate::hair_physics::HairSimulation,

    pub solve: bool,

    pub viewport_pixels: [f32; 2],

    pub frame: u64,
}

impl HairPaintCallback {
    pub fn paint_callback(self) -> epaint::PaintCallback {
        Callback::new_paint_callback(self.spot.rect, self)
    }
}

impl CallbackTrait for HairPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        crate::renderer::sync_scene_samples(device, queue, callback_resources);
        if let Some(resources) = callback_resources.get_mut::<HairRenderResources>() {
            resources.prepare(device, queue, egui_encoder, self);
        }
        let Some(mut pass) = crate::renderer::begin_scene_layer(
            device,
            egui_encoder,
            callback_resources,
            screen_descriptor,
            self.spot,
        ) else {
            return Vec::new();
        };
        if let Some(resources) = callback_resources.get::<HairRenderResources>() {
            resources.paint(&mut pass, self.scene_key);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        crate::renderer::blit_scene(render_pass, callback_resources);
    }
}

struct HairGpuScene {
    physics: HairPhysicsScene,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    last_used: u64,
}

pub(crate) struct HairRenderResources {
    pipeline: wgpu::RenderPipeline,

    bind_group_layout: wgpu::BindGroupLayout,
    physics_pipelines: HairPhysicsPipelines,
    scenes: BTreeMap<u64, HairGpuScene>,

    empty: BTreeMap<u64, crate::hair_physics::SceneInputs>,
    quad_indices: Option<(wgpu::Buffer, u32)>,
    target_is_srgb: bool,
}

pub(crate) const HAIR_QUAD_CORNERS: u32 = 4;

fn build_quad_indices(device: &wgpu::Device, quads: u32) -> (wgpu::Buffer, u32) {
    let mut indices = Vec::with_capacity(quads as usize * 6);
    for quad in 0..quads {
        let base = quad * HAIR_QUAD_CORNERS;
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
    }
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vkit.hair.quad-indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (buffer, quads)
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
                render_storage_entry(4, wgpu::ShaderStages::VERTEX),
                render_storage_entry(5, wgpu::ShaderStages::VERTEX),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.hair.pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let build = |target_format: wgpu::TextureFormat| {
            let shader = &shader;
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
            pipeline: build(target_format),
            bind_group_layout,
            physics_pipelines: HairPhysicsPipelines::new(device),
            scenes: BTreeMap::new(),
            empty: BTreeMap::new(),
            quad_indices: None,
            target_is_srgb: target_format.is_srgb(),
        }
    }

    pub(crate) fn prepare(
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
        if self.empty.get(&callback.scene_key).is_some_and(|inputs| {
            inputs.matches(&callback.preview, &callback.mesh, callback.simulate_hair)
        }) {
            return;
        }
        if let Some(scene) = self.scenes.get_mut(&callback.scene_key)
            && scene
                .physics
                .matches(&callback.preview, &callback.mesh, callback.simulate_hair)
        {
            uniform.grading[3] = scene.physics.render_subdivisions() as f32;
            scene.last_used = callback.frame;
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
                callback.solve,
            );
            return;
        }
        let Some(physics) = HairPhysicsScene::new(
            device,
            Arc::clone(&callback.preview),
            Arc::clone(&callback.mesh),
            &self.physics_pipelines,
            callback.simulate_hair,
        ) else {
            self.scenes.remove(&callback.scene_key);
            self.empty.insert(
                callback.scene_key,
                crate::hair_physics::SceneInputs::of(
                    Arc::clone(&callback.preview),
                    Arc::clone(&callback.mesh),
                    callback.simulate_hair,
                ),
            );
            return;
        };
        self.empty.remove(&callback.scene_key);
        uniform.grading[3] = physics.render_subdivisions() as f32;
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: physics.guide_normal_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: physics.render_run_buffer().as_entire_binding(),
                },
            ],
        });
        self.scenes.insert(
            callback.scene_key,
            HairGpuScene {
                physics,
                uniform_buffer,
                bind_group,
                last_used: callback.frame,
            },
        );
        self.evict_stale_scenes(callback.frame);
        self.ensure_quad_indices(device);
    }

    fn evict_stale_scenes(&mut self, frame: u64) {
        while self.scenes.len() > HAIR_SCENE_CACHE_CAP {
            let stale = self
                .scenes
                .iter()
                .filter(|(_, scene)| scene.last_used != frame)
                .min_by_key(|(_, scene)| scene.last_used)
                .map(|(&key, _)| key);
            let Some(stale) = stale else {
                return;
            };
            self.scenes.remove(&stale);
        }
    }

    fn ensure_quad_indices(&mut self, device: &wgpu::Device) {
        let wanted = self
            .scenes
            .values()
            .map(|scene| scene.physics.render_index_count() / 6)
            .max()
            .unwrap_or(0);
        if wanted == 0 {
            return;
        }
        if self
            .quad_indices
            .as_ref()
            .is_some_and(|(_, quads)| *quads >= wanted)
        {
            return;
        }
        self.quad_indices = Some(build_quad_indices(device, wanted));
    }

    pub(crate) fn paint(&self, render_pass: &mut wgpu::RenderPass<'static>, scene_key: u64) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };
        let Some((indices, _)) = self.quad_indices.as_ref() else {
            return;
        };
        let _keep_uniform_alive = &scene.uniform_buffer;
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        for (indices, run) in scene.physics.render_runs() {
            if indices == 0 {
                continue;
            }
            render_pass.draw_indexed(0..indices, 0, run..run + 1);
        }
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
                "scalp",
                SCALP_SHADER,
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
                    curl_rand: [0.5, 0.5, 0.5],
                    curl_phase: 0.0,
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
            body_capsules: Vec::new(),
        });
        let mut callback = HairPaintCallback {
            spot: crate::renderer::SceneSpot::default(),
            scene_key: 1,
            frame: 0,
            mesh,
            preview,
            simulate_hair: crate::hair_physics::HairSimulation::Off,
            solve: true,
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

#[cfg(test)]
mod scalp_mask_tests {
    use super::*;
    use crate::skin_preview::{SkinImage, SkinUvOrientation};

    fn sheet(pixels: Vec<u8>) -> SkinImage {
        let count = pixels.len() / 4;
        SkinImage {
            revision: 1,
            width: count as u32,
            height: 1,
            rgba8: std::sync::Arc::new(pixels),
            uv_orientation: SkinUvOrientation::ObjFlipV,
        }
    }

    #[test]
    fn a_jpeg_mask_is_read_from_its_ink() {
        let mut pixels = Vec::new();
        for step in 0..600_u32 {
            let ink = (step % 256) as u8;
            pixels.extend_from_slice(&[ink, ink, ink, 255]);
        }
        assert!(
            !mask_lives_in_alpha(&sheet(pixels)),
            "a flat alpha channel says nothing, so the ink has to be believed",
        );
    }

    #[test]
    fn a_sheet_that_speaks_through_alpha_is_read_there() {
        let mut pixels = Vec::new();
        for step in 0..600_u32 {
            pixels.extend_from_slice(&[20, 20, 20, (step % 256) as u8]);
        }
        assert!(
            mask_lives_in_alpha(&sheet(pixels)),
            "near-black ink with a varying alpha is the other convention, and \
             reading the ink would erase the scalp",
        );
    }

    #[test]
    fn a_solid_sheet_is_not_mistaken_for_an_alpha_one() {
        let pixels = vec![255_u8; 4 * 600];
        assert!(!mask_lives_in_alpha(&sheet(pixels)));
    }

    #[test]
    fn the_floor_is_a_floor_and_not_a_cutout_threshold() {
        const {
            assert!(
                SCALP_ALPHA_FLOOR < 0.01,
                "anything higher throws away the soft edge of every strand",
            );
        }
    }
}

#[cfg(test)]
mod scene_cache_tests {
    fn evict(scenes: &mut Vec<(u64, u64)>, cap: usize, frame: u64) {
        while scenes.len() > cap {
            let stale = scenes
                .iter()
                .enumerate()
                .filter(|(_, (_, used))| *used != frame)
                .min_by_key(|(_, (_, used))| *used)
                .map(|(index, _)| index);
            let Some(stale) = stale else {
                return;
            };
            scenes.remove(stale);
        }
    }

    #[test]
    fn a_frame_that_needs_more_scenes_than_the_cap_keeps_all_of_them() {
        let mut scenes: Vec<(u64, u64)> = (0..14).map(|key| (key, 9)).collect();
        evict(&mut scenes, 4, 9);
        assert_eq!(
            scenes.len(),
            14,
            "eviction took a scene the frame was about to draw",
        );
    }

    #[test]
    fn scenes_from_older_frames_go_oldest_first() {
        let mut scenes: Vec<(u64, u64)> = vec![(0, 1), (1, 5), (2, 3), (3, 9), (4, 9)];
        evict(&mut scenes, 3, 9);
        assert_eq!(scenes.len(), 3);
        let kept: Vec<u64> = scenes.iter().map(|(key, _)| *key).collect();
        assert!(kept.contains(&3) && kept.contains(&4), "this frame stays");
        assert!(kept.contains(&1), "and the newest of the old ones");
        assert!(!kept.contains(&0), "the oldest goes first");
    }
}

#[cfg(test)]
mod quad_contract_tests {
    use super::*;

    #[test]
    fn the_shader_reads_the_corners_the_indices_write() {
        let divide = format!("let quad_index = vertex_index / {HAIR_QUAD_CORNERS}u;");
        let remainder = format!("let corner = vertex_index % {HAIR_QUAD_CORNERS}u;");
        assert!(
            HAIR_SHADER.contains(&divide),
            "the shader does not find its quad the way the indices are written: {divide}",
        );
        assert!(
            HAIR_SHADER.contains(&remainder),
            "the shader does not find its corner the way the indices are written: {remainder}",
        );
    }

    #[test]
    fn the_index_pattern_covers_the_quad_exactly_once() {
        let pattern = [0_u32, 1, 2, 2, 1, 3];
        let corner = |index: u32| {
            let use_end = index >= 2;
            let positive = index % 2 == 1;
            (use_end, positive)
        };
        let mut seen = std::collections::BTreeSet::new();
        for index in pattern {
            seen.insert(corner(index));
        }
        assert_eq!(
            seen.len(),
            4,
            "the two triangles have to reach all four corners: {seen:?}",
        );
        assert_eq!(
            pattern.len(),
            6,
            "two triangles, six indices, four corners run through the vertex stage",
        );
    }
}

#[cfg(test)]
mod child_length_tests {
    use super::*;

    fn corner(density: f32, domain: f32, length_factor: f32) -> f32 {
        (domain * (density - 0.001) * length_factor)
            .floor()
            .min(density - 1.0)
    }

    fn travel(corner: f32, segment_count: u32, density: f32) -> f32 {
        corner * (segment_count as f32 + 1.0) / density
    }

    #[test]
    fn a_full_length_child_reaches_the_last_point_and_no_further() {
        for density in [2.0_f32, 8.0, 16.0, 32.0, 64.0] {
            assert_eq!(
                corner(density, 1.0, 1.0),
                density - 1.0,
                "density {density} at full length must end on the last point",
            );
            assert_eq!(corner(density, 0.0, 1.0), 0.0);
        }
    }

    #[test]
    fn a_short_child_lands_on_a_density_point_rather_than_between_two() {
        assert_eq!(corner(16.0, 1.0, 0.5), 7.0);
    }

    #[test]
    fn neighbouring_tiers_collapse_onto_the_same_point() {
        assert_eq!(corner(16.0, 1.0, 0.44), corner(16.0, 1.0, 0.50));
        assert_ne!(corner(16.0, 1.0, 0.50), corner(16.0, 1.0, 0.60));
    }

    #[test]
    fn the_last_tessellation_point_sits_where_the_kernel_puts_it() {
        let last = corner(32.0, 1.0, 1.0);
        assert_eq!(last, 31.0);
        let parameter = travel(last, 11, 32.0);
        assert!(
            (parameter - 11.625).abs() < 1.0e-4,
            "the last point landed at {parameter}, not the kernel's 11.625 — a \
             whole particle index short ends every strand half a cell early",
        );
    }

    #[test]
    fn the_vertex_grid_is_never_coarser_than_the_polyline() {
        for density in 2_u32..=64 {
            for segment_count in 1_u32..=50 {
                let subdivisions =
                    crate::hair_physics::segment_subdivisions_for_test(density, segment_count);
                assert!(
                    segment_count * subdivisions >= density,
                    "density {density} over {segment_count} segments draws only \
                     {} vertices, so whole corners are dropped",
                    segment_count * subdivisions,
                );
            }
        }
    }

    #[test]
    fn the_shader_truncates_the_length_the_way_the_game_does() {
        assert!(
            HAIR_SHADER.contains(
                "let tess_point = min(floor(t * (density - 0.001) * length_factor), density - 1.0);"
            ),
            "the shader no longer truncates every drawn vertex to a density point",
        );
        assert!(
            HAIR_SHADER.contains("return corner * (f32(segment_count) + 1.0) / max(density, 1.0);"),
            "the spline parameter has to be (corner / density) * particles",
        );
        assert!(
            !HAIR_SHADER.contains("polyline_sample"),
            "a drawn vertex is a tessellation point, not a resample between two",
        );
    }
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    #[test]
    fn the_shader_reproduces_the_pow_the_engine_uses_rather_than_flooring_it() {
        assert!(
            HAIR_SHADER.contains("fn envelope_ease(base: f32, power: f32) -> f32 {")
                && HAIR_SHADER.contains("return select(0.0, 1.0, power <= 0.0);"),
            "pow(0, 0) has to come out as 1, not as a NaN and not as a floor",
        );
        assert!(
            !HAIR_SHADER.contains("let eased = pow("),
            "every envelope ease has to go through envelope_ease",
        );
    }

    #[test]
    fn the_curve_powers_reach_the_gpu_as_they_were_loaded() {
        let source = include_str!("hair_physics.rs");
        assert!(
            !source.contains("curve_power.max("),
            "a floor on curve_power puts the step envelope out of reach",
        );
    }

    #[test]
    fn the_curl_amplitude_is_a_chord_fit_over_the_particles() {
        assert!(
            HAIR_SHADER.contains("fn baked_envelope(travel: f32, segment_count: u32"),
            "the amplitude has to be baked at particle times and interpolated",
        );
        assert!(
            HAIR_SHADER
                .contains("* (scale * amp_random * baked_envelope(travel, segment_count, part));"),
            "the curl offset has to use the baked amplitude, not the curve",
        );
    }
}

#[cfg(test)]
mod curl_frequency_tests {
    use super::*;

    #[test]
    fn the_turn_count_is_measured_on_the_rest_chord_not_the_simulated_one() {
        assert!(
            HAIR_SHADER.contains(
                "* (curl.phase + t * curl.rest_chord * part.waviness_b.x * freq_random);"
            ),
            "the curl angle is back on the live chord, so the frequency breathes",
        );
        assert!(
            HAIR_SHADER.contains("curl.rest_chord = guide_data[root].rand.w;"),
            "the rest chord has to come from the guide data the mesh rebuilds",
        );
        assert!(
            HAIR_SHADER.contains("let spine = curl.spine / chord;"),
            "the rotation axis must still come from the live spine",
        );
    }
}

#[cfg(test)]
mod normal_randomize_tests {
    use super::*;

    #[test]
    fn normal_randomize_moves_the_light_centre_and_nothing_else() {
        assert!(
            HAIR_SHADER
                .contains("output.light_centre = root_world - light_normal * part.lengths.w;")
                && HAIR_SHADER
                    .contains("let pick = u32(clamp(last_lane * curl_x.rand.x, 0.0, last_lane));"),
            "the randomized normal has to be the one the light centre is built from",
        );
        assert!(
            !HAIR_SHADER.contains("random_angle"),
            "the shading normal twist, and its magic 0.9, have no counterpart in the game",
        );
        assert!(
            !HAIR_SHADER.contains("0.9;"),
            "no magic 0.9 survives in the strand shader",
        );
    }

    #[test]
    fn the_borrowed_normal_is_lerped_rather_than_renormalised() {
        let at = HAIR_SHADER
            .find("let light_normal = mix(")
            .expect("the light normal is a lerp");
        let tail = &HAIR_SHADER[at..at + 200];
        assert!(
            !tail.contains("safe_normalize(mix(") && !tail.contains("normalize(light_normal)"),
            "the lerp must not be renormalised: {tail}",
        );
    }
}

#[cfg(test)]
mod strand_width_tests {
    use super::*;

    #[test]
    fn the_ribbon_is_the_authored_width_and_the_fragment_is_opaque() {
        assert!(
            HAIR_SHADER.contains("let half_width = part.width.x * taper;"),
            "the drawn width has to be the authored one, tapered and nothing else",
        );
        for gone in [
            "MIN_STRAND_HALF_PIXELS",
            "MAX_STRAND_WIDENING",
            "ribbon_half_pixels",
            "edge_coverage",
            "feather_start",
        ] {
            assert!(
                !HAIR_SHADER.contains(gone),
                "{gone} has no counterpart in the game and still stands",
            );
        }
        assert!(
            HAIR_SHADER.contains("return vec4<f32>(display_color(input.radiance), 1.0);"),
            "the fragment writes o0.a = 1, as the game's pixel shader does",
        );
    }
}
