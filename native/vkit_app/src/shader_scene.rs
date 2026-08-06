macro_rules! scene_uniform_wgsl {
    () => {
        r#"
struct PunctualLight {

    position_range: vec4<f32>,

    direction_inner: vec4<f32>,

    radiance_outer: vec4<f32>,
};

struct SceneUniform {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    color: vec4<f32>,
    eye: vec4<f32>,
    lighting: vec4<f32>,
    key_light: vec4<f32>,
    fill_light: vec4<f32>,
    environment_top: vec4<f32>,
    environment_bottom: vec4<f32>,

    grading: vec4<f32>,

    punctual_meta: vec4<f32>,
    punctual: array<PunctualLight, 4>,
};

@group(0) @binding(0) var<uniform> scene: SceneUniform;

struct PunctualSample {
    direction: vec3<f32>,
    radiance: vec3<f32>,
};

fn yaw_rig(vector: vec3<f32>) -> vec3<f32> {
    let cosine = cos(scene.lighting.x);
    let sine = sin(scene.lighting.x);
    return vec3<f32>(
        cosine * vector.x + sine * vector.z,
        vector.y,
        -sine * vector.x + cosine * vector.z,
    );
}

fn punctual_count() -> u32 {
    return u32(clamp(scene.punctual_meta.x, 0.0, 4.0));
}

fn punctual_sample(index: u32, world_position: vec3<f32>) -> PunctualSample {
    let light = scene.punctual[index];
    let to_light = yaw_rig(light.position_range.xyz) - world_position;

    let distance_squared = max(dot(to_light, to_light), 1.0e-8);
    let distance = sqrt(distance_squared);

    let range = max(light.position_range.w, 1.0e-3);
    let ratio = clamp(distance / range, 0.0, 1.0);
    let window = 1.0 - ratio * ratio * ratio * ratio;
    let falloff = window * window / distance_squared;

    let aim = yaw_rig(light.direction_inner.xyz);
    let alignment = dot(-to_light / distance, aim);
    let edge = max(light.direction_inner.w - light.radiance_outer.w, 1.0e-4);
    let cone = clamp((alignment - light.radiance_outer.w) / edge, 0.0, 1.0);

    var sampled: PunctualSample;
    sampled.direction = to_light / distance;
    sampled.radiance = light.radiance_outer.rgb * falloff * cone * cone;
    return sampled;
}
"#
    };
}

pub(crate) use scene_uniform_wgsl;
