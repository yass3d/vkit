use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use glam::Vec3;
use wgpu::util::DeviceExt as _;

use rayon::prelude::*;
use vkit_core::formats::Mesh;
use vkit_core::spatial::{SurfaceProjector, projector_for_mesh};

use crate::{
    hair_preview::{HairPreview, HairPreviewGuide, HairRootBinding, HairStrandSource},
    scene::SurfaceMesh,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HairSimulation {
    #[default]
    Off,
    Every,
}

const LIGHT_CENTRE_DEPTH_M: f32 = 0.1;

const WORKGROUP_SIZE: u32 = 64;
const FIXED_STEP_SECONDS: f32 = 1.0 / 60.0;
/// How many fixed steps one rendered frame may consume.
///
/// The sim lives in `FixedUpdate`, which Unity runs as many times per frame as
/// the accumulated time needs; the game defines no per-frame cap at all. A cap
/// of one, paired with an accumulator clamped back to a single step, threw away
/// real time every frame: at 30 fps the hair integrated thirty steps a second
/// instead of sixty and fell at half speed, so every physics slider read weaker
/// in the viewport than it would behave in VaM — and that mis-tune shipped.
/// Six is the elapsed clamp divided by the step, so it can never bind.
const MAX_FRAME_STEPS: usize = 6;

/// The most quads a single guide segment can be asked to carry.
///
/// The game has no per-segment ceiling: the hull shader sets
/// `detail = curveDensity`, whose own clamp is 64, and a strand with one
/// segment must still be drawn at every one of those points. Anything
/// lower drops whole corners of the polyline rather than merely rounding
/// them — at the worst shipped ratio a curl collapsed to a near straight
/// line.
pub(crate) const MAX_RENDER_SUBDIVISIONS: u32 = 64;

const MAX_VAM_ITERATIONS: u32 = 5;
const MAX_PART_INDEX: usize = u16::MAX as usize;
const MAX_SEGMENTS_PER_STRAND: usize = u16::MAX as usize;

pub(crate) const HAIR_PHYSICS_SHADER: &str = r#"
struct PhysicsUniform {
    fixed_dt: f32,
    particle_count: u32,
    collider_count: u32,

    settle_gravity: f32,

    frame: u32,
    capsule_count: u32,
    strand_count: u32,
    _pad: u32,
};

struct PhaseUniform {
    offset: u32,
    count: u32,
    kind: u32,
    iteration: u32,
};

struct Particle {
    position: vec4<f32>,

    previous: vec4<f32>,

    inner: vec4<f32>,

    velocity: vec4<f32>,
};

struct RestParticle {

    position: vec4<f32>,

    data: vec4<f32>,

    indices: vec4<u32>,
};

struct PartSettings {

    forces: vec4<f32>,

    wind: vec4<f32>,

    rigidity: vec4<f32>,

    constraints: vec4<f32>,

    collision: vec4<f32>,

    misc: vec4<f32>,
};

struct Constraint {

    indices: vec4<u32>,

    values: vec4<f32>,
};

struct StrandRange {

    span: vec4<u32>,
};

struct BodyCapsule {

    a: vec4<f32>,

    b: vec4<f32>,
};

@group(0) @binding(0) var<uniform> physics: PhysicsUniform;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(2) var<storage, read> rests: array<RestParticle>;
@group(0) @binding(3) var<storage, read> parts: array<PartSettings>;
@group(0) @binding(4) var<storage, read> constraints: array<Constraint>;

@group(0) @binding(5) var<storage, read> head_field: array<vec4<f32>>;

@group(0) @binding(6) var<storage, read> strands: array<StrandRange>;

@group(0) @binding(7) var<storage, read> capsules: array<BodyCapsule>;
@group(1) @binding(0) var<uniform> phase: PhaseUniform;

const WARMUP_RESET_FRAMES: u32 = 10u;
const WARMUP_STILL_FRAMES: u32 = 20u;

const DISTANCE_JOINT_POWER: f32 = 0.5;

fn head_sdf_cell(x: i32, y: i32, z: i32, resolution: i32) -> f32 {
    let cx = clamp(x, 0, resolution - 1);
    let cy = clamp(y, 0, resolution - 1);
    let cz = clamp(z, 0, resolution - 1);
    let index = u32((cz * resolution + cy) * resolution + cx);
    let packed = head_field[2u + index / 4u];
    let lane = index % 4u;
    if (lane == 0u) { return packed.x; }
    if (lane == 1u) { return packed.y; }
    if (lane == 2u) { return packed.z; }
    return packed.w;
}

fn head_distance(position: vec3<f32>) -> f32 {
    let header = head_field[0];
    let resolution = i32(header.w);
    let cell = head_field[1].x;
    if (resolution <= 0 || cell <= 0.0 || physics.collider_count == 0u) {
        return 1.0e9;
    }
    let grid = (position - header.xyz) / cell;
    let base = floor(grid);
    let t = grid - base;
    let x0 = i32(base.x);
    let y0 = i32(base.y);
    let z0 = i32(base.z);
    var total = 0.0;
    for (var k = 0; k < 2; k = k + 1) {
        let wz = select(1.0 - t.z, t.z, k == 1);
        for (var j = 0; j < 2; j = j + 1) {
            let wy = select(1.0 - t.y, t.y, j == 1);
            for (var i = 0; i < 2; i = i + 1) {
                let wx = select(1.0 - t.x, t.x, i == 1);
                total = total
                    + head_sdf_cell(x0 + i, y0 + j, z0 + k, resolution) * wx * wy * wz;
            }
        }
    }
    // Outside the box the cell fetch clamps, which repeats the boundary slab
    // for ever — a head-shaped column extruded down through the shoulders,
    // which is not a collider anybody built.
    //
    // Added, not maxed. The cell fetch has already clamped the sample onto the
    // box wall, so `total` is the distance from the nearest point on that wall
    // to the head; how far past the wall we stand adds to it. That keeps the
    // gradient pointing away from the *head*. A `max` hands back the box's own
    // gradient instead — a repulsive shell around the grid, which is a second
    // phantom collider in place of the first. Inside, the term is zero and the
    // field is untouched, negatives and all.
    let span = cell * max(f32(resolution) - 1.0, 0.0);
    let outside = max(header.xyz - position, position - (header.xyz + vec3<f32>(span)));
    return total + length(max(outside, vec3<f32>(0.0)));
}

fn head_gradient(position: vec3<f32>) -> vec3<f32> {
    let cell = head_field[1].x;
    let step = max(cell, 1.0e-6) * 0.5;
    let dx = head_distance(position + vec3<f32>(step, 0.0, 0.0))
        - head_distance(position - vec3<f32>(step, 0.0, 0.0));
    let dy = head_distance(position + vec3<f32>(0.0, step, 0.0))
        - head_distance(position - vec3<f32>(0.0, step, 0.0));
    let dz = head_distance(position + vec3<f32>(0.0, 0.0, step))
        - head_distance(position - vec3<f32>(0.0, 0.0, step));
    let gradient = vec3<f32>(dx, dy, dz);
    let magnitude = length(gradient);
    if (magnitude <= 1.0e-9) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return gradient / magnitude;
}

fn part_iterations(part: PartSettings) -> f32 {
    return max(part.collision.w, 1.0);
}

fn active_for_iteration(part: PartSettings) -> bool {
    return f32(phase.iteration) < part_iterations(part);
}

fn part_inner_dt(part: PartSettings) -> f32 {
    return physics.fixed_dt * max(part.forces.z, 0.01) / part_iterations(part);
}

fn simulating(part: PartSettings) -> bool {
    return part.forces.w >= 0.5 || physics.settle_gravity > 0.0;
}

@compute @workgroup_size(64)
fn integrate(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let local_index = invocation.x;
    if (local_index >= phase.count) {
        return;
    }
    let index = phase.offset + local_index;
    if (index >= physics.particle_count) {
        return;
    }
    let rest = rests[index];
    let part = parts[rest.indices.x];
    if (!active_for_iteration(part) || rest.indices.z != 0u || !simulating(part)) {
        return;
    }

    if (physics.frame < WARMUP_RESET_FRAMES) {
        particles[index].position = vec4<f32>(rest.position.xyz, 1.0);
        particles[index].previous = vec4<f32>(rest.position.xyz, 1.0);
        particles[index].inner = vec4<f32>(rest.position.xyz, 0.0);
        particles[index].velocity = vec4<f32>(rest.position.xyz, 0.0);
        return;
    }

    if (physics.frame <= WARMUP_STILL_FRAMES) {
        return;
    }

    let iterations = part_iterations(part);
    let dt = part_inner_dt(part);

    let inv_drag = clamp(1.0 - part.forces.y / iterations, 0.0, 1.0);
    let current = particles[index].position.xyz;
    let previous = particles[index].previous.xyz;
    let delta = (current - previous) * inv_drag;

    var gravity = part.forces.x;
    if (physics.settle_gravity > 0.0) {
        gravity = max(gravity, physics.settle_gravity);
    }
    let collision_drag = clamp(particles[index].velocity.w, 0.0, 1.0);
    let acceleration =
        vec3<f32>(0.0, -9.81 * gravity * part.misc.y, 0.0) +
        part.wind.xyz * part.misc.y;
    particles[index].previous = vec4<f32>(current, 1.0);
    particles[index].position = vec4<f32>(
        current + delta + acceleration * (0.5 * dt * dt) * (1.0 - collision_drag),
        1.0,
    );
}

@compute @workgroup_size(64)
fn point_joints(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let local_index = invocation.x;
    if (local_index >= phase.count) {
        return;
    }
    let index = phase.offset + local_index;
    if (index >= physics.particle_count) {
        return;
    }
    let rest = rests[index];
    let part = parts[rest.indices.x];
    if (!active_for_iteration(part)) {
        return;
    }

    if (rest.indices.z != 0u || !simulating(part)) {
        particles[index].position = vec4<f32>(rest.position.xyz, 1.0);
        return;
    }
    if (physics.frame < WARMUP_RESET_FRAMES) {
        return;
    }

    if (rest.data.y >= 1.0) {
        particles[index].position = vec4<f32>(rest.position.xyz, 1.0);
        return;
    }
    let rigidity = clamp(rest.data.y * max(part.forces.z, 0.0), 0.0, 1.0);
    if (rigidity <= 0.0) {
        return;
    }
    particles[index].position = vec4<f32>(
        mix(particles[index].position.xyz, rest.position.xyz, rigidity),
        1.0,
    );
}

@compute @workgroup_size(64)
fn solve_constraint(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let local_index = invocation.x;
    if (local_index >= phase.count) {
        return;
    }
    let joint = constraints[phase.offset + local_index];
    let part = parts[joint.indices.z];
    if (!active_for_iteration(part)) {
        return;
    }
    if (physics.frame < WARMUP_RESET_FRAMES) {
        return;
    }
    let a_index = joint.indices.x;
    let b_index = joint.indices.y;
    var a = particles[a_index].position.xyz;
    var b = particles[b_index].position.xyz;
    let delta = b - a;
    let distance = length(delta);
    if (distance < 1.0e-7) {
        return;
    }

    let iterations = part_iterations(part);
    var strength = DISTANCE_JOINT_POWER;
    if (joint.indices.w == 1u) {

        if (distance >= joint.values.x) {
            return;
        }
        strength = clamp(part.constraints.y, 0.0, 1.0) / iterations;
    } else if (joint.indices.w == 2u) {
        let elasticity = clamp(joint.values.y, 0.0, 1.0);
        let rolloff = clamp(part.constraints.w, 0.0, 1.0);
        strength = clamp(part.constraints.z, 0.0, 1.0) * 0.5 / iterations
            * (1.0 + rolloff * (elasticity - 1.0));
    }
    let error = (distance - joint.values.x) / distance;
    let correction = delta * error * strength;
    if (rests[a_index].indices.z == 0u) {
        a = a + correction;
        particles[a_index].position = vec4<f32>(a, 1.0);
    }
    if (rests[b_index].indices.z == 0u) {
        b = b - correction;
        particles[b_index].position = vec4<f32>(b, 1.0);
    }
}

@compute @workgroup_size(64)
fn spline_joints(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let strand_index = invocation.x;
    if (strand_index >= physics.strand_count) {
        return;
    }
    let strand = strands[strand_index].span;
    let part = parts[strand.z];
    if (!active_for_iteration(part) || !simulating(part)) {
        return;
    }
    if (physics.frame < WARMUP_RESET_FRAMES) {
        return;
    }

    let power = min(
        clamp(part.constraints.x, 0.0, 1.0) * 2.0 / part_iterations(part),
        1.0,
    );
    if (power <= 0.0) {
        return;
    }
    let start = strand.x;
    let count = strand.y;

    for (var point = 1u; point < count; point = point + 1u) {
        let index = start + point;
        let rest_length = length(
            rests[index].position.xyz - rests[index - 1u].position.xyz,
        );
        let parent = particles[index - 1u].position.xyz;
        let current = particles[index].position.xyz;
        let delta = parent - current;
        let distance = length(delta);
        if (distance <= 1.0e-6) {
            continue;
        }
        particles[index].position = vec4<f32>(
            current + delta * (power * (distance - rest_length) / distance),
            1.0,
        );
    }
}

fn resolve_capsule(position: vec3<f32>, radius: f32, capsule: BodyCapsule) -> vec3<f32> {
    let a = capsule.a.xyz;
    let axis = capsule.b.xyz - a;
    let length_squared = max(dot(axis, axis), 1.0e-8);
    let t = clamp(dot(position - a, axis) / length_squared, 0.0, 1.0);
    let nearest = a + axis * t;
    let offset = position - nearest;
    let distance = length(offset);
    let clearance = capsule.a.w + radius;
    if (distance >= clearance || distance < 1.0e-6) {
        return position;
    }
    return nearest + offset * (clearance / distance);
}

@compute @workgroup_size(64)
fn collide(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let local_index = invocation.x;
    if (local_index >= phase.count) {
        return;
    }
    let index = phase.offset + local_index;
    if (index >= physics.particle_count) {
        return;
    }
    let rest = rests[index];
    let part = parts[rest.indices.x];

    if (rest.indices.z != 0u || !simulating(part)) {
        return;
    }
    if (physics.frame < WARMUP_RESET_FRAMES) {
        return;
    }

    let collides = part.collision.x >= 0.5 || physics.settle_gravity > 0.0;
    let strand_radius = rest.data.z;
    var position = particles[index].position.xyz;
    let original = position;
    if (collides) {
        let distance = head_distance(position);
        if (distance < strand_radius) {
            position = position + head_gradient(position) * (strand_radius - distance);
        }
        for (var capsule = 0u; capsule < physics.capsule_count; capsule = capsule + 1u) {
            position = resolve_capsule(position, strand_radius, capsules[capsule]);
        }
    }
    let pen = length(position - original);
    var hold = 0.0;
    var drag = 0.0;
    if (pen > 0.0) {
        let friction = clamp(part.wind.w, 0.0, 1.0);
        let pen_f = clamp(pen / max(part.misc.y, 1.0e-6) * 50.0, 0.0, 0.5);
        hold = clamp(pen_f + friction + 0.02, 0.0, 1.0);
        drag = friction;
        particles[index].position = vec4<f32>(position, 1.0);
    }
    particles[index].inner.w = hold;
    particles[index].velocity.w = drag;
}

@compute @workgroup_size(64)
fn velocity_inner(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let local_index = invocation.x;
    if (local_index >= phase.count) {
        return;
    }
    let index = phase.offset + local_index;
    if (index >= physics.particle_count) {
        return;
    }
    let rest = rests[index];
    let part = parts[rest.indices.x];
    if (!active_for_iteration(part) || rest.indices.z != 0u || !simulating(part)) {
        return;
    }
    if (physics.frame < WARMUP_RESET_FRAMES) {
        return;
    }
    let hold = clamp(particles[index].inner.w, 0.0, 1.0);
    let position = particles[index].position.xyz;
    let damped = (position - particles[index].inner.xyz) * (1.0 - hold);
    particles[index].previous = vec4<f32>(position - damped, 1.0);
    particles[index].inner = vec4<f32>(position, hold);
}

@compute @workgroup_size(64)
fn velocity_outer(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let local_index = invocation.x;
    if (local_index >= phase.count) {
        return;
    }
    let index = phase.offset + local_index;
    if (index >= physics.particle_count) {
        return;
    }
    let rest = rests[index];
    let part = parts[rest.indices.x];
    if (rest.indices.z != 0u || !simulating(part)) {
        return;
    }
    if (physics.frame < WARMUP_RESET_FRAMES) {
        return;
    }
    let dt = part_inner_dt(part);
    let outer_dt = dt * part_iterations(part);
    let hold = clamp(particles[index].inner.w, 0.0, 1.0);
    let position = particles[index].position.xyz;
    let velocity = (position - particles[index].velocity.xyz) * ((1.0 - hold) / outer_dt);
    particles[index].inner = vec4<f32>(position, hold);
    particles[index].velocity = vec4<f32>(position, particles[index].velocity.w);
    particles[index].previous = vec4<f32>(position - velocity * dt, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPhysicsUniform {
    fixed_dt: f32,
    particle_count: u32,
    collider_count: u32,
    settle_gravity: f32,
    frame: u32,
    capsule_count: u32,
    strand_count: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPhaseUniform {
    offset: u32,
    count: u32,
    kind: u32,
    iteration: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParticle {
    position: [f32; 4],
    previous: [f32; 4],

    inner: [f32; 4],

    velocity: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuStrandRange {
    meta: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuBodyCapsule {
    a: [f32; 4],
    b: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRestParticle {
    position: [f32; 4],
    data: [f32; 4],
    meta: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPartSettings {
    forces: [f32; 4],
    wind: [f32; 4],
    rigidity: [f32; 4],
    constraints: [f32; 4],
    collision: [f32; 4],
    misc: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuConstraint {
    meta: [u32; 4],
    values: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuHeadFieldElement {
    lanes: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuHairRenderSegment {
    pub particles: [u32; 4],

    pub weights: [f32; 4],
    pub slot: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuHairRenderPart {
    pub root_color: [f32; 4],

    pub tip_color: [f32; 4],

    pub specular: [f32; 4],

    pub lobes: [f32; 4],

    pub variation: [f32; 4],

    pub width: [f32; 4],

    pub waviness_a: [f32; 4],

    pub waviness_b: [f32; 4],

    pub waviness_c: [f32; 4],

    pub waviness_d: [f32; 4],

    pub spread_a: [f32; 4],

    pub spread_b: [f32; 4],

    pub lengths: [f32; 4],
}

#[derive(Clone, Copy)]
struct ConstraintRange {
    offset: u32,
    count: u32,
    kind: u32,
}

#[derive(Clone, Copy)]
enum DispatchStage {
    Integrate,
    PointJoints,
    Constraint,
    SplineJoints,
    Collide,
    VelocityInner,
    VelocityOuter,
}

struct PhaseDispatch {
    stage: DispatchStage,
    _uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    workgroups: u32,
}

pub(crate) struct HairPhysicsPipelines {
    main_layout: wgpu::BindGroupLayout,
    phase_layout: wgpu::BindGroupLayout,
    integrate: wgpu::ComputePipeline,
    point_joints: wgpu::ComputePipeline,
    constraint: wgpu::ComputePipeline,
    spline_joints: wgpu::ComputePipeline,
    collide: wgpu::ComputePipeline,
    velocity_inner: wgpu::ComputePipeline,
    velocity_outer: wgpu::ComputePipeline,
}

impl HairPhysicsPipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.hair-physics.shader"),
            source: wgpu::ShaderSource::Wgsl(HAIR_PHYSICS_SHADER.into()),
        });
        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.hair-physics.main-layout"),
            entries: &[
                uniform_entry(0),
                storage_entry(1, false),
                storage_entry(2, true),
                storage_entry(3, true),
                storage_entry(4, true),
                storage_entry(5, true),
                storage_entry(6, true),
                storage_entry(7, true),
            ],
        });
        let phase_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.hair-physics.phase-layout"),
            entries: &[uniform_entry(0)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.hair-physics.pipeline-layout"),
            bind_group_layouts: &[Some(&main_layout), Some(&phase_layout)],
            immediate_size: 0,
        });
        let pipeline = |label: &'static str, entry_point: &'static str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            main_layout,
            phase_layout,
            integrate: pipeline("vkit.hair-physics.integrate", "integrate"),
            point_joints: pipeline("vkit.hair-physics.point-joints", "point_joints"),
            constraint: pipeline("vkit.hair-physics.constraint", "solve_constraint"),
            spline_joints: pipeline("vkit.hair-physics.spline-joints", "spline_joints"),
            collide: pipeline("vkit.hair-physics.collide", "collide"),
            velocity_inner: pipeline("vkit.hair-physics.velocity-inner", "velocity_inner"),
            velocity_outer: pipeline("vkit.hair-physics.velocity-outer", "velocity_outer"),
        }
    }
}

pub(crate) struct HairPhysicsScene {
    simulate: HairSimulation,
    preview: Arc<HairPreview>,
    mesh: Arc<SurfaceMesh>,
    mesh_revision: u64,
    particle_buffer: wgpu::Buffer,
    rest_buffer: wgpu::Buffer,
    _settings_buffer: wgpu::Buffer,
    _constraint_buffer: wgpu::Buffer,
    collider_buffer: wgpu::Buffer,
    guide_normal_buffer: wgpu::Buffer,
    render_segment_buffer: wgpu::Buffer,
    render_run_buffer: wgpu::Buffer,
    render_part_buffer: wgpu::Buffer,
    _strand_buffer: wgpu::Buffer,
    _capsule_buffer: wgpu::Buffer,
    physics_uniform: wgpu::Buffer,
    main_bind_group: wgpu::BindGroup,
    phases: Vec<PhaseDispatch>,
    particle_count: u32,
    render_segment_count: u32,
    render_runs: Vec<GpuDrawRun>,
    render_subdivisions: u32,

    settle_gravity: f32,

    frame: u32,
    last_time_seconds: Option<f64>,
    accumulator_seconds: f64,
}

impl HairPhysicsScene {
    pub(crate) fn new(
        device: &wgpu::Device,
        preview: Arc<HairPreview>,
        mesh: Arc<SurfaceMesh>,
        pipelines: &HairPhysicsPipelines,
        simulate: HairSimulation,
    ) -> Option<Self> {
        let storage_limit = device.limits().max_storage_buffer_binding_size as usize;
        let max_segment_bytes = device
            .limits()
            .max_storage_buffer_binding_size
            .min(device.limits().max_buffer_size)
            .min(u64::from(u32::MAX)) as usize;
        let max_segments = (max_segment_bytes / std::mem::size_of::<GpuHairRenderSegment>())
            .min(u32::MAX as usize / 6);
        let max_constraints = storage_limit / std::mem::size_of::<GpuConstraint>();
        let build_started = std::time::Instant::now();
        let data = build_scene_data(&preview, &mesh, max_segments, max_constraints, simulate)?;
        let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
        let upload_started = std::time::Instant::now();
        if data.rests.is_empty() || data.render_segments.is_empty() {
            return None;
        }

        let particle_bytes = data.particles.len() * std::mem::size_of::<GpuParticle>();
        let rest_bytes = data.rests.len() * std::mem::size_of::<GpuRestParticle>();
        if particle_bytes > storage_limit || rest_bytes > storage_limit {
            return None;
        }

        let particle_buffer = storage_buffer(
            device,
            "vkit.hair-physics.particles",
            &data.particles,
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        );
        let rest_buffer = storage_buffer(
            device,
            "vkit.hair-physics.rests",
            &data.rests,
            wgpu::BufferUsages::COPY_DST,
        );
        let guide_normal_buffer = storage_buffer(
            device,
            "vkit.hair-physics.guide-data",
            &data.guide_data,
            wgpu::BufferUsages::COPY_DST,
        );
        let settings_buffer = storage_buffer(
            device,
            "vkit.hair-physics.settings",
            &data.settings,
            wgpu::BufferUsages::empty(),
        );
        let constraints = nonempty_or_default(data.constraints);
        let constraint_buffer = storage_buffer(
            device,
            "vkit.hair-physics.constraints",
            &constraints,
            wgpu::BufferUsages::empty(),
        );
        let colliders = nonempty_or_default(data.colliders);
        let collider_buffer = storage_buffer(
            device,
            "vkit.hair-physics.colliders",
            &colliders,
            wgpu::BufferUsages::COPY_DST,
        );
        let render_run_buffer = storage_buffer(
            device,
            "vkit.hair.render-runs",
            &nonempty_or_default(data.render_runs.clone()),
            wgpu::BufferUsages::empty(),
        );
        let render_segment_buffer = storage_buffer(
            device,
            "vkit.hair.render-segments",
            &data.render_segments,
            wgpu::BufferUsages::empty(),
        );
        let render_part_buffer = storage_buffer(
            device,
            "vkit.hair.render-parts",
            &data.render_parts,
            wgpu::BufferUsages::empty(),
        );
        let strand_count = data.strands.len() as u32;
        let strands = nonempty_or_default(data.strands);
        let strand_buffer = storage_buffer(
            device,
            "vkit.hair-physics.strands",
            &strands,
            wgpu::BufferUsages::empty(),
        );
        let capsule_count = data.capsules.len() as u32;
        let capsules = nonempty_or_default(data.capsules);
        let capsule_buffer = storage_buffer(
            device,
            "vkit.hair-physics.capsules",
            &capsules,
            wgpu::BufferUsages::empty(),
        );
        let physics_uniform_data = GpuPhysicsUniform {
            fixed_dt: FIXED_STEP_SECONDS,
            particle_count: data.rests.len() as u32,
            collider_count: data.collider_count,
            settle_gravity: 0.0,
            frame: 0,
            capsule_count,
            strand_count,
            _pad: 0,
        };
        let physics_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vkit.hair-physics.uniform"),
            contents: bytemuck::bytes_of(&physics_uniform_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let main_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.hair-physics.main-bind-group"),
            layout: &pipelines.main_layout,
            entries: &[
                buffer_entry(0, &physics_uniform),
                buffer_entry(1, &particle_buffer),
                buffer_entry(2, &rest_buffer),
                buffer_entry(3, &settings_buffer),
                buffer_entry(4, &constraint_buffer),
                buffer_entry(5, &collider_buffer),
                buffer_entry(6, &strand_buffer),
                buffer_entry(7, &capsule_buffer),
            ],
        });
        let phases = build_phase_dispatches(
            device,
            &pipelines.phase_layout,
            data.rests.len() as u32,
            strand_count,
            data.max_iterations,
            &data.constraint_ranges,
        );
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Info,
            "hair",
            "physics_built",
            &format!(
                "particles={}; cling_joints={}; cling_groups={}; iterations={}; dispatches_per_step={}; workgroups_per_step={}",
                data.rests.len(),
                data.constraint_ranges
                    .iter()
                    .filter(|range| range.kind == 2)
                    .map(|range| range.count as usize)
                    .sum::<usize>(),
                data.constraint_ranges
                    .iter()
                    .filter(|range| range.kind == 2 && range.count > 0)
                    .count(),
                data.max_iterations,
                phases.len(),
                phases
                    .iter()
                    .map(|phase| u64::from(phase.workgroups))
                    .sum::<u64>(),
            ),
        );
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Info,
            "hair",
            "scene_timing",
            &format!(
                "build_ms={build_ms:.1}; upload_ms={:.1}",
                upload_started.elapsed().as_secs_f64() * 1000.0,
            ),
        );
        Some(Self {
            simulate,
            mesh_revision: mesh.revision,
            preview,
            mesh,
            particle_buffer,
            rest_buffer,
            _settings_buffer: settings_buffer,
            _constraint_buffer: constraint_buffer,
            collider_buffer,
            guide_normal_buffer,
            render_segment_buffer,
            render_run_buffer,
            render_part_buffer,
            _strand_buffer: strand_buffer,
            _capsule_buffer: capsule_buffer,
            physics_uniform,
            main_bind_group,
            phases,
            particle_count: data.rests.len() as u32,
            render_segment_count: data.render_segments.len() as u32,
            render_runs: data.render_runs,
            render_subdivisions: data.render_subdivisions,
            settle_gravity: 0.0,
            frame: 0,
            last_time_seconds: None,
            accumulator_seconds: 0.0,
        })
    }

    pub(crate) fn matches(
        &self,
        preview: &Arc<HairPreview>,
        mesh: &Arc<SurfaceMesh>,
        simulate: HairSimulation,
    ) -> bool {
        same_inputs(
            (&self.preview, &self.mesh, self.simulate),
            (preview, mesh, simulate),
        )
    }

    pub(crate) fn update_head_if_needed(&mut self, queue: &wgpu::Queue, mesh: Arc<SurfaceMesh>) {
        if self.mesh_revision == mesh.revision
            && self.mesh.topology_revision == mesh.topology_revision
            && self.mesh.mesh.vertices.len() == mesh.mesh.vertices.len()
        {
            self.mesh = mesh;
            return;
        }
        if let Some(rests) = build_rest_particles(&self.preview, &mesh)
            && rests.len() as u32 == self.particle_count
        {
            queue.write_buffer(&self.rest_buffer, 0, bytemuck::cast_slice(&rests));
            if let Some(guide_data) = build_guide_data(&self.preview, &mesh) {
                queue.write_buffer(
                    &self.guide_normal_buffer,
                    0,
                    bytemuck::cast_slice(&guide_data),
                );
            }

            if self.simulate == HairSimulation::Off {
                let particles = particles_from_rests(&rests);
                queue.write_buffer(&self.particle_buffer, 0, bytemuck::cast_slice(&particles));
            }
            let (field, _) = head_field_for_mesh(&mesh);
            queue.write_buffer(&self.collider_buffer, 0, bytemuck::cast_slice(&field));
            self.mesh_revision = mesh.revision;
            self.mesh = mesh;
        }
    }

    pub(crate) fn step(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipelines: &HairPhysicsPipelines,
        time_seconds: f64,
        settle_gravity: f32,
        solve: bool,
    ) {
        if !solve {
            self.last_time_seconds = Some(time_seconds);
            return;
        }
        // A morphing head is not a reset. `PartialReset(10)` fires on a
        // WorldScale change and on nothing else; a morph moves
        // `transforms[MatrixId]` and the strands follow through their point
        // joints without the solver being told anything. Resetting on a quiet
        // period after a sculpt edit dropped all inertia and froze the hair for
        // ten frames, so the settling the author was judging was not the
        // settling the game produces.
        let effective_gravity = settle_gravity;
        if (self.settle_gravity - effective_gravity).abs() > f32::EPSILON {
            // Physics off to on is the one case that really does reset: VaM
            // runs the whole reset kernel and snaps every particle to rest, so
            // the warmup starts from the beginning rather than ten frames in.
            if self.settle_gravity <= 0.0 && effective_gravity > 0.0 {
                self.frame = 0;
            }
            self.settle_gravity = effective_gravity;
            queue.write_buffer(
                &self.physics_uniform,
                std::mem::offset_of!(GpuPhysicsUniform, settle_gravity) as u64,
                bytemuck::bytes_of(&effective_gravity),
            );
        }
        if effective_gravity <= 0.0 && self.simulate == HairSimulation::Off {
            self.last_time_seconds = Some(time_seconds);
            return;
        }
        let Some(last_time) = self.last_time_seconds.replace(time_seconds) else {
            return;
        };
        let elapsed = (time_seconds - last_time).clamp(0.0, 0.1);
        self.accumulator_seconds += elapsed;
        let fixed = f64::from(FIXED_STEP_SECONDS);
        let steps = ((self.accumulator_seconds / fixed).floor() as usize).min(MAX_FRAME_STEPS);
        if steps == 0 {
            return;
        }
        self.accumulator_seconds = (self.accumulator_seconds - fixed * steps as f64).min(fixed);

        queue.write_buffer(
            &self.physics_uniform,
            std::mem::offset_of!(GpuPhysicsUniform, frame) as u64,
            bytemuck::bytes_of(&self.frame),
        );
        self.frame = self.frame.saturating_add(1);

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("vkit.hair-physics.pass"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, &self.main_bind_group, &[]);
        for _ in 0..steps {
            for phase in &self.phases {
                let pipeline = match phase.stage {
                    DispatchStage::Integrate => &pipelines.integrate,
                    DispatchStage::PointJoints => &pipelines.point_joints,
                    DispatchStage::Constraint => &pipelines.constraint,
                    DispatchStage::SplineJoints => &pipelines.spline_joints,
                    DispatchStage::Collide => &pipelines.collide,
                    DispatchStage::VelocityInner => &pipelines.velocity_inner,
                    DispatchStage::VelocityOuter => &pipelines.velocity_outer,
                };
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &phase.bind_group, &[]);
                pass.dispatch_workgroups(phase.workgroups, 1, 1);
            }
        }
    }

    pub(crate) fn particle_buffer(&self) -> &wgpu::Buffer {
        &self.particle_buffer
    }

    pub(crate) fn guide_normal_buffer(&self) -> &wgpu::Buffer {
        &self.guide_normal_buffer
    }

    pub(crate) fn render_segment_buffer(&self) -> &wgpu::Buffer {
        &self.render_segment_buffer
    }

    pub(crate) fn render_part_buffer(&self) -> &wgpu::Buffer {
        &self.render_part_buffer
    }

    pub(crate) const fn render_subdivisions(&self) -> u32 {
        self.render_subdivisions
    }

    /// Indices to draw with: six to a quad over
    /// [`crate::hair_renderer::HAIR_QUAD_CORNERS`] corners, two of them repeats
    /// the vertex stage no longer has to run.
    /// What a single flat draw would have needed — still the size of the shared
    /// quad index buffer, which has to cover the largest run.
    pub(crate) const fn render_index_count(&self) -> u32 {
        self.render_segment_count
            .saturating_mul(6 * self.render_subdivisions)
    }

    /// One draw per stride: how many indices it covers, and the instance slot
    /// that carries its stride and its first segment.
    pub(crate) fn render_runs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.render_runs
            .iter()
            .enumerate()
            .map(|(index, run)| (run.meta[2].saturating_mul(6 * run.meta[0]), index as u32))
    }

    pub(crate) fn render_run_buffer(&self) -> &wgpu::Buffer {
        &self.render_run_buffer
    }
}

/// What a hair scene is built from.
///
/// Kept so a build that produced NOTHING can be remembered as well as one that
/// produced something. A part with no strands used to fail the scene lookup
/// every frame, rebuild everything — the rest particles, the constraint graph,
/// and a 64-cubed distance field against the head — and be thrown away again,
/// for as long as it was on screen.
#[derive(Clone)]
pub(crate) struct SceneInputs {
    preview: Arc<HairPreview>,
    mesh: Arc<SurfaceMesh>,
    simulate: HairSimulation,
}

impl SceneInputs {
    #[must_use]
    pub(crate) const fn of(
        preview: Arc<HairPreview>,
        mesh: Arc<SurfaceMesh>,
        simulate: HairSimulation,
    ) -> Self {
        Self {
            preview,
            mesh,
            simulate,
        }
    }

    #[must_use]
    pub(crate) fn matches(
        &self,
        preview: &Arc<HairPreview>,
        mesh: &Arc<SurfaceMesh>,
        simulate: HairSimulation,
    ) -> bool {
        same_inputs(
            (&self.preview, &self.mesh, self.simulate),
            (preview, mesh, simulate),
        )
    }
}

/// The one rule for "these are the same inputs", used by the live scene and by
/// the record of an empty one alike.
fn same_inputs(
    held: (&Arc<HairPreview>, &Arc<SurfaceMesh>, HairSimulation),
    asked: (&Arc<HairPreview>, &Arc<SurfaceMesh>, HairSimulation),
) -> bool {
    let (held_preview, held_mesh, held_simulate) = held;
    let (preview, mesh, simulate) = asked;
    held_simulate == simulate
        && Arc::ptr_eq(held_preview, preview)
        && held_mesh.topology_revision == mesh.topology_revision
        && held_mesh.mesh.vertices.len() == mesh.mesh.vertices.len()
        && held_mesh.mesh.triangles.len() == mesh.mesh.triangles.len()
}

struct SceneData {
    particles: Vec<GpuParticle>,
    rests: Vec<GpuRestParticle>,
    settings: Vec<GpuPartSettings>,
    constraints: Vec<GpuConstraint>,
    constraint_ranges: Vec<ConstraintRange>,
    strands: Vec<GpuStrandRange>,
    capsules: Vec<GpuBodyCapsule>,

    colliders: Vec<GpuHeadFieldElement>,

    collider_count: u32,
    guide_data: Vec<GpuGuideData>,
    render_segments: Vec<GpuHairRenderSegment>,
    render_runs: Vec<GpuDrawRun>,
    render_parts: Vec<GpuHairRenderPart>,
    render_subdivisions: u32,
    max_iterations: u32,
}

#[derive(Clone, Copy)]
struct GuideRange {
    start: u32,
    count: u32,
}

fn build_scene_data(
    preview: &HairPreview,
    mesh: &SurfaceMesh,
    max_render_segments: usize,
    max_constraints: usize,
    simulate: HairSimulation,
) -> Option<SceneData> {
    if preview.parts.len() > MAX_PART_INDEX {
        return None;
    }
    let rests = build_rest_particles(preview, mesh)?;
    let guide_data = build_guide_data(preview, mesh)?;
    let particles = particles_from_rests(&rests);
    let settings = preview
        .parts
        .iter()
        .map(|part| gpu_settings(part, simulate))
        .collect::<Vec<_>>();
    let render_parts = preview
        .parts
        .iter()
        .map(|part| {
            let optics = part.optics;
            GpuHairRenderPart {
                root_color: [
                    part.root_color[0],
                    part.root_color[1],
                    part.root_color[2],
                    optics.color_rolloff,
                ],
                tip_color: [
                    part.tip_color[0],
                    part.tip_color[1],
                    part.tip_color[2],
                    optics.diffuse_softness,
                ],
                specular: [
                    optics.specular_color[0],
                    optics.specular_color[1],
                    optics.specular_color[2],
                    optics.specular_shift,
                ],
                lobes: [
                    optics.primary_specular_sharpness,
                    optics.secondary_specular_sharpness,
                    optics.fresnel_power,
                    optics.fresnel_attenuation,
                ],
                variation: [
                    optics.random_color_power,
                    optics.random_color_offset,
                    optics.ibl_factor,
                    optics.normal_randomize,
                ],

                width: [
                    part.width,
                    1.0,
                    optics.shader_type.id() as f32,
                    (part.waviness.allow_reverse as u32
                        | (part.waviness.allow_flip_axis as u32) << 1) as f32,
                ],
                waviness_a: [
                    part.waviness.vector_m[0] * part.metres_to_template,
                    part.waviness.vector_m[1] * part.metres_to_template,
                    part.waviness.vector_m[2] * part.metres_to_template,
                    part.waviness.scale,
                ],
                waviness_b: [
                    part.waviness.frequency * 100.0 / part.metres_to_template.max(1.0e-6),
                    part.waviness.scale_randomness,
                    part.waviness.frequency_randomness,
                    part.waviness.curve_power,
                ],
                waviness_c: [
                    part.waviness.root,
                    part.waviness.mid,
                    part.waviness.tip,
                    part.waviness.midpoint,
                ],
                waviness_d: [
                    part.waviness.normal_adjust * part.metres_to_template,
                    0.0,
                    0.0,
                    0.0,
                ],
                spread_a: [
                    part.spread.root,
                    part.spread.mid,
                    part.spread.tip,
                    part.spread.midpoint,
                ],
                spread_b: [
                    part.spread.curve_power,
                    part.spread.max_spread_m * part.metres_to_template,
                    part.strand_length_m.max(0.01),
                    part.curve_density.clamp(2, 64) as f32,
                ],
                lengths: [
                    optics.child_lengths[0],
                    optics.child_lengths[1],
                    optics.child_lengths[2],
                    LIGHT_CENTRE_DEPTH_M * part.metres_to_template,
                ],
            }
        })
        .collect::<Vec<_>>();
    let guide_ranges = guide_ranges(preview)?;

    let mut distance_even = Vec::new();
    let mut distance_odd = Vec::new();
    let mut bend = [Vec::new(), Vec::new(), Vec::new()];
    for (part_index, part_ranges) in guide_ranges.iter().enumerate() {
        for range in part_ranges {
            for point in 0..range.count.saturating_sub(1) {
                let a = range.start + point;
                let b = a + 1;
                let joint = constraint(a, b, part_index, 0, &rests);
                if point % 2 == 0 {
                    distance_even.push(joint);
                } else {
                    distance_odd.push(joint);
                }
            }
            for point in 0..range.count.saturating_sub(2) {
                let a = range.start + point;
                let b = a + 2;
                let mut joint = constraint(a, b, part_index, 1, &rests);

                joint.values[0] *= 0.95;
                bend[(point % 3) as usize].push(joint);
            }
        }
    }
    let cling = cling_constraints(preview, &guide_ranges, &rests);

    let mut groups = vec![
        (distance_even, 0_u32),
        (distance_odd, 0),
        (std::mem::take(&mut bend[0]), 1),
        (std::mem::take(&mut bend[1]), 1),
        (std::mem::take(&mut bend[2]), 1),
    ];

    for bucket in color_disjoint(cling, rests.len()) {
        groups.push((bucket, 2));
    }
    let mut constraints = Vec::new();
    let mut constraint_ranges = Vec::new();
    for (group, kind) in groups {
        let offset = constraints.len() as u32;
        let remaining = max_constraints.saturating_sub(constraints.len());
        let count = group.len().min(remaining) as u32;
        constraints.extend(group.into_iter().take(count as usize));
        constraint_ranges.push(ConstraintRange {
            offset,
            count,
            kind,
        });
    }

    let built = render_segments(preview, &guide_ranges, max_render_segments);
    let render_subdivisions = built.stride;
    let render_segments = built.segments;
    let render_runs = built.runs;
    let _ = crate::diagnostics::record(
        crate::diagnostics::Severity::Info,
        "hair",
        "scene_built",
        &format!(
            "particles={}; segments={}; stride={}; runs={}; quads={} (flat would be {}, {:.1}x)",
            rests.len(),
            render_segments.len(),
            render_subdivisions,
            render_runs.len(),
            built.run_quads,
            built.flat_quads,
            built.flat_quads as f64 / built.run_quads.max(1) as f64,
        ),
    );
    // Nothing to draw, so nothing to collide with. `head_field_for_mesh`
    // measures a 64-cubed grid against the head — 262,144 point-to-mesh
    // distances — and the caller was about to throw the result away.
    // Nothing to draw, so nothing to collide with. `head_field_for_mesh`
    // measures a 64-cubed grid against the head — 262,144 point-to-mesh
    // distances — and the caller was about to throw the result away.
    if rests.is_empty() || render_segments.is_empty() {
        return None;
    }
    let (colliders, collider_count) = head_field_for_mesh(mesh);
    let strands = guide_ranges
        .iter()
        .enumerate()
        .flat_map(|(part_index, ranges)| {
            ranges.iter().map(move |range| GpuStrandRange {
                meta: [range.start, range.count, part_index as u32, 0],
            })
        })
        .collect::<Vec<_>>();
    let capsules = preview
        .body_capsules
        .iter()
        .map(|capsule| GpuBodyCapsule {
            a: [capsule.a[0], capsule.a[1], capsule.a[2], capsule.radius],
            b: [capsule.b[0], capsule.b[1], capsule.b[2], 0.0],
        })
        .collect::<Vec<_>>();
    let max_iterations = preview
        .parts
        .iter()
        .map(|part| part.physics.iterations.clamp(1, MAX_VAM_ITERATIONS))
        .max()
        .unwrap_or(1);
    Some(SceneData {
        particles,
        rests,
        settings,
        constraints,
        constraint_ranges,
        strands,
        capsules,
        colliders,
        collider_count,
        guide_data,
        render_segments,
        render_runs,
        render_subdivisions,
        render_parts,
        max_iterations,
    })
}

fn particles_from_rests(rests: &[GpuRestParticle]) -> Vec<GpuParticle> {
    rests
        .iter()
        .map(|rest| GpuParticle {
            position: [rest.position[0], rest.position[1], rest.position[2], 1.0],
            previous: [rest.position[0], rest.position[1], rest.position[2], 1.0],
            inner: [rest.position[0], rest.position[1], rest.position[2], 0.0],
            velocity: [rest.position[0], rest.position[1], rest.position[2], 0.0],
        })
        .collect()
}

fn resolved_rigidity(
    physics: &vkit_core::vam::HairPhysicsSettings,
    painted: Option<f32>,
    point_index: usize,
    segments: usize,
) -> f32 {
    if point_index == 0 {
        return 1.0;
    }
    if physics.use_painted_rigidity
        && let Some(painted) = painted
    {
        return painted.clamp(0.0, 1.0);
    }
    vkit_core::vam::rolloff_rigidity(physics, point_index, segments)
}

fn collision_radius(physics: &vkit_core::vam::HairPhysicsSettings, point_index: usize) -> f32 {
    if point_index == 1 {
        physics.collision_radius_root_m.max(0.0)
    } else {
        physics.collision_radius_m.max(0.0)
    }
}

/// How many quads one segment of a strand is drawn with.
///
/// This has to agree exactly with the shader, which reads the density from
/// `spread_b.w` — clamped to 2..=64, NOT to the subdivision ceiling — and
/// divides it by the strand's segment count. Clamping first and dividing after
/// gives a smaller answer, and the shader then draws half the samples it wanted:
/// each quad spans an arc it was never meant to, and a curl comes out as flat
/// black shards.
/// The subdivision count, reachable from the renderer's contract test.
#[cfg(test)]
pub(crate) fn segment_subdivisions_for_test(curve_density: u32, segment_count: u32) -> u32 {
    segment_subdivisions(curve_density, segment_count)
}

fn segment_subdivisions(curve_density: u32, segment_count: u32) -> u32 {
    curve_density
        .clamp(2, 64)
        .div_ceil(segment_count.max(1))
        .clamp(1, MAX_RENDER_SUBDIVISIONS)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuGuideData {
    pub normal_phase: [f32; 4],

    /// The three curl randoms, and in `w` the chord of the guide **at rest**.
    pub rand: [f32; 4],
}

fn build_guide_data(preview: &HairPreview, mesh: &SurfaceMesh) -> Option<Vec<GpuGuideData>> {
    let mut data = Vec::new();
    for part in preview.parts.iter() {
        for guide in part.guides.iter() {
            let frame = binding_frame(&guide.binding, mesh)?;
            // How many turns a strand carries is measured on the rest chord —
            // the kernel reads GPPointJoint.Point for the first and last
            // particle, a different buffer from the live positions it takes the
            // rotation axis from. Reading the live chord instead makes the
            // ringlet count breathe as the lock stretches and compresses.
            let rest = deformed_guide_points(guide, mesh)?;
            let rest_chord = match (rest.first(), rest.last()) {
                (Some(first), Some(last)) => (*last - *first).length(),
                _ => 0.0,
            };
            let entry = GpuGuideData {
                normal_phase: [frame.3.x, frame.3.y, frame.3.z, guide.curl_phase],
                rand: [
                    guide.curl_rand[0],
                    guide.curl_rand[1],
                    guide.curl_rand[2],
                    rest_chord,
                ],
            };
            data.extend(std::iter::repeat_n(entry, guide.local_points.len()));
        }
    }
    Some(data)
}

fn build_rest_particles(preview: &HairPreview, mesh: &SurfaceMesh) -> Option<Vec<GpuRestParticle>> {
    let mut rests = Vec::new();
    for (part_index, part) in preview.parts.iter().enumerate() {
        for guide in part.guides.iter() {
            let points = deformed_guide_points(guide, mesh)?;
            let root_index = rests.len() as u32;
            let segments = points.len();
            let last = points.len().saturating_sub(1).max(1) as f32;
            for (point_index, point) in points.into_iter().enumerate() {
                let painted = guide.painted_rigidity.get(point_index).copied();
                rests.push(GpuRestParticle {
                    position: [point.x, point.y, point.z, point_index as f32 / last],
                    data: [
                        painted.unwrap_or(1.0),
                        resolved_rigidity(&part.physics, painted, point_index, segments),
                        collision_radius(&part.physics, point_index) * part.metres_to_template,
                        0.0,
                    ],
                    meta: [
                        part_index as u32,
                        root_index,
                        u32::from(point_index == 0),
                        0,
                    ],
                });
            }
        }
    }
    Some(rests)
}

fn guide_ranges(preview: &HairPreview) -> Option<Vec<Vec<GuideRange>>> {
    let mut next = 0_u32;
    let mut result = Vec::with_capacity(preview.parts.len());
    for part in &preview.parts {
        let mut ranges = Vec::with_capacity(part.guides.len());
        for guide in part.guides.iter() {
            let count = u32::try_from(guide.local_points.len()).ok()?;
            ranges.push(GuideRange { start: next, count });
            next = next.checked_add(count)?;
        }
        result.push(ranges);
    }
    Some(result)
}

fn gpu_settings(
    part: &crate::hair_preview::HairPreviewPart,
    simulate: HairSimulation,
) -> GpuPartSettings {
    let mut physics = part.physics;
    match simulate {
        HairSimulation::Off => {
            physics.simulation_enabled = false;
            physics.collision_enabled = false;
            physics.gravity_multiplier = 0.0;
        }
        HairSimulation::Every => physics.simulation_enabled = true,
    }
    let iterations = physics.iterations.clamp(1, MAX_VAM_ITERATIONS);
    GpuPartSettings {
        forces: [
            physics.gravity_multiplier,
            physics.drag.clamp(0.0, 1.0),
            physics.weight.max(0.0),
            f32::from(u8::from(physics.simulation_enabled)),
        ],
        wind: [
            physics.wind[0],
            physics.wind[1],
            physics.wind[2],
            physics.friction.clamp(0.0, 1.0),
        ],
        rigidity: [
            physics.root_rigidity,
            physics.main_rigidity,
            physics.tip_rigidity,
            physics.rigidity_rolloff_power,
        ],
        constraints: [
            physics.snap,
            physics.bend_resistance,
            physics.cling,
            physics.cling_rolloff,
        ],
        collision: [
            f32::from(u8::from(physics.collision_enabled)),
            physics.collision_radius_root_m.max(0.0) * part.metres_to_template,
            physics.collision_radius_m.max(0.0) * part.metres_to_template,
            iterations as f32,
        ],
        misc: [
            f32::from(u8::from(physics.use_painted_rigidity)),
            part.metres_to_template,
            0.0,
            0.0,
        ],
    }
}

fn constraint(a: u32, b: u32, part: usize, kind: u32, rests: &[GpuRestParticle]) -> GpuConstraint {
    let pa = Vec3::from_array(rests[a as usize].position[..3].try_into().expect("xyz"));
    let pb = Vec3::from_array(rests[b as usize].position[..3].try_into().expect("xyz"));
    GpuConstraint {
        meta: [a, b, part as u32, kind],
        values: [
            pa.distance(pb),
            (rests[a as usize].position[3] + rests[b as usize].position[3]) * 0.5,
            0.0,
            0.0,
        ],
    }
}

fn color_disjoint(joints: Vec<GpuConstraint>, particle_count: usize) -> Vec<Vec<GpuConstraint>> {
    let mut buckets: Vec<(Vec<GpuConstraint>, Vec<bool>)> = Vec::new();
    for joint in joints {
        let a = joint.meta[0] as usize;
        let b = joint.meta[1] as usize;
        if a >= particle_count || b >= particle_count {
            continue;
        }
        let slot = buckets
            .iter()
            .position(|(_, touched)| !touched[a] && !touched[b]);
        let slot = match slot {
            Some(slot) => slot,
            None => {
                buckets.push((Vec::new(), vec![false; particle_count]));
                buckets.len() - 1
            }
        };
        buckets[slot].0.push(joint);
        buckets[slot].1[a] = true;
        buckets[slot].1[b] = true;
    }
    buckets.into_iter().map(|(bucket, _)| bucket).collect()
}

fn cling_constraints(
    preview: &HairPreview,
    guide_ranges: &[Vec<GuideRange>],
    rests: &[GpuRestParticle],
) -> Vec<GpuConstraint> {
    let mut result = Vec::new();
    for (part_index, part) in preview.parts.iter().enumerate() {
        let Some(ranges) = guide_ranges.get(part_index) else {
            continue;
        };
        for joint in &part.nearby_joints {
            let (Some(range_a), Some(range_b)) = (
                ranges.get(joint.a_guide as usize),
                ranges.get(joint.b_guide as usize),
            ) else {
                continue;
            };
            if joint.a_point >= range_a.count || joint.b_point >= range_b.count {
                continue;
            }
            let mut cohesion = constraint(
                range_a.start + joint.a_point,
                range_b.start + joint.b_point,
                part_index,
                2,
                rests,
            );
            cohesion.values[1] = joint.elasticity;
            result.push(cohesion);
        }
    }
    result
}

/// The quads to draw, and how many of them each segment is worth.
///
/// The stride has to be the largest number of quads any one segment actually
/// asks for. Handing the draw the whole `curveDensity` instead issues that many
/// per segment and throws all but `curveDensity / segments` of them away — four
/// out of every five for a typical style, every frame, in the vertex shader.
/// One draw: a stride, and the run of segments that want it.
///
/// `[stride, first_segment, segment_count, 0]`, laid out for the shader to read
/// by `instance_index`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuDrawRun {
    pub meta: [u32; 4],
}

/// The segments to draw, grouped by how finely each one is subdivided.
///
/// It used to be one flat list drawn at the LARGEST subdivision count any
/// strand asked for, with the vertex stage throwing away every quad past each
/// strand's own count — four vertices invoked and discarded, per wasted quad.
/// The count is `ceil(curveDensity / segment_count)`, so it is largest for the
/// SHORTEST strands, and one short strand made every long one in the preset pay
/// its stride. Grouping costs a handful of draw calls and wastes nothing.
struct RenderSegments {
    segments: Vec<GpuHairRenderSegment>,
    runs: Vec<GpuDrawRun>,

    /// The largest stride in use, for sizing the shared index buffer.
    stride: u32,

    /// Quads a single-stride draw would have issued, and quads these runs
    /// issue. Logged so the saving is a measurement and not a claim.
    flat_quads: u64,
    run_quads: u64,
}

fn render_segments(
    preview: &HairPreview,
    ranges: &[Vec<GuideRange>],
    limit: usize,
) -> RenderSegments {
    // Bucketed by stride as they are built, so the flatten below is a
    // concatenation rather than a sort: segment order within a stride is the
    // order they were authored in.
    let mut buckets: std::collections::BTreeMap<u32, Vec<GpuHairRenderSegment>> =
        std::collections::BTreeMap::new();
    let mut produced = 0_usize;
    let mut subdivisions = 1_u32;
    'parts: for (part_index, part) in preview.parts.iter().enumerate() {
        for strand in part.strands.iter() {
            let (guide_indices, weights, length_weights) = match strand.source {
                HairStrandSource::Guide(guide) => {
                    ([guide, guide, guide], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
                }
                HairStrandSource::Interpolated {
                    guides,
                    barycentric,
                    length_barycentric,
                } => (guides, barycentric, length_barycentric),
            };
            let Some(guide_ranges) = guide_indices
                .map(|guide| {
                    ranges
                        .get(part_index)
                        .and_then(|part_ranges| part_ranges.get(guide as usize))
                        .copied()
                })
                .into_iter()
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let segment_count = guide_ranges
                .iter()
                .map(|range| range.count.saturating_sub(1) as usize)
                .min()
                .unwrap_or(0)
                .min(strand.point_count.saturating_sub(1) as usize)
                .min(MAX_SEGMENTS_PER_STRAND);
            if segment_count == 0 {
                continue;
            }
            let packed = (part_index as u32) | ((segment_count as u32) << 16);
            let mine = segment_subdivisions(part.curve_density, segment_count as u32);
            subdivisions = subdivisions.max(mine);
            let bucket = buckets.entry(mine).or_default();
            for segment in 0..segment_count {
                if produced >= limit {
                    break 'parts;
                }
                produced += 1;
                bucket.push(GpuHairRenderSegment {
                    particles: [
                        guide_ranges[0].start + segment as u32,
                        guide_ranges[1].start + segment as u32,
                        guide_ranges[2].start + segment as u32,
                        packed,
                    ],
                    weights: [
                        weights[0],
                        weights[1],
                        weights[2],
                        segment as f32 / segment_count as f32,
                    ],
                    slot: [length_weights[0], length_weights[1], length_weights[2], 0.0],
                });
            }
        }
    }
    let stride = subdivisions.min(MAX_RENDER_SUBDIVISIONS);
    let mut segments = Vec::with_capacity(produced);
    let mut runs = Vec::with_capacity(buckets.len());
    let mut run_quads = 0_u64;
    for (bucket_stride, bucket) in buckets {
        if bucket.is_empty() {
            continue;
        }
        let bucket_stride = bucket_stride.clamp(1, MAX_RENDER_SUBDIVISIONS);
        runs.push(GpuDrawRun {
            meta: [bucket_stride, segments.len() as u32, bucket.len() as u32, 0],
        });
        run_quads += bucket.len() as u64 * u64::from(bucket_stride);
        segments.extend(bucket);
    }
    RenderSegments {
        flat_quads: segments.len() as u64 * u64::from(stride),
        run_quads,
        segments,
        runs,
        stride,
    }
}

const HEAD_SDF_RESOLUTION: usize = 64;

const HEAD_SDF_MARGIN_CELLS: f32 = 3.0;

struct HeadSdfGrid {
    origin: Vec3,
    cell: f32,
    distances: Vec<f32>,
}

impl HeadSdfGrid {
    #[cfg(test)]
    fn sample(&self, point: Vec3) -> f32 {
        let grid = (point - self.origin) / self.cell;
        let base = grid.floor();
        let t = grid - base;
        let read = |x: i32, y: i32, z: i32| -> f32 {
            let last = HEAD_SDF_RESOLUTION as i32 - 1;
            let x = x.clamp(0, last) as usize;
            let y = y.clamp(0, last) as usize;
            let z = z.clamp(0, last) as usize;
            self.distances[(z * HEAD_SDF_RESOLUTION + y) * HEAD_SDF_RESOLUTION + x]
        };
        let mut total = 0.0;
        for k in 0..2 {
            let wz = if k == 1 { t.z } else { 1.0 - t.z };
            for j in 0..2 {
                let wy = if j == 1 { t.y } else { 1.0 - t.y };
                for i in 0..2 {
                    let wx = if i == 1 { t.x } else { 1.0 - t.x };
                    total += read(base.x as i32 + i, base.y as i32 + j, base.z as i32 + k)
                        * wx
                        * wy
                        * wz;
                }
            }
        }
        // The same far field the kernel uses.
        total + self.escape(point)
    }

    /// How far a point lies outside the grid's box, or zero inside it.
    #[cfg(test)]
    fn escape(&self, point: Vec3) -> f32 {
        let span = self.cell * (HEAD_SDF_RESOLUTION as f32 - 1.0);
        let outside = (self.origin - point).max(point - (self.origin + Vec3::splat(span)));
        outside.max(Vec3::ZERO).length()
    }
}

fn head_sdf_for_mesh(mesh: &SurfaceMesh) -> Option<HeadSdfGrid> {
    let visible = Mesh {
        vertices: mesh.mesh.vertices.clone(),
        triangles: (*mesh.render_triangles).clone(),
    };
    if visible.triangles.is_empty() {
        return None;
    }
    let projector = projector_for_mesh(&visible).ok()?;

    let mut minimum = Vec3::splat(f32::MAX);
    let mut maximum = Vec3::splat(f32::MIN);
    for triangle in &visible.triangles {
        for index in triangle {
            let Some(point) = visible.vertices.get(*index as usize) else {
                continue;
            };
            let point = Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32);
            minimum = minimum.min(point);
            maximum = maximum.max(point);
        }
    }
    let span = (maximum - minimum).max_element();
    if !span.is_finite() || span <= 1.0e-6 {
        return None;
    }
    let steps = HEAD_SDF_RESOLUTION as f32 - 1.0 - 2.0 * HEAD_SDF_MARGIN_CELLS;
    let cell = span / steps.max(1.0);
    let centre = (minimum + maximum) * 0.5;
    let origin = centre - Vec3::splat(cell * (HEAD_SDF_RESOLUTION as f32 - 1.0) * 0.5);

    let open_rim = open_rim_triangles(&visible);
    let distances = (0..HEAD_SDF_RESOLUTION.pow(3))
        .into_par_iter()
        .map(|index| {
            let x = index % HEAD_SDF_RESOLUTION;
            let y = (index / HEAD_SDF_RESOLUTION) % HEAD_SDF_RESOLUTION;
            let z = index / (HEAD_SDF_RESOLUTION * HEAD_SDF_RESOLUTION);
            let point = origin + Vec3::new(x as f32, y as f32, z as f32) * cell;
            signed_distance_to(&visible, &open_rim, &projector, point).unwrap_or(span)
        })
        .collect();

    Some(HeadSdfGrid {
        origin,
        cell,
        distances,
    })
}

/// Which triangles sit on an open edge of the mesh.
///
/// The head is cut off at the neck, so it is not a closed surface, and the
/// inside/outside test below — the sign of `offset · normal` against the
/// nearest face — is only meaningful where the surface actually closes. At the
/// rim of the opening the nearest triangle's normal points sideways while the
/// offset points down past it, so the dot product sits near zero and its sign
/// flips with whichever rim triangle happens to be closest. Neighbouring cells
/// come out `+d` and `-d`, and the field grows a ridged phantom surface in the
/// band below the neck — right under the trapezius, which is where hair was
/// being shoved back and forth and never settling.
fn open_rim_triangles(mesh: &Mesh) -> Vec<bool> {
    use std::collections::HashMap;

    let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
    for triangle in &mesh.triangles {
        for pair in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = (pair.0.min(pair.1), pair.0.max(pair.1));
            *uses.entry(edge).or_default() += 1;
        }
    }
    mesh.triangles
        .iter()
        .map(|triangle| {
            [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ]
            .into_iter()
            .any(|pair| {
                let edge = (pair.0.min(pair.1), pair.0.max(pair.1));
                uses.get(&edge).copied().unwrap_or(0) < 2
            })
        })
        .collect()
}

fn signed_distance_to(
    mesh: &Mesh,
    open_rim: &[bool],
    projector: &SurfaceProjector,
    point: Vec3,
) -> Option<f32> {
    let hit = projector
        .project([f64::from(point.x), f64::from(point.y), f64::from(point.z)])
        .ok()?;
    let surface = Vec3::new(
        hit.point[0] as f32,
        hit.point[1] as f32,
        hit.point[2] as f32,
    );
    let normal = mesh
        .triangles
        .get(hit.primitive_id as usize)
        .and_then(|triangle| face_normal(mesh, *triangle))?;
    let offset = point - surface;
    let distance = offset.length();
    // No sign where the surface does not close. Unsigned reads as "outside",
    // which is the honest answer below the neck: there is nothing there to be
    // inside of, and what does live there is the body's own colliders.
    if open_rim
        .get(hit.primitive_id as usize)
        .copied()
        .unwrap_or(false)
    {
        return Some(distance);
    }
    Some(if offset.dot(normal) < 0.0 {
        -distance
    } else {
        distance
    })
}

fn face_normal(mesh: &Mesh, triangle: [u32; 3]) -> Option<Vec3> {
    let corner = |index: u32| -> Option<Vec3> {
        mesh.vertices
            .get(index as usize)
            .map(|point| Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32))
    };
    let a = corner(triangle[0])?;
    let b = corner(triangle[1])?;
    let c = corner(triangle[2])?;
    let normal = (b - a).cross(c - a);
    (normal.length_squared() > 1.0e-20).then(|| normal.normalize())
}

/// The last head measured, kept so the next scene does not measure it again.
///
/// Every hair part builds a scene of its own, and every scene wanted this
/// field: seven parts, seven identical grids, off the same head. Each one is a
/// BVH over the head's triangles plus 262,144 signed-distance queries against
/// it, and the seven of them were the 0.77-second stall a preset load put on
/// the frame that loaded it.
///
/// One entry is enough — there is one head. The key is `SurfaceMesh::revision`,
/// which is a content hash of the vertices and the visible triangles, so a head
/// that moved gets a new grid and a head that did not gets the old one.
static HEAD_SDF: std::sync::Mutex<Option<(u64, Arc<HeadSdfGrid>)>> = std::sync::Mutex::new(None);

fn cached_head_sdf(mesh: &SurfaceMesh) -> Option<Arc<HeadSdfGrid>> {
    if let Ok(held) = HEAD_SDF.lock()
        && let Some((revision, grid)) = held.as_ref()
        && *revision == mesh.revision
    {
        return Some(Arc::clone(grid));
    }
    let grid = Arc::new(head_sdf_for_mesh(mesh)?);
    if let Ok(mut held) = HEAD_SDF.lock() {
        *held = Some((mesh.revision, Arc::clone(&grid)));
    }
    Some(grid)
}

fn head_field_for_mesh(mesh: &SurfaceMesh) -> (Vec<GpuHeadFieldElement>, u32) {
    let Some(grid) = cached_head_sdf(mesh) else {
        return (vec![GpuHeadFieldElement { lanes: [0.0; 4] }], 0);
    };
    let mut packed = Vec::with_capacity(2 + grid.distances.len().div_ceil(4));
    packed.push(GpuHeadFieldElement {
        lanes: [
            grid.origin.x,
            grid.origin.y,
            grid.origin.z,
            HEAD_SDF_RESOLUTION as f32,
        ],
    });
    packed.push(GpuHeadFieldElement {
        lanes: [grid.cell, 0.0, 0.0, 0.0],
    });
    for chunk in grid.distances.chunks(4) {
        let mut lanes = [0.0_f32; 4];
        lanes[..chunk.len()].copy_from_slice(chunk);
        packed.push(GpuHeadFieldElement { lanes });
    }
    let count = grid.distances.len() as u32;
    (packed, count)
}

fn deformed_guide_points(guide: &HairPreviewGuide, mesh: &SurfaceMesh) -> Option<Vec<Vec3>> {
    let frame = binding_frame(&guide.binding, mesh)?;
    Some(
        guide
            .local_points
            .iter()
            .map(|local| frame.0 + frame.1 * local[0] + frame.2 * local[1] + frame.3 * local[2])
            .collect(),
    )
}

fn binding_frame(
    binding: &HairRootBinding,
    mesh: &SurfaceMesh,
) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
    let [a, b, c] = binding.triangle.map(|index| {
        mesh.mesh
            .vertices
            .get(index as usize)
            .copied()
            .map(|point| Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32))
    });
    let [Some(a), Some(b), Some(c)] = [a, b, c] else {
        return None;
    };
    let tangent = (b - a).try_normalize()?;
    let normal = (b - a).cross(c - a).try_normalize()?;
    let bitangent = normal.cross(tangent);
    let weights = binding.barycentric;
    let root = a * weights[0] + b * weights[1] + c * weights[2] + normal * binding.normal_offset;
    Some((root, tangent, bitangent, normal))
}

fn build_phase_dispatches(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    particle_count: u32,
    strand_count: u32,
    iterations: u32,
    constraint_ranges: &[ConstraintRange],
) -> Vec<PhaseDispatch> {
    let mut result = Vec::new();
    let particle_phase = |stage, iteration| {
        (
            stage,
            GpuPhaseUniform {
                offset: 0,
                count: particle_count,
                kind: 0,
                iteration,
            },
        )
    };
    let push = |result: &mut Vec<PhaseDispatch>, (stage, uniform)| {
        result.push(phase_dispatch(device, layout, stage, uniform));
    };
    let distance_ranges = || {
        constraint_ranges
            .iter()
            .filter(|range| range.count > 0 && range.kind == 0)
    };
    let shape_ranges = || {
        constraint_ranges
            .iter()
            .filter(|range| range.count > 0 && range.kind != 0)
    };
    let constraint_phase = |range: &ConstraintRange, iteration| {
        (
            DispatchStage::Constraint,
            GpuPhaseUniform {
                offset: range.offset,
                count: range.count,
                kind: range.kind,
                iteration,
            },
        )
    };
    for iteration in 0..iterations {
        push(
            &mut result,
            particle_phase(DispatchStage::Integrate, iteration),
        );
        push(
            &mut result,
            particle_phase(DispatchStage::PointJoints, iteration),
        );
        for range in distance_ranges() {
            push(&mut result, constraint_phase(range, iteration));
        }
        for range in shape_ranges() {
            push(&mut result, constraint_phase(range, iteration));
        }
        for range in distance_ranges() {
            push(&mut result, constraint_phase(range, iteration));
        }
        if strand_count > 0 {
            push(
                &mut result,
                (
                    DispatchStage::SplineJoints,
                    GpuPhaseUniform {
                        offset: 0,
                        count: strand_count,
                        kind: 0,
                        iteration,
                    },
                ),
            );
        }
        for range in distance_ranges() {
            push(&mut result, constraint_phase(range, iteration));
        }
        push(
            &mut result,
            particle_phase(DispatchStage::VelocityInner, iteration),
        );
    }
    push(&mut result, particle_phase(DispatchStage::Collide, 0));
    push(&mut result, particle_phase(DispatchStage::PointJoints, 0));
    push(&mut result, particle_phase(DispatchStage::VelocityOuter, 0));
    result
}

fn phase_dispatch(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    stage: DispatchStage,
    uniform: GpuPhaseUniform,
) -> PhaseDispatch {
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vkit.hair-physics.phase-uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vkit.hair-physics.phase-bind-group"),
        layout,
        entries: &[buffer_entry(0, &buffer)],
    });
    PhaseDispatch {
        stage,
        _uniform: buffer,
        bind_group,
        workgroups: uniform.count.div_ceil(WORKGROUP_SIZE).max(1),
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn storage_buffer<T: Pod>(
    device: &wgpu::Device,
    label: &'static str,
    values: &[T],
    additional_usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(values),
        usage: wgpu::BufferUsages::STORAGE | additional_usage,
    })
}

fn nonempty_or_default<T: Pod + Zeroable + Copy>(values: Vec<T>) -> Vec<T> {
    if values.is_empty() {
        vec![T::zeroed()]
    } else {
        values
    }
}

#[cfg(test)]
mod tests {

    use super::*;

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
    fn the_physics_pods_are_the_same_size_on_both_sides() {
        for (declared, expected) in [
            ("PhysicsUniform", std::mem::size_of::<GpuPhysicsUniform>()),
            ("PhaseUniform", std::mem::size_of::<GpuPhaseUniform>()),
            ("Particle", std::mem::size_of::<GpuParticle>()),
            ("RestParticle", std::mem::size_of::<GpuRestParticle>()),
            ("PartSettings", std::mem::size_of::<GpuPartSettings>()),
            ("Constraint", std::mem::size_of::<GpuConstraint>()),
        ] {
            assert_eq!(
                wgsl_struct_size("hair-physics", HAIR_PHYSICS_SHADER, declared),
                expected,
                "{declared} differs between Rust and WGSL"
            );
        }
    }

    /// An open mesh has no inside near its opening, and the field must not
    /// claim one.
    ///
    /// The sign comes from the nearest face's normal, which only means anything
    /// where the surface closes. At the rim of the neck the normal points
    /// sideways while the offset points down past it, so the dot product sits
    /// near zero and flips with whichever rim triangle wins — neighbouring
    /// cells read `+d` and `-d`, and the band below the neck grows a ridged
    /// phantom surface for hair to be shoved back and forth across.
    #[test]
    fn a_surface_that_does_not_close_gets_no_inside() {
        // A single triangle: every edge is open, so nothing may read negative.
        let mesh = Mesh {
            vertices: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]],
            triangles: vec![[0, 1, 2]],
        };
        let open = open_rim_triangles(&mesh);
        assert_eq!(open, vec![true], "a lone triangle is all rim");

        let projector = projector_for_mesh(&mesh).expect("a projector");
        for below in [-0.5_f32, -2.0, -8.0] {
            let point = Vec3::new(3.0, below, 3.0);
            let reading = signed_distance_to(&mesh, &open, &projector, point).expect("a reading");
            assert!(
                reading > 0.0,
                "at {below} under an open sheet the field claimed an inside: {reading}",
            );
        }
    }

    /// Below the head there is no collider, and the field has to say so.
    ///
    /// The cell fetch clamps at the grid edge, which outside the box repeats
    /// the boundary slab for ever — the head's lowest cross-section extruded
    /// straight down through the neck and out into the shoulders. Hair hanging
    /// there was being held up by a column nobody built, and a joint resting on
    /// its ridge got pushed out and pulled back every step, for ever.
    #[test]
    fn the_head_field_does_not_extrude_a_phantom_column_below_the_head() {
        let mesh = sphere_mesh(Vec3::new(0.0, 160.0, 0.0), 9.0);
        let grid = head_sdf_for_mesh(&mesh).expect("a field");
        let span = grid.cell * (HEAD_SDF_RESOLUTION as f32 - 1.0);
        let centre = grid.origin + Vec3::splat(span * 0.5);

        // Straight down from the middle of the head, a long way past the box.
        for drop in [1.5_f32, 3.0, 8.0] {
            let below = Vec3::new(centre.x, grid.origin.y - span * drop, centre.z);
            let reach = grid.origin.y - below.y;
            assert!(
                grid.sample(below) >= reach - grid.cell,
                "a point {reach:.1} below the box reads {:.2} away",
                grid.sample(below),
            );
        }
        // And inside, the field is untouched — the far term is zero there.
        assert!(
            grid.sample(centre) < span,
            "the middle of the head stopped reading as near it",
        );
    }

    #[test]
    #[ignore = "needs a GPU adapter"]
    fn a_settle_drops_the_strands_without_blowing_them_out() {
        let Some((device, queue)) = test_device() else {
            eprintln!("no adapter; skipping");
            return;
        };

        let head = sphere_mesh(Vec3::ZERO, 10.0);
        let preview = Arc::new(two_strand_preview());
        let pipelines = HairPhysicsPipelines::new(&device);
        let mut scene = HairPhysicsScene::new(
            &device,
            Arc::clone(&preview),
            Arc::clone(&head),
            &pipelines,
            HairSimulation::Every,
        )
        .expect("the scene builds");

        let before = read_particles(&device, &queue, &scene);

        let mut time = 0.0_f64;
        for _ in 0..150 {
            time += 1.0 / 60.0;
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vkit.test.hair"),
            });
            scene.step(
                &queue,
                &mut encoder,
                &pipelines,
                time,
                crate::state::HAIR_SETTLE_GRAVITY,
                true,
            );
            queue.submit([encoder.finish()]);
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        let after = read_particles(&device, &queue, &scene);

        let styled = 0..11_usize;
        let loose = 11..22_usize;

        for root in [styled.start, loose.start] {
            let moved = (after[root] - before[root]).length();
            assert!(moved < 0.01, "root {root} stayed put: moved {moved}");
        }

        let loose_drop = before[loose.end - 1].y - after[loose.end - 1].y;
        assert!(
            loose_drop > 5.0,
            "the unstyled strand hangs: fell {loose_drop:.2}"
        );

        let styled_drop = before[styled.end - 1].y - after[styled.end - 1].y;
        assert!(
            styled_drop > 0.005,
            "the styled strand felt gravity at all: fell {styled_drop:.3}"
        );
        assert!(
            styled_drop < loose_drop * 0.9,
            "and its rigidity resisted: styled fell {styled_drop:.2} against loose {loose_drop:.2}"
        );

        for (range, label) in [(styled.clone(), "styled"), (loose.clone(), "loose")] {
            let root = after[range.start];
            let length = (before[range.end - 1] - before[range.start]).length();
            let reach = after[range.clone()]
                .iter()
                .map(|point| (*point - root).length())
                .fold(0.0_f32, f32::max);
            assert!(
                reach < length * 1.5,
                "{label} blew out: reach {reach:.2} against {length:.2}"
            );
        }

        let deepest = after
            .iter()
            .map(|point| point.length())
            .fold(f32::INFINITY, f32::min);
        assert!(deepest > 9.5, "sank into the head: closest {deepest:.2}");
        eprintln!(
            "settle: styled fell {styled_drop:.2}, loose fell {loose_drop:.2}, closest {deepest:.2}"
        );
    }

    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vkit.test"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    const SPHERE_RING: u32 = 96;

    const SPHERE_SIDE: u32 = 24 * SPHERE_RING;

    fn sphere_mesh(centre: Vec3, radius: f32) -> Arc<SurfaceMesh> {
        let rings = 48_u32;
        let mut vertices = Vec::new();
        for ring in 0..=rings {
            let elevation = std::f32::consts::PI * ring as f32 / rings as f32;
            for step in 0..rings * 2 {
                let azimuth = std::f32::consts::TAU * step as f32 / (rings * 2) as f32;
                let point = centre
                    + Vec3::new(
                        elevation.sin() * azimuth.cos(),
                        elevation.cos(),
                        elevation.sin() * azimuth.sin(),
                    ) * radius;
                vertices.push([f64::from(point.x), f64::from(point.y), f64::from(point.z)]);
            }
        }
        let triangles = (0..vertices.len() as u32 / 3)
            .map(|index| [index * 3, index * 3 + 1, index * 3 + 2])
            .collect();
        Arc::new(
            SurfaceMesh::new(Mesh {
                vertices,
                triangles,
            })
            .expect("a sphere is a mesh"),
        )
    }

    /// The same head is measured once, however many hair parts want it.
    ///
    /// Seven parts meant seven identical distance fields, each a BVH over the
    /// head plus 262,144 queries against it. That was the 0.77-second stall on
    /// the frame a preset loaded.
    #[test]
    fn one_head_is_measured_once_however_many_parts_ask() {
        let mesh = sphere_mesh(Vec3::ZERO, 1.0);

        let first = cached_head_sdf(&mesh).expect("a sphere has a field");
        let second = cached_head_sdf(&mesh).expect("and the cache still has it");
        assert!(
            Arc::ptr_eq(&first, &second),
            "the second part measured the head again",
        );

        // A head that moved is a different head, and gets its own field.
        let moved = sphere_mesh(Vec3::new(0.0, 3.0, 0.0), 1.0);
        assert_ne!(moved.revision, mesh.revision, "the fixture must differ");
        let after = cached_head_sdf(&moved).expect("the moved head has a field");
        assert!(!Arc::ptr_eq(&first, &after));
        assert!(
            (after.origin.y - first.origin.y).abs() > 1.0,
            "the new field is measured where the head now is",
        );
    }

    /// A preview with nothing to draw refuses BEFORE the expensive half.
    ///
    /// This is what the log caught: eighty-seven consecutive frames of
    /// `particles=0; segments=0`. A part with no strands failed the scene
    /// lookup, rebuilt everything a scene needs, found it had no segments, and
    /// was discarded — then did it again next frame. The costly part is
    /// `head_field_for_mesh`, a 64-cubed grid of point-to-mesh distances, and
    /// it ran every one of those times for a result nobody kept.
    #[test]
    fn a_preview_with_no_strands_stops_before_the_distance_field() {
        use crate::hair_preview::HairPreviewStrand;

        let mut preview = two_strand_preview();
        for part in &mut preview.parts {
            part.strands = Arc::new(Vec::<HairPreviewStrand>::new());
        }
        let mesh = sphere_mesh(Vec3::ZERO, 1.0);

        // `build_scene_data` itself refuses. The check used to live one level
        // up, in `HairPhysicsScene::new`, which meant the field had already
        // been built by the time anybody noticed there was nothing to draw —
        // so moving it back there fails here, which is the ordering this pins.
        assert!(
            build_scene_data(&preview, &mesh, usize::MAX, usize::MAX, HairSimulation::Off)
                .is_none(),
            "an empty preview must be refused before the distance field is built",
        );
    }

    /// The two records of "what this was built from" answer alike.
    #[test]
    fn an_empty_record_and_a_live_scene_agree_on_what_changed() {
        let preview = Arc::new(two_strand_preview());
        let mesh = sphere_mesh(Vec3::ZERO, 1.0);
        let inputs = SceneInputs::of(Arc::clone(&preview), Arc::clone(&mesh), HairSimulation::Off);

        assert!(inputs.matches(&preview, &mesh, HairSimulation::Off));
        assert!(
            !inputs.matches(&preview, &mesh, HairSimulation::Every),
            "a change of simulation is a change of scene"
        );

        let same_shape = Arc::new(two_strand_preview());
        assert!(
            !inputs.matches(&same_shape, &mesh, HairSimulation::Off),
            "a rebuilt preview is a different preview, whatever it holds"
        );
    }

    /// Every segment lands in a run whose stride is its OWN subdivision count.
    ///
    /// The draw used to be one flat range at the largest count any strand asked
    /// for, and the vertex stage discarded the rest — four vertices invoked per
    /// wasted quad. `ceil(curveDensity / segment_count)` is largest for the
    /// SHORTEST strands, so one short strand made every long strand in the
    /// preset pay its stride. This is the fixture where that hurts: two parts,
    /// one dense and one sparse.
    #[test]
    fn each_segment_is_drawn_at_its_own_subdivision_and_no_finer() {
        let mut preview = two_strand_preview();
        // Same guides, different densities: the second wants eight times the
        // subdivisions of the first over the same segments.
        preview.parts[0].curve_density = 4;
        preview.parts[1].curve_density = 64;

        let ranges = guide_ranges(&preview).expect("the fixture has guides");
        let built = render_segments(&preview, &ranges, usize::MAX);

        assert!(built.runs.len() >= 2, "two densities, two strides");

        // Every run's segments, checked against the count that run promises.
        let mut covered = 0_usize;
        for run in &built.runs {
            let [stride, first, count, _] = run.meta;
            assert!((1..=MAX_RENDER_SUBDIVISIONS).contains(&stride));
            for index in 0..count as usize {
                let segment = built.segments[first as usize + index];
                let packed = segment.particles[3];
                let part = &preview.parts[(packed & 0xffff) as usize];
                let segment_count = (packed >> 16).max(1);
                assert_eq!(
                    stride,
                    segment_subdivisions(part.curve_density, segment_count),
                    "a segment is in the wrong run",
                );
            }
            covered += count as usize;
        }
        assert_eq!(
            covered,
            built.segments.len(),
            "a segment fell out of every run"
        );

        // The saving, stated as the number it is.
        assert!(
            built.run_quads < built.flat_quads,
            "grouping issued {} quads and the flat draw issued {}",
            built.run_quads,
            built.flat_quads,
        );
    }

    /// One density everywhere means one run and nothing saved — and nothing
    /// lost either, which is the case that must not regress.
    #[test]
    fn a_preset_of_one_density_is_a_single_run() {
        let mut preview = two_strand_preview();
        preview.parts[1].curve_density = preview.parts[0].curve_density;
        let ranges = guide_ranges(&preview).expect("the fixture has guides");
        let built = render_segments(&preview, &ranges, usize::MAX);

        assert_eq!(built.runs.len(), 1);
        assert_eq!(built.run_quads, built.flat_quads);
    }

    fn two_strand_preview() -> HairPreview {
        use crate::hair_preview::{HairPreviewPart, HairPreviewStrand};
        use vkit_core::vam::{HairOpticalSettings, HairPhysicsSettings};

        let points: Vec<[f32; 3]> = (0..11).map(|index| [0.0, 0.0, index as f32]).collect();
        let guide = |triangle: [u32; 3]| HairPreviewGuide {
            binding: HairRootBinding {
                triangle,
                barycentric: [1.0, 0.0, 0.0],
                normal_offset: 0.0,
                base_tangent: [1.0, 0.0, 0.0],
                base_bitangent: [0.0, 0.0, 1.0],
                base_normal: [0.0, 1.0, 0.0],
            },
            local_points: points.clone(),
            painted_rigidity: vec![1.0; points.len()],
            curl_rand: [0.5, 0.5, 0.5],
            curl_phase: 0.0,
        };
        let part = |guide: HairPreviewGuide, physics: HairPhysicsSettings| HairPreviewPart {
            curve_density: 4,
            guides: Arc::new(vec![guide]),
            strands: Arc::new(vec![HairPreviewStrand {
                point_count: points.len() as u32,
                source: HairStrandSource::Guide(0),
            }]),
            root_color: [0.1; 3],
            tip_color: [0.1; 3],
            width: 0.01,
            metres_to_template: 1.0,
            optics: HairOpticalSettings::default(),
            physics,
            waviness: Default::default(),
            spread: Default::default(),
            strand_length_m: 0.3,
            nearby_joints: Vec::new(),
        };

        let styled = HairPhysicsSettings {
            simulation_enabled: false,
            gravity_multiplier: 0.0,
            ..HairPhysicsSettings::default()
        };
        let loose = HairPhysicsSettings {
            root_rigidity: 0.0,
            main_rigidity: 0.0,
            tip_rigidity: 0.0,
            snap: 0.0,
            bend_resistance: 0.0,
            cling: 0.0,
            ..styled
        };
        HairPreview {
            parts: vec![
                part(
                    guide([SPHERE_SIDE, SPHERE_SIDE - SPHERE_RING, SPHERE_SIDE + 1]),
                    styled,
                ),
                part(
                    guide([
                        SPHERE_SIDE + 24,
                        SPHERE_SIDE + 24 - SPHERE_RING,
                        SPHERE_SIDE + 25,
                    ]),
                    loose,
                ),
            ],
            scalps: Vec::new(),
            body_capsules: Vec::new(),
        }
    }

    fn read_particles(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &HairPhysicsScene,
    ) -> Vec<Vec3> {
        let size = (scene.particle_count as usize * std::mem::size_of::<GpuParticle>()) as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vkit.test.readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vkit.test.copy"),
        });
        encoder.copy_buffer_to_buffer(&scene.particle_buffer, 0, &staging, 0, size);
        queue.submit([encoder.finish()]);
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let view = staging.slice(..).get_mapped_range();
        let particles: &[GpuParticle] = bytemuck::cast_slice(&view);
        let points = particles
            .iter()
            .map(|particle| {
                Vec3::new(
                    particle.position[0],
                    particle.position[1],
                    particle.position[2],
                )
            })
            .collect();
        drop(view);
        staging.unmap();
        points
    }

    fn sphere_shell(centre: Vec3, radius: f32, rings: u32) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let mut vertices = Vec::new();
        let columns = rings * 2;
        for ring in 0..=rings {
            let elevation = std::f32::consts::PI * ring as f32 / rings as f32;
            for step in 0..columns {
                let azimuth = std::f32::consts::TAU * step as f32 / columns as f32;
                let point = centre
                    + Vec3::new(
                        elevation.sin() * azimuth.cos(),
                        elevation.cos(),
                        elevation.sin() * azimuth.sin(),
                    ) * radius;
                vertices.push([f64::from(point.x), f64::from(point.y), f64::from(point.z)]);
            }
        }
        let mut triangles = Vec::new();
        for ring in 0..rings {
            for step in 0..columns {
                let next = (step + 1) % columns;
                let a = ring * columns + step;
                let b = ring * columns + next;
                let c = (ring + 1) * columns + step;
                let d = (ring + 1) * columns + next;
                triangles.push([a, d, c]);
                triangles.push([a, b, d]);
            }
        }
        (vertices, triangles)
    }

    #[test]
    fn the_head_field_measures_the_surface_it_was_built_from() {
        let centre = Vec3::new(3.0, -2.0, 1.5);
        let radius = 7.0_f32;
        let (vertices, triangles) = sphere_shell(centre, radius, 48);
        let mesh = SurfaceMesh::new(Mesh {
            vertices,
            triangles,
        })
        .expect("a sphere is a mesh");

        let grid = head_sdf_for_mesh(&mesh).expect("a sphere has a field");
        let tolerance = grid.cell * 1.2;

        for step in 0..64 {
            let angle = std::f32::consts::TAU * step as f32 / 64.0;
            let direction = Vec3::new(angle.cos(), (angle * 0.37).sin(), angle.sin()).normalize();
            for offset in [-2.0_f32, -1.0, -0.4, 0.0, 0.4] {
                let point = centre + direction * (radius + offset);
                let read = grid.sample(point);
                assert!(
                    (read - offset).abs() < tolerance,
                    "at {offset} from the surface the field read {read} (tolerance {tolerance})",
                );
            }

            // Past the margin the read clamps to it, and the margin is positive:
            // far from the head is never a collision, whichever way you left.
            let far = centre + direction * (radius * 4.0);
            assert!(
                grid.sample(far) > 0.0,
                "a point well outside the box must still read as open space",
            );
        }
    }

    #[test]
    fn a_crevice_stays_open_where_a_star_field_would_fill_it() {
        let radius = 7.0_f32;
        let left = Vec3::new(-5.0, 0.0, 0.0);
        let right = Vec3::new(5.0, 0.0, 0.0);
        let (mut vertices, mut triangles) = sphere_shell(left, radius, 48);
        let (other_vertices, other_triangles) = sphere_shell(right, radius, 48);
        let offset = vertices.len() as u32;
        vertices.extend(other_vertices);
        triangles.extend(
            other_triangles
                .into_iter()
                .map(|triangle| triangle.map(|index| index + offset)),
        );
        let mesh = SurfaceMesh::new(Mesh {
            vertices,
            triangles,
        })
        .expect("two spheres are a mesh");

        let grid = head_sdf_for_mesh(&mesh).expect("two spheres have a field");
        let tolerance = grid.cell * 1.5;

        // The seam sits between the two spheres, outside both, and a star field
        // built from the pair's centre reports it buried under the far surfaces.
        for height in [5.5_f32, 6.5, 7.5] {
            let seam = Vec3::new(0.0, height, 0.0);
            let truth = (seam - left).length().min((seam - right).length()) - radius;
            let read = grid.sample(seam);
            assert!(
                (read - truth).abs() < tolerance,
                "at height {height} the crevice reads {read} where the surface is {truth} away",
            );
            assert!(
                read > 0.0,
                "the crevice at height {height} is open space, not solid",
            );
        }

        let buried = Vec3::new(0.0, 0.0, 0.0);
        assert!(
            grid.sample(buried) < 0.0,
            "the overlap of the two spheres is solid",
        );
    }

    #[test]
    fn a_part_is_tessellated_as_finely_as_the_shader_will_ask_for() {
        // The shader's own arithmetic, which this has to agree with exactly:
        // it reads the density clamped to 2..=64 and divides by the segments.
        let shader_wants = |density: u32, segments: u32| {
            let density = density.clamp(2, 64) as f32;
            (density / segments as f32).ceil() as u32
        };
        for density in [0_u32, 1, 2, 4, 8, 16, 24, 32, 64, 200] {
            for segments in [1_u32, 2, 4, 8, 15, 23, 40] {
                let ours = segment_subdivisions(density, segments);
                let theirs = shader_wants(density, segments).clamp(1, MAX_RENDER_SUBDIVISIONS);
                assert_eq!(
                    ours, theirs,
                    "density {density} over {segments} segments: the stride has to be what                      the shader asks for, or it draws half the samples and a curl comes out                      as flat shards",
                );
            }
        }
        // The whole point of computing it per segment rather than per part.
        assert!(
            segment_subdivisions(16, 4) < 16,
            "sixteen points spread over four segments is four apiece, not sixteen",
        );
    }

    #[test]
    fn render_segment_layout_stays_compact() {
        assert_eq!(std::mem::size_of::<GpuHairRenderSegment>(), 48);
        assert_eq!(std::mem::align_of::<GpuHairRenderSegment>(), 4);
        assert_eq!(std::mem::size_of::<GpuHairRenderPart>(), 208);
        assert_eq!(std::mem::size_of::<GpuHairRenderPart>() % 16, 0);
    }

    #[test]
    fn phase_graph_coloring_keeps_adjacent_distance_joints_apart() {
        let points = (0..7)
            .map(|index| GpuRestParticle {
                position: [index as f32, 0.0, 0.0, index as f32 / 6.0],
                data: [1.0, 0.0, 0.0, 0.0],
                meta: [0, 0, u32::from(index == 0), 0],
            })
            .collect::<Vec<_>>();
        for parity in 0..2 {
            let joints = (parity..6)
                .step_by(2)
                .map(|index| constraint(index, index + 1, 0, 0, &points))
                .collect::<Vec<_>>();
            let mut touched = std::collections::BTreeSet::new();
            for joint in joints {
                assert!(touched.insert(joint.meta[0]));
                assert!(touched.insert(joint.meta[1]));
            }
        }
    }

    #[test]
    fn guide_frame_follows_a_deformed_scalp_triangle() {
        let mesh = SurfaceMesh::new(
            Mesh::new(
                vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap();
        let guide = HairPreviewGuide {
            binding: HairRootBinding {
                triangle: [0, 1, 2],
                barycentric: [0.5, 0.25, 0.25],
                normal_offset: 0.0,
                base_tangent: [1.0, 0.0, 0.0],
                base_bitangent: [0.0, 1.0, 0.0],
                base_normal: [0.0, 0.0, 1.0],
            },
            local_points: vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            painted_rigidity: vec![1.0, 1.0],
            curl_rand: [0.5, 0.5, 0.5],
            curl_phase: 0.0,
        };
        let points = deformed_guide_points(&guide, &mesh).unwrap();
        assert!(points[0].distance(Vec3::new(-0.25, 0.25, 0.0)) < 1.0e-6);
        assert!(points[1].distance(Vec3::new(-1.25, 0.25, 0.0)) < 1.0e-6);
    }

    #[test]
    fn the_mode_governs_every_route_gravity_can_take() {
        let preview = two_strand_preview();
        let mut part = preview.parts[0].clone();
        part.physics.simulation_enabled = true;
        part.physics.collision_enabled = true;
        part.physics.gravity_multiplier = 4.0;

        let off = gpu_settings(&part, HairSimulation::Off);
        assert!(
            off.forces[3] < 0.5,
            "simulate flag survived off: {:?}",
            off.forces
        );
        assert!(
            (off.forces[0]).abs() < f32::EPSILON,
            "gravity survived off: {:?}",
            off.forces
        );
        assert!(
            off.collision[0] < 0.5,
            "collision survived off: {:?}",
            off.collision
        );

        let on = gpu_settings(&part, HairSimulation::Every);
        assert!(
            on.forces[3] >= 0.5,
            "simulate flag lost on: {:?}",
            on.forces
        );
        assert!(
            (on.forces[0] - 4.0).abs() < f32::EPSILON,
            "gravity lost on: {:?}",
            on.forces
        );
        assert!(
            on.collision[0] >= 0.5,
            "collision lost on: {:?}",
            on.collision
        );
    }

    #[test]
    fn the_viewport_switch_overrules_a_part_that_would_stand_still() {
        let preview = two_strand_preview();
        let mut part = preview.parts[0].clone();
        part.physics.simulation_enabled = false;
        part.physics.gravity_multiplier = 4.0;

        let off = gpu_settings(&part, HairSimulation::Off);
        assert!(
            off.forces[3] < 0.5,
            "with the switch off nothing moves: {:?}",
            off.forces
        );

        let every = gpu_settings(&part, HairSimulation::Every);
        assert!(
            every.forces[3] >= 0.5,
            "the viewport switch answers for every part: {:?}",
            every.forces
        );
        assert!(
            (every.forces[0] - 4.0).abs() < f32::EPSILON,
            "the part keeps its own numbers, only the switch changes: {:?}",
            every.forces
        );
        assert!(
            !part.physics.simulation_enabled,
            "the export setting is read, never written"
        );
    }
}

#[cfg(test)]
mod face_field_probe {
    use super::*;

    /// A closed box whose +z face is a shallow dish with a small bump in it —
    /// the two things a face has that a sphere does not: a concavity a hair can
    /// sit inside, and a protrusion beside it.
    ///
    /// `dish` is how deep the concavity goes (into the box), `bump` how far the
    /// nose stands out.
    fn face_box(dish: f32, bump: f32, steps: usize) -> Mesh {
        let half = 10.0_f32;
        let depth = 10.0_f32;
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();

        // Front surface: a grid over x,y with z modulated.
        let at = |i: usize, j: usize| -> u32 { (j * (steps + 1) + i) as u32 };
        for j in 0..=steps {
            for i in 0..=steps {
                let u = i as f32 / steps as f32 * 2.0 - 1.0;
                let v = j as f32 / steps as f32 * 2.0 - 1.0;
                let radial = (u * u + v * v).sqrt().min(1.0);
                // A dish across the whole face, plus a narrow bump at the centre.
                let sink = -dish * (1.0 - radial * radial);
                let nose = bump * (-(radial * 6.0) * (radial * 6.0)).exp();
                vertices.push([
                    f64::from(u * half),
                    f64::from(v * half),
                    f64::from(sink + nose),
                ]);
            }
        }
        for j in 0..steps {
            for i in 0..steps {
                triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        // Back plate and a skirt, so the mesh is closed and the sign is defined.
        let back_base = vertices.len() as u32;
        for j in 0..=steps {
            for i in 0..=steps {
                let u = i as f32 / steps as f32 * 2.0 - 1.0;
                let v = j as f32 / steps as f32 * 2.0 - 1.0;
                vertices.push([f64::from(u * half), f64::from(v * half), f64::from(-depth)]);
            }
        }
        let back = |i: usize, j: usize| -> u32 { back_base + (j * (steps + 1) + i) as u32 };
        for j in 0..steps {
            for i in 0..steps {
                triangles.push([back(i, j), back(i + 1, j + 1), back(i + 1, j)]);
                triangles.push([back(i, j), back(i, j + 1), back(i + 1, j + 1)]);
            }
        }
        for i in 0..steps {
            triangles.push([at(i, 0), back(i, 0), at(i + 1, 0)]);
            triangles.push([at(i + 1, 0), back(i, 0), back(i + 1, 0)]);
            triangles.push([at(i, steps), at(i + 1, steps), back(i, steps)]);
            triangles.push([at(i + 1, steps), back(i + 1, steps), back(i, steps)]);
            triangles.push([at(0, i), at(0, i + 1), back(0, i)]);
            triangles.push([at(0, i + 1), back(0, i + 1), back(0, i)]);
            triangles.push([at(steps, i), back(steps, i), at(steps, i + 1)]);
            triangles.push([at(steps, i + 1), back(steps, i), back(steps, i + 1)]);
        }
        Mesh {
            vertices,
            triangles,
        }
    }

    #[test]
    fn open_air_in_front_of_a_face_is_never_solid() {
        for (label, dish, bump) in [
            ("deep features", 1.0_f32, 3.0_f32),
            ("shallow features", 0.3, 0.8),
            ("very shallow", 0.1, 0.3),
        ] {
            let mesh = SurfaceMesh::new(face_box(dish, bump, 40)).expect("a face is a mesh");
            let grid = head_sdf_for_mesh(&mesh).expect("a face has a field");
            let mut worst: Option<(Vec3, f32)> = None;
            // Sweep the air in front of the whole face.
            for gy in -9..=9 {
                for gx in -9..=9 {
                    for gz in 1..=40 {
                        let point = Vec3::new(gx as f32, gy as f32, bump + gz as f32 * 0.25);
                        let read = grid.sample(point);
                        if read < 0.0 && worst.is_none_or(|(_, held)| read < held) {
                            worst = Some((point, read));
                        }
                    }
                }
            }
            assert!(
                worst.is_none(),
                "{label}: the field calls open air solid at {:?} (read {:?})",
                worst.map(|(p, _)| p),
                worst.map(|(_, r)| r),
            );
        }
    }
}
