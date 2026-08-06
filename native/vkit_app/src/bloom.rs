use egui::{Rect, epaint};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};

use crate::ambient_occlusion::{AmbientOcclusion, AmbientOcclusionSettings, AoView};
use crate::hdr_target::{HDR_FORMAT, SceneHdrTarget};
use crate::post_process::BloomSettings;
use crate::renderer::SceneTarget;
use crate::shader_color::color_grading_wgsl;

const MIN_LEVEL_EXTENT: u32 = 8;

const UNIFORM_STRIDE: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniform {
    texel: [f32; 2],

    uv_scale: [f32; 2],

    curve: [f32; 4],

    params: [f32; 4],
}

pub(crate) const BLOOM_WGSL: &str = concat!(
    color_grading_wgsl!(),
    r#"
struct Uniforms {
    texel: vec2<f32>,
    uv_scale: vec2<f32>,
    curve: vec4<f32>,
    params: vec4<f32>,
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> VertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.uv = uv;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

fn downsample_box(uv: vec2<f32>, texel: vec2<f32>) -> vec3<f32> {
    let offset = vec4<f32>(texel, texel) * vec4<f32>(-1.0, -1.0, 1.0, 1.0);
    var sum = textureSampleLevel(source_texture, source_sampler, uv + offset.xy, 0.0).rgb;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.zy, 0.0).rgb;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.xw, 0.0).rgb;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.zw, 0.0).rgb;
    return sum * 0.25;
}

fn upsample_tent(uv: vec2<f32>, texel: vec2<f32>, scale: f32) -> vec3<f32> {
    let offset = vec4<f32>(texel, texel) * vec4<f32>(1.0, 1.0, -1.0, 0.0) * scale;
    var sum = textureSampleLevel(source_texture, source_sampler, uv - offset.xy, 0.0).rgb;
    sum += textureSampleLevel(source_texture, source_sampler, uv - offset.wy, 0.0).rgb * 2.0;
    sum += textureSampleLevel(source_texture, source_sampler, uv - offset.zy, 0.0).rgb;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.zw, 0.0).rgb * 2.0;
    sum += textureSampleLevel(source_texture, source_sampler, uv, 0.0).rgb * 4.0;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.xw, 0.0).rgb * 2.0;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.zy, 0.0).rgb;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.wy, 0.0).rgb * 2.0;
    sum += textureSampleLevel(source_texture, source_sampler, uv + offset.xy, 0.0).rgb;
    return sum * (1.0 / 16.0);
}

fn quadratic_threshold(color: vec3<f32>) -> vec3<f32> {
    let brightest = max(max(color.r, color.g), color.b);
    var soft = clamp(brightest - uniforms.curve.x, 0.0, uniforms.curve.y);
    soft = uniforms.curve.z * soft * soft;
    let keep = max(soft, brightest - uniforms.curve.w);
    return color * keep / max(brightest, 1.0e-5);
}

@fragment
fn fs_prefilter(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = in.uv * uniforms.uv_scale;

    let occlusion = textureSampleLevel(occlusion_texture, source_sampler, uv, 0.0).r;
    let color = sanitize_radiance(downsample_box(uv, uniforms.texel)) * occlusion;
    return vec4<f32>(quadratic_threshold(color), 1.0);
}

@fragment
fn fs_downsample(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(downsample_box(in.uv, uniforms.texel), 1.0);
}

@fragment
fn fs_upsample(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(upsample_tent(in.uv, uniforms.texel, uniforms.params.x), 1.0);
}
"#,
    r#"
@group(1) @binding(0) var scene_texture: texture_2d<f32>;
@group(1) @binding(1) var occlusion_texture: texture_2d<f32>;

@fragment
fn fs_overlay(in: VertexOut) -> @location(0) vec4<f32> {
    let source_uv = in.uv * uniforms.uv_scale;

    let occlusion = textureSampleLevel(occlusion_texture, source_sampler, source_uv, 0.0).r;
    let scene = sanitize_radiance(
        textureSampleLevel(scene_texture, source_sampler, source_uv, 0.0).rgb
    ) * occlusion;
    let bloom = textureSampleLevel(source_texture, source_sampler, source_uv, 0.0).rgb
        * uniforms.params.y;
    let exposure = uniforms.params.z;
    let filmic = uniforms.params.w > 0.5;
    let with_bloom = graded_display(scene + bloom, exposure, filmic, true);
    let without_bloom = graded_display(scene, exposure, filmic, true);

    return vec4<f32>(max(with_bloom - without_bloom, vec3<f32>(0.0)), 0.0);
}
"#
);

struct BloomLevel {
    view: wgpu::TextureView,
    extent: (u32, u32),

    down: wgpu::BindGroup,

    up: Option<wgpu::BindGroup>,
}

pub struct BloomResources {
    prefilter: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    upsample: wgpu::RenderPipeline,
    overlay: wgpu::RenderPipeline,
    filter_layout: wgpu::BindGroupLayout,
    scene_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,

    pub target: SceneHdrTarget,

    occlusion: AmbientOcclusion,

    live: (u32, u32),
    pyramid: Option<wgpu::Texture>,
    levels: Vec<BloomLevel>,

    overlay_scene: Option<wgpu::BindGroup>,
    overlay_bloom: Option<wgpu::BindGroup>,

    built_for: Option<((u32, u32), u32)>,

    queue: Vec<HdrDraw>,

    armed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HdrDraw {
    Skin(u64),
    Scalp(u64),
    Hair(u64),
}

const UNIFORM_SLOTS: u64 = crate::post_process::MAX_PYRAMID_LEVELS as u64 * 2 + 1;

impl BloomResources {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.bloom.shader"),
            source: wgpu::ShaderSource::Wgsl(BLOOM_WGSL.into()),
        });
        let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.bloom.filter-layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<BloomUniform>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.bloom.scene-layout"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let filter_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vkit.bloom.filter-pipeline-layout"),
                bind_group_layouts: &[Some(&filter_layout)],
                immediate_size: 0,
            });
        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vkit.bloom.overlay-pipeline-layout"),
                bind_group_layouts: &[Some(&filter_layout), Some(&scene_layout)],
                immediate_size: 0,
            });

        let filter = |entry: &'static str, label: &'static str| {
            let layout = if entry == "fs_prefilter" {
                &overlay_pipeline_layout
            } else {
                &filter_pipeline_layout
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,

                        blend: (entry == "fs_upsample").then_some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
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
            prefilter: filter("fs_prefilter", "vkit.bloom.prefilter"),
            downsample: filter("fs_downsample", "vkit.bloom.downsample"),
            upsample: filter("fs_upsample", "vkit.bloom.upsample"),
            overlay: overlay_pipeline(device, &shader, &overlay_pipeline_layout, surface_format),
            filter_layout,
            scene_layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vkit.bloom.sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }),
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vkit.bloom.uniforms"),
                size: UNIFORM_STRIDE * UNIFORM_SLOTS,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            target: SceneHdrTarget::new(device, width, height),
            occlusion: AmbientOcclusion::new(device, queue, surface_format),
            live: (width.max(1), height.max(1)),
            pyramid: None,
            levels: Vec::new(),
            overlay_scene: None,
            overlay_bloom: None,
            built_for: None,
            queue: Vec::new(),
            armed: false,
        }
    }

    pub fn arm(&mut self) {
        self.armed = true;
        self.queue.clear();
        self.target.end_frame();
    }

    pub fn record(&mut self, draw: HdrDraw) {
        if self.armed {
            self.queue.push(draw);
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, settings: BloomSettings, live: (u32, u32)) {
        self.live = (live.0.max(1), live.1.max(1));
        let replaced = self.target.ensure(device, self.live.0, self.live.1);
        let levels = settings.pyramid_levels(self.live.1);

        self.occlusion.resize(
            device,
            self.target.depth_view(),
            self.target.resolved_view(),
            self.target.size(),
            replaced,
        );
        if replaced || self.built_for != Some((self.target.size(), levels)) {
            self.rebuild_pyramid(device, levels);
        }
    }

    pub fn build_occlusion(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        settings: AmbientOcclusionSettings,
        view: AoView,
    ) {
        if !self.target.has_scene() {
            self.occlusion.clear_ready();
            return;
        }
        self.occlusion.build(
            queue,
            encoder,
            settings,
            view,
            self.target.size(),
            self.live,
        );
    }

    pub fn paint_occlusion(&self, pass: &mut wgpu::RenderPass<'static>) {
        self.occlusion.paint(pass);
    }

    fn uv_scale(&self) -> [f32; 2] {
        let (block_width, block_height) = self.target.size();
        [
            self.live.0 as f32 / block_width as f32,
            self.live.1 as f32 / block_height as f32,
        ]
    }

    fn rebuild_pyramid(&mut self, device: &wgpu::Device, levels: u32) {
        let (block_width, block_height) = self.target.size();

        let mut extents = Vec::new();
        for level in 0..levels {
            let width = block_width >> (level + 1);
            let height = block_height >> (level + 1);
            if width < MIN_LEVEL_EXTENT || height < MIN_LEVEL_EXTENT {
                break;
            }
            extents.push((width, height));
        }
        if extents.is_empty() {
            self.pyramid = None;
            self.levels.clear();
            self.overlay_bloom = None;
            self.overlay_scene = None;
            self.built_for = Some(((block_width, block_height), levels));
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vkit.bloom.pyramid"),
            size: wgpu::Extent3d {
                width: extents[0].0,
                height: extents[0].1,
                depth_or_array_layers: 1,
            },
            mip_level_count: extents.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mip_view = |level: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("vkit.bloom.pyramid.level"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };

        let mut levels_out = Vec::with_capacity(extents.len());
        for (index, extent) in extents.iter().copied().enumerate() {
            let source = if index == 0 {
                self.target.resolved_view().clone()
            } else {
                mip_view(index as u32 - 1)
            };
            let down = self.filter_bind_group(device, &source, index as u64);
            let up = (index + 1 < extents.len()).then(|| {
                self.filter_bind_group(
                    device,
                    &mip_view(index as u32 + 1),
                    crate::post_process::MAX_PYRAMID_LEVELS as u64 + index as u64,
                )
            });
            levels_out.push(BloomLevel {
                view: mip_view(index as u32),
                extent,
                down,
                up,
            });
        }

        let overlay_slot = crate::post_process::MAX_PYRAMID_LEVELS as u64 * 2;
        self.overlay_bloom = Some(self.filter_bind_group(device, &mip_view(0), overlay_slot));
        self.overlay_scene = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.bloom.overlay-scene"),
            layout: &self.scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(self.target.resolved_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(self.occlusion.visibility_view()),
                },
            ],
        }));
        self.pyramid = Some(texture);
        self.levels = levels_out;
        self.built_for = Some(((block_width, block_height), levels));
    }

    fn filter_bind_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        slot: u64,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.bloom.filter"),
            layout: &self.filter_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniforms,
                        offset: slot * UNIFORM_STRIDE,
                        size: wgpu::BufferSize::new(size_of::<BloomUniform>() as u64),
                    }),
                },
            ],
        })
    }

    pub fn ready(&self) -> bool {
        !self.levels.is_empty() && self.target.has_scene()
    }

    pub fn build(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        settings: BloomSettings,
        exposure: f32,
        filmic: f32,
    ) {
        if !self.ready() {
            return;
        }
        let curve = settings.threshold_curve();
        let curve = [
            curve.knee_start,
            curve.knee_span,
            curve.knee_scale,
            curve.threshold,
        ];
        let sample_scale = settings.sample_scale(self.live.1);
        let params = [sample_scale, settings.intensity, exposure, filmic];
        let block = self.target.size();
        let uv_scale = self.uv_scale();

        for (index, level) in self.levels.iter().enumerate() {
            let source = if index == 0 {
                (block.0 as f32, block.1 as f32)
            } else {
                let previous = self.levels[index - 1].extent;
                (previous.0 as f32, previous.1 as f32)
            };
            queue.write_buffer(
                &self.uniforms,
                index as u64 * UNIFORM_STRIDE,
                bytemuck::bytes_of(&BloomUniform {
                    texel: [1.0 / source.0, 1.0 / source.1],

                    uv_scale: if index == 0 { uv_scale } else { [1.0, 1.0] },
                    curve,
                    params,
                }),
            );
            let mut pass = level_pass(encoder, &level.view, "vkit.bloom.down");
            pass.set_bind_group(0, &level.down, &[]);
            if index == 0 {
                pass.set_pipeline(&self.prefilter);

                if let Some(scene) = self.overlay_scene.as_ref() {
                    pass.set_bind_group(1, scene, &[]);
                }
            } else {
                pass.set_pipeline(&self.downsample);
            }
            pass.draw(0..3, 0..1);
        }

        for index in (0..self.levels.len().saturating_sub(1)).rev() {
            let Some(up) = self.levels[index].up.as_ref() else {
                continue;
            };
            let source = self.levels[index + 1].extent;
            let slot = crate::post_process::MAX_PYRAMID_LEVELS as u64 + index as u64;
            queue.write_buffer(
                &self.uniforms,
                slot * UNIFORM_STRIDE,
                bytemuck::bytes_of(&BloomUniform {
                    texel: [1.0 / source.0 as f32, 1.0 / source.1 as f32],
                    uv_scale: [1.0, 1.0],
                    curve,
                    params,
                }),
            );
            let mut pass = load_pass(encoder, &self.levels[index].view, "vkit.bloom.up");
            pass.set_pipeline(&self.upsample);
            pass.set_bind_group(0, up, &[]);
            pass.draw(0..3, 0..1);
        }

        self.target.end_frame();
        let overlay_slot = crate::post_process::MAX_PYRAMID_LEVELS as u64 * 2;
        let top = self.levels[0].extent;
        queue.write_buffer(
            &self.uniforms,
            overlay_slot * UNIFORM_STRIDE,
            bytemuck::bytes_of(&BloomUniform {
                texel: [1.0 / top.0 as f32, 1.0 / top.1 as f32],
                uv_scale,
                curve,
                params,
            }),
        );
    }

    pub fn paint(&self, pass: &mut wgpu::RenderPass<'static>) {
        let (Some(bloom), Some(scene)) = (self.overlay_bloom.as_ref(), self.overlay_scene.as_ref())
        else {
            return;
        };
        if !self.ready() {
            return;
        }
        pass.set_pipeline(&self.overlay);
        pass.set_bind_group(0, bloom, &[]);
        pass.set_bind_group(1, scene, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn overlay_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vkit.bloom.overlay"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_overlay"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },

                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::renderer::DEPTH_FORMAT,

            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: crate::renderer::MSAA_SAMPLES,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

fn level_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::RenderPass<'static> {
    begin_pass(
        encoder,
        view,
        label,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    )
}

fn load_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::RenderPass<'static> {
    begin_pass(encoder, view, label, wgpu::LoadOp::Load)
}

fn begin_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &'static str,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'static> {
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime()
}

#[derive(Clone, Copy, Debug)]
pub struct BloomBeginCallback;

impl BloomBeginCallback {
    pub fn paint_callback(self, rect: Rect) -> epaint::PaintCallback {
        Callback::new_paint_callback(rect, self)
    }
}

impl CallbackTrait for BloomBeginCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(bloom) = callback_resources.get_mut::<BloomResources>() {
            bloom.arm();
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        _pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &CallbackResources,
    ) {
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BloomOverlayCallback {
    pub settings: BloomSettings,
    pub occlusion: AmbientOcclusionSettings,

    pub view: AoView,

    pub rect: Rect,
    pub exposure: f32,

    pub filmic: f32,
}

impl BloomOverlayCallback {
    pub fn paint_callback(self, rect: Rect) -> epaint::PaintCallback {
        Callback::new_paint_callback(rect, self)
    }
}

impl CallbackTrait for BloomOverlayCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(mut bloom) = callback_resources.remove::<BloomResources>() else {
            return Vec::new();
        };
        let scale = screen.pixels_per_point.max(0.1);
        let live = (
            (self.rect.width() * scale).round().max(1.0) as u32,
            (self.rect.height() * scale).round().max(1.0) as u32,
        );
        bloom.resize(device, self.settings, live);
        let queued = std::mem::take(&mut bloom.queue);
        bloom.armed = false;

        for draw in queued {
            let mut pass = bloom.target.begin_scene_pass(encoder);
            pass.set_viewport(0.0, 0.0, live.0 as f32, live.1 as f32, 0.0, 1.0);
            match draw {
                HdrDraw::Skin(key) => {
                    if let Some(skin) =
                        callback_resources.get::<crate::renderer::SkinRenderResources>()
                    {
                        skin.paint(&mut pass, key, SceneTarget::Hdr);
                    }
                }
                HdrDraw::Scalp(key) => {
                    if let Some(scalp) =
                        callback_resources.get::<crate::hair_renderer::ScalpRenderResources>()
                    {
                        scalp.paint(&mut pass, key, SceneTarget::Hdr);
                    }
                }
                HdrDraw::Hair(key) => {
                    if let Some(hair) =
                        callback_resources.get::<crate::hair_renderer::HairRenderResources>()
                    {
                        hair.paint(&mut pass, key, SceneTarget::Hdr);
                    }
                }
            }
        }

        bloom.build_occlusion(queue, encoder, self.occlusion, self.view);
        bloom.build(queue, encoder, self.settings, self.exposure, self.filmic);
        callback_resources.insert(bloom);
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(bloom) = callback_resources.get::<BloomResources>() {
            bloom.paint_occlusion(pass);
            bloom.paint(pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shader_declares_every_entry_point_a_pipeline_asks_for() {
        for entry in [
            "fn vs_fullscreen(",
            "fn fs_prefilter(",
            "fn fs_downsample(",
            "fn fs_upsample(",
            "fn fs_overlay(",
        ] {
            assert!(BLOOM_WGSL.contains(entry), "shader lost {entry}");
        }

        assert!(BLOOM_WGSL.contains("fn graded_display("));
    }

    #[test]
    fn the_bloom_pipelines_build_on_an_available_adapter() {
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
                label: Some("vkit.bloom-pipeline-test"),
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
        let mut resources = BloomResources::new(
            &device,
            &queue,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            1280,
            720,
        );

        resources.resize(&device, BloomSettings::default(), (1280, 720));
        let failure = pollster::block_on(scope.pop());
        assert!(failure.is_none(), "bloom failed to build: {failure:?}");
        assert!(
            !resources.levels.is_empty(),
            "a 720p viewport produced no pyramid"
        );
    }

    #[test]
    fn the_uniform_matches_the_layout_the_shader_declares() {
        assert_eq!(size_of::<BloomUniform>(), 48);
        assert!(UNIFORM_STRIDE >= size_of::<BloomUniform>() as u64);
    }
}
