use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use egui::{Rect, epaint};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::mesh::MeshRenderResources;
use super::mip::MipBlit;
use super::shaders::SKIN_SHADER;
use super::{
    RenderDepthScope, SceneUniform, SmoothedPositionCache, evict_lru_scenes, lighting_uniform_data,
    normal_matrix, rgba8_srgb_to_linear, sanitized_light_yaw,
};
use crate::{
    lighting::{LightingPreset, sanitize_brightness},
    scene::SurfaceMesh,
    skin_preview::{SkinChannel, SkinPreview, SkinUvOrientation},
};

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
    pub spot: crate::renderer::SceneSpot,
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
    pub fn paint_callback(
        mut self,
        rect: Rect,
        spot: crate::renderer::SceneSpot,
    ) -> epaint::PaintCallback {
        self.spot = spot;
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

        let Some(mut pass) = crate::renderer::begin_scene_layer(
            device,
            _egui_encoder,
            callback_resources,
            _screen_descriptor,
            self.spot,
        ) else {
            return Vec::new();
        };
        if self.depth_scope.resets_before_draw()
            && let Some(mesh_resources) = callback_resources.get::<MeshRenderResources>()
        {
            mesh_resources.reset_depth(&mut pass);
        }
        if let Some(resources) = callback_resources.get::<SkinRenderResources>() {
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

const SKIN_SCENE_CACHE_CAP: usize = 24;

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

pub(super) const SKIN_TEXTURE_COUNT: usize = 16;

const COLOUR_TEXTURE_COUNT: usize = 10;

pub(super) const SKIN_SAMPLER_BINDING: u32 = SKIN_TEXTURE_COUNT as u32 + 1;

struct GpuSkinTextures {
    views: [Arc<wgpu::TextureView>; SKIN_TEXTURE_COUNT],
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
    pub(super) fn new(
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

        let pipelines = |format: wgpu::TextureFormat| {
            let attachment = SkinAttachment {
                format,
                sample_count,
            };
            let module = &shader;
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
            screen: pipelines(target_format),
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

    pub(crate) fn prepare_scene(
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

    pub(crate) fn paint(&self, render_pass: &mut wgpu::RenderPass<'static>, scene_key: u64) {
        let Some(scene) = self.scenes.get(&scene_key) else {
            return;
        };

        if scene.vertex_count == 0 {
            return;
        }
        let pipelines = &self.screen;
        render_pass.set_bind_group(0, &scene.bind_group, &[]);
        render_pass.set_vertex_buffer(0, scene.vertex_buffer.slice(..));
        render_pass.set_pipeline(&pipelines.opaque);
        render_pass.draw(0..scene.vertex_count, 0..1);
        render_pass.set_pipeline(&pipelines.transparent);
        render_pass.draw(0..scene.vertex_count, 0..1);
    }
}

pub(super) const fn channel_bit(channel: SkinChannel) -> u32 {
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

pub(super) fn skin_render_uvs(
    channel: SkinChannel,
    uv: [f32; 2],
    source_orientation: SkinUvOrientation,
) -> ([f32; 2], [f32; 2]) {
    (channel.texture_uv(uv, source_orientation), uv)
}

fn skin_channel_tint(skin: &SkinPreview, channel: SkinChannel) -> [f32; 4] {
    skin_channel_tint_data(&skin.auxiliary_colors, &skin.auxiliary_textured, channel)
}

pub(super) fn skin_channel_tint_data(
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

pub(super) fn skin_visibility_mask(
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
