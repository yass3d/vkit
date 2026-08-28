use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use egui::epaint;
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::shaders::{DEPTH_RESET_SHADER, SHADER};
use super::{
    RenderDepthScope, SceneUniform, SmoothedPositionCache, evict_lru_scenes, lighting_uniform_data,
    normal_matrix, rgba_srgb_to_linear, sanitized_light_yaw,
};
use crate::{
    lighting::{LightingPreset, sanitize_brightness},
    scene::SurfaceMesh,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct RenderVertex {
    pub(super) position: [f32; 3],
    pub(super) normal: [f32; 3],
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderStyle {
    #[default]
    Solid,
    Wire,
    Xray,

    Overlay,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarkerInstance {
    pub position: [f32; 3],
    pub radius: f32,
    pub fill: [f32; 4],
    pub ring: [f32; 4],

    pub shape: f32,
}

const MARKER_BUFFERS: [wgpu::VertexBufferLayout<'static>; 1] = [MarkerInstance::layout()];
const LINE_BUFFERS: [wgpu::VertexBufferLayout<'static>; 1] = [LineInstance::layout()];

impl MarkerInstance {
    pub const ROUND: f32 = 0.0;

    pub const SQUARE: f32 = 1.0;

    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32,
    ];

    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineInstance {
    pub from_position: [f32; 3],
    pub from_width: f32,
    pub to_position: [f32; 3],
    pub to_width: f32,
    pub from_colour: [f32; 4],
    pub to_colour: [f32; 4],
}

impl LineInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32,
        2 => Float32x3,
        3 => Float32,
        4 => Float32x4,
        5 => Float32x4,
    ];

    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkerLayer {
    Bed = 0,
    Over = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MeshPassKind {
    SolidOpaque,
    TranslucentDepthPrepass,
    TranslucentColor,
    Wire,
    Xray,
}

pub(super) const fn mesh_pass_sequence(
    style: RenderStyle,
    translucent: bool,
) -> &'static [MeshPassKind] {
    match (style, translucent) {
        (RenderStyle::Solid, false) => &[MeshPassKind::SolidOpaque],
        (RenderStyle::Solid, true) => &[
            MeshPassKind::TranslucentDepthPrepass,
            MeshPassKind::TranslucentColor,
        ],
        (RenderStyle::Wire, _) => &[MeshPassKind::Wire],
        (RenderStyle::Xray, _) => &[MeshPassKind::Xray],
        (RenderStyle::Overlay, _) => &[MeshPassKind::TranslucentDepthPrepass],
    }
}

impl MeshPassKind {
    #[cfg(test)]
    pub(crate) const fn writes_depth_without_colour(self) -> bool {
        let config = mesh_pipeline_config(self);
        config.depth_write_enabled && config.write_mask.is_empty()
    }
}

#[cfg(test)]
pub(crate) fn mesh_pass_shape_for_test(style: RenderStyle) -> Vec<bool> {
    mesh_pass_sequence(style, false)
        .iter()
        .map(|kind| kind.writes_depth_without_colour())
        .collect()
}

pub(super) fn mesh_color_is_translucent(color: [f32; 4]) -> bool {
    color[3] < 1.0
}

#[derive(Clone)]
pub struct MeshPaintCallback {
    pub spot: crate::renderer::SceneSpot,
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

    pub viewport_pixels: [f32; 2],

    pub lines: Option<Arc<Vec<LineInstance>>>,

    pub bed_markers: Option<Arc<Vec<MarkerInstance>>>,

    pub markers: Option<Arc<Vec<MarkerInstance>>>,
}

impl MeshPaintCallback {
    pub fn paint_callback(self) -> epaint::PaintCallback {
        Callback::new_paint_callback(self.spot.rect, self)
    }
}

impl CallbackTrait for MeshPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        crate::renderer::sync_scene_samples(device, queue, callback_resources);
        let Some(resources) = callback_resources.get_mut::<MeshRenderResources>() else {
            return Vec::new();
        };
        resources.prepare_scene(device, queue, self);
        resources.upload_markers(
            device,
            queue,
            self.scene_key,
            MarkerLayer::Bed,
            self.bed_markers.as_ref(),
        );
        resources.upload_markers(
            device,
            queue,
            self.scene_key,
            MarkerLayer::Over,
            self.markers.as_ref(),
        );
        resources.upload_lines(device, queue, self.scene_key, self.lines.as_ref());

        let Some(mut pass) = crate::renderer::begin_scene_layer(
            device,
            egui_encoder,
            callback_resources,
            screen_descriptor,
            self.spot,
        ) else {
            return Vec::new();
        };
        if let Some(resources) = callback_resources.get::<MeshRenderResources>() {
            if self.depth_scope.resets_before_draw() {
                resources.reset_depth(&mut pass);
            }
            resources.paint(
                &mut pass,
                self.scene_key,
                self.style,
                mesh_color_is_translucent(self.color),
            );
            resources.paint_markers(&mut pass, self.scene_key, MarkerLayer::Bed);
            resources.paint_lines(&mut pass, self.scene_key);
            resources.paint_markers(&mut pass, self.scene_key, MarkerLayer::Over);
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

const MESH_SCENE_CACHE_CAP: usize = 128;

pub(super) fn fill_render_vertices(
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

    marker_buffers: [Option<wgpu::Buffer>; 2],
    marker_capacities: [usize; 2],
    marker_counts: [u32; 2],

    line_buffer: Option<wgpu::Buffer>,
    line_capacity: usize,
    line_count: u32,
}

pub(super) struct MeshRenderResources {
    solid_pipeline: wgpu::RenderPipeline,
    wire_pipeline: wgpu::RenderPipeline,
    xray_pipeline: wgpu::RenderPipeline,
    translucent_prepass_pipeline: wgpu::RenderPipeline,
    translucent_color_pipeline: wgpu::RenderPipeline,
    depth_reset_pipeline: wgpu::RenderPipeline,
    marker_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    target_is_srgb: bool,
    bind_group_layout: wgpu::BindGroupLayout,
    scenes: BTreeMap<u64, GpuScene>,

    vertex_scratch: Vec<RenderVertex>,
    normal_scratch: Vec<Vec3>,
    smoothing_cache: SmoothedPositionCache,
    use_counter: u64,
}

impl MeshRenderResources {
    pub(super) fn new(
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
        let marker_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.mesh.marker_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                crate::renderer::shaders::MARKER_SHADER,
            )),
        });
        let marker_pipeline = create_marker_pipeline(
            device,
            &marker_shader,
            &layout,
            target_format,
            sample_count,
            depth_format,
        );
        let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.mesh.line_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(crate::renderer::shaders::LINE_SHADER)),
        });
        let line_pipeline = create_overlay_pipeline(
            device,
            &line_shader,
            &layout,
            target_format,
            sample_count,
            depth_format,
            OverlayPipeline {
                label: "vkit.mesh.lines",
                vertex_entry: "vs_line",
                fragment_entry: "fs_line",
                buffers: &LINE_BUFFERS,
            },
        );

        Self {
            solid_pipeline,
            wire_pipeline,
            xray_pipeline,
            translucent_prepass_pipeline,
            translucent_color_pipeline,
            depth_reset_pipeline,
            marker_pipeline,
            line_pipeline,
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
            grading: [
                callback.tone_mapping.shader_flag(),
                callback.viewport_pixels[0],
                callback.viewport_pixels[1],
                0.0,
            ],
            punctual_meta: light.punctual_meta,
            punctual: light.punctual,
        };
        queue.write_buffer(&scene.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn upload_markers(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_key: u64,
        layer: MarkerLayer,
        markers: Option<&Arc<Vec<MarkerInstance>>>,
    ) {
        let Some(scene) = self.scenes.get_mut(&scene_key) else {
            return;
        };
        let slot = layer as usize;
        let markers = markers.map(|held| held.as_slice()).unwrap_or(&[]);
        scene.marker_counts[slot] = markers.len().min(u32::MAX as usize) as u32;
        if markers.is_empty() {
            return;
        }
        if scene.marker_capacities[slot] < markers.len() {
            let wanted = markers.len().next_power_of_two();
            scene.marker_buffers[slot] = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vkit.mesh.markers"),
                size: (wanted * std::mem::size_of::<MarkerInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            scene.marker_capacities[slot] = wanted;
        }
        if let Some(buffer) = scene.marker_buffers[slot].as_ref() {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(markers));
        }
    }

    fn upload_lines(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_key: u64,
        lines: Option<&Arc<Vec<LineInstance>>>,
    ) {
        let Some(scene) = self.scenes.get_mut(&scene_key) else {
            return;
        };
        let lines = lines.map(|held| held.as_slice()).unwrap_or(&[]);
        scene.line_count = lines.len().min(u32::MAX as usize) as u32;
        if lines.is_empty() {
            return;
        }
        if scene.line_capacity < lines.len() {
            let wanted = lines.len().next_power_of_two();
            scene.line_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vkit.mesh.lines"),
                size: (wanted * std::mem::size_of::<LineInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            scene.line_capacity = wanted;
        }
        if let Some(buffer) = scene.line_buffer.as_ref() {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(lines));
        }
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
            marker_buffers: [None, None],
            marker_capacities: [0, 0],
            marker_counts: [0, 0],
            line_buffer: None,
            line_capacity: 0,
            line_count: 0,
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

    fn paint_lines(&self, render_pass: &mut wgpu::RenderPass<'static>, scene_key: u64) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };
        let (Some(buffer), 1..) = (scene.line_buffer.as_ref(), scene.line_count) else {
            return;
        };
        render_pass.set_pipeline(&self.line_pipeline);
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..6, 0..scene.line_count);
    }

    fn paint_markers(
        &self,
        render_pass: &mut wgpu::RenderPass<'static>,
        scene_key: u64,
        layer: MarkerLayer,
    ) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };
        let slot = layer as usize;
        let count = scene.marker_counts[slot];
        let (Some(buffer), 1..) = (scene.marker_buffers[slot].as_ref(), count) else {
            return;
        };
        render_pass.set_pipeline(&self.marker_pipeline);
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..6, 0..count);
    }

    pub(super) fn reset_depth(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        render_pass.set_pipeline(&self.depth_reset_pipeline);
        render_pass.draw(0..3, 0..1);
    }
}

pub(super) struct MeshPipelineConfig {
    pub(super) topology: wgpu::PrimitiveTopology,
    pub(super) depth_write_enabled: bool,
    pub(super) fragment_entry: &'static str,
    pub(super) blend: Option<wgpu::BlendState>,
    pub(super) write_mask: wgpu::ColorWrites,
    pub(super) label: &'static str,
}

pub(super) const fn mesh_pipeline_config(kind: MeshPassKind) -> MeshPipelineConfig {
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

struct OverlayPipeline {
    label: &'static str,
    vertex_entry: &'static str,
    fragment_entry: &'static str,
    buffers: &'static [wgpu::VertexBufferLayout<'static>],
}

fn create_marker_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
    depth_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_overlay_pipeline(
        device,
        shader,
        layout,
        target_format,
        sample_count,
        depth_format,
        OverlayPipeline {
            label: "vkit.mesh.markers",
            vertex_entry: "vs_marker",
            fragment_entry: "fs_marker",
            buffers: &MARKER_BUFFERS,
        },
    )
}

fn create_overlay_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
    depth_format: wgpu::TextureFormat,
    config: OverlayPipeline,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(config.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(config.vertex_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: config.buffers,
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
            depth_write_enabled: Some(false),
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
mod marker_tests {
    use super::MarkerInstance;

    #[test]
    fn the_instance_matches_what_the_shader_declares() {
        assert_eq!(
            std::mem::size_of::<MarkerInstance>(),
            (3 + 1 + 4 + 4 + 1) * std::mem::size_of::<f32>(),
            "the instance has padding the shader does not expect",
        );
        let layout = MarkerInstance::layout();
        assert_eq!(
            layout.step_mode,
            wgpu::VertexStepMode::Instance,
            "per-vertex stepping would give every marker the first one's colour",
        );
        assert_eq!(
            layout.array_stride,
            std::mem::size_of::<MarkerInstance>() as wgpu::BufferAddress,
        );

        let offsets: Vec<u64> = layout.attributes.iter().map(|a| a.offset).collect();
        assert_eq!(offsets, vec![0, 12, 16, 32, 48], "{offsets:?}");
        let locations: Vec<u32> = layout
            .attributes
            .iter()
            .map(|a| a.shader_location)
            .collect();
        assert_eq!(locations, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_default_instance_draws_nothing() {
        let blank = MarkerInstance::default();
        assert_eq!(blank.radius, 0.0);
        assert_eq!(blank.fill[3], 0.0, "a transparent marker is no marker");
    }
}
