pub(super) const SHADER: &str = concat!(
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

pub(super) const SKIN_SHADER: &str =
    skin_shader_source!(crate::shader_color::color_grading_wgsl!());
pub(super) const SKIN_SHADER_HDR: &str =
    skin_shader_source!(crate::shader_color::color_grading_hdr_wgsl!());

pub(super) const DEPTH_RESET_SHADER: &str = r#"
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

pub(super) const MIP_BLIT_SHADER: &str = r#"
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
