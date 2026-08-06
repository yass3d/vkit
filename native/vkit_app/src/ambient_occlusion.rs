use crate::renderer::MSAA_SAMPLES;
use crate::shader_color::color_grading_wgsl;

const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

const AO_DIVISOR: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientOcclusionSettings {
    pub enabled: bool,

    pub intensity: f32,

    pub radius: f32,
}

impl Default for AmbientOcclusionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: DEFAULT_INTENSITY,
            radius: DEFAULT_RADIUS,
        }
    }
}

pub const DEFAULT_INTENSITY: f32 = 0.55;
pub const INTENSITY_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;

pub const DEFAULT_RADIUS: f32 = 0.05;
pub const RADIUS_RANGE: std::ops::RangeInclusive<f32> = 0.01..=0.25;

impl AmbientOcclusionSettings {
    pub fn contributes(self) -> bool {
        self.enabled && self.intensity > 0.0 && self.radius > 0.0
    }

    pub fn world_radius(self, frame_radius: f32) -> f32 {
        let frame = if frame_radius.is_finite() && frame_radius > 0.0 {
            frame_radius
        } else {
            1.0
        };
        (frame
            * self
                .radius
                .clamp(*RADIUS_RANGE.start(), *RADIUS_RANGE.end()))
        .max(1.0e-4)
    }
}

pub fn ao_size(block: (u32, u32)) -> (u32, u32) {
    ((block.0 / AO_DIVISOR).max(1), (block.1 / AO_DIVISOR).max(1))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct AoUniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],

    eye: [f32; 4],

    params: [f32; 4],

    sizes: [f32; 4],

    uv_scale: [f32; 2],
    texel: [f32; 2],
}

pub(crate) const AO_WGSL: &str = r#"
struct Uniforms {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    params: vec4<f32>,
    sizes: vec4<f32>,
    uv_scale: vec2<f32>,
    texel: vec2<f32>,
}

@group(0) @binding(0) var depth_texture: texture_depth_multisampled_2d;
@group(0) @binding(1) var<uniform> uniforms: Uniforms;

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

fn depth_texel(uv: vec2<f32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let limit = vec2<i32>(uniforms.sizes.xy) - vec2<i32>(1);
    return clamp(vec2<i32>(clamped * uniforms.sizes.xy), vec2<i32>(0), limit);
}

fn world_at(coord: vec2<i32>) -> vec4<f32> {
    let limit = vec2<i32>(uniforms.sizes.xy) - vec2<i32>(1);
    let clamped = clamp(coord, vec2<i32>(0), limit);
    let depth = textureLoad(depth_texture, clamped, 0);
    if (depth >= 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let uv = (vec2<f32>(clamped) + vec2<f32>(0.5)) / uniforms.sizes.xy;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world = uniforms.inverse_view_projection * ndc;
    return vec4<f32>(world.xyz / world.w, 1.0);
}

fn world_position(uv: vec2<f32>) -> vec4<f32> {
    return world_at(depth_texel(uv));
}

const SAMPLE_COUNT: u32 = 12u;

const NORMAL_STEP: i32 = 2;

fn hemisphere_tap(index: u32, rotation: f32) -> vec3<f32> {
    let fraction = (f32(index) + 0.5) / f32(SAMPLE_COUNT);

    let angle = f32(index) * 2.399963 + rotation;
    let height = sqrt(1.0 - fraction);
    let ring = sqrt(fraction);

    let shuffled = f32((index * 5u) % SAMPLE_COUNT) / f32(SAMPLE_COUNT);
    let length = mix(0.25, 1.0, shuffled);
    return vec3<f32>(cos(angle) * ring, sin(angle) * ring, height) * length;
}

fn tangent_frame(normal: vec3<f32>) -> mat3x3<f32> {
    let up = select(
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(1.0, 0.0, 0.0),
        abs(normal.y) > 0.99,
    );
    let tangent = normalize(cross(up, normal));
    return mat3x3<f32>(tangent, cross(normal, tangent), normal);
}

fn dither(position: vec2<f32>) -> f32 {
    let cell = vec2<u32>(max(position, vec2<f32>(0.0)));
    let x = cell.x & 3u;
    let y = cell.y & 3u;
    let mixed = x ^ y;
    let index = ((mixed & 1u) << 3u) | ((y & 1u) << 2u) | (mixed & 2u) | ((y & 2u) >> 1u);
    return f32(index) / 16.0;
}

@fragment
fn fs_occlusion(in: VertexOut) -> @location(0) vec4<f32> {
    let base = depth_texel(in.uv * uniforms.uv_scale);
    let here = world_at(base);
    if (here.w < 0.5) {
        return vec4<f32>(1.0);
    }

    let right = world_at(base + vec2<i32>(NORMAL_STEP, 0));
    let down = world_at(base + vec2<i32>(0, NORMAL_STEP));
    if (right.w < 0.5 || down.w < 0.5) {
        return vec4<f32>(1.0);
    }
    var normal = cross(right.xyz - here.xyz, down.xyz - here.xyz);
    let length_squared = dot(normal, normal);
    if (length_squared < 1.0e-18) {
        return vec4<f32>(1.0);
    }
    normal = normal * inverseSqrt(length_squared);

    if (dot(normal, uniforms.eye.xyz - here.xyz) < 0.0) {
        normal = -normal;
    }

    let radius = uniforms.params.x;
    let frame = tangent_frame(normal);
    let rotation = dither(in.position.xy) * 6.2831853;
    var occlusion = 0.0;
    for (var index = 0u; index < SAMPLE_COUNT; index = index + 1u) {
        let offset = frame * hemisphere_tap(index, rotation);
        let probe = here.xyz + offset * radius;
        let clip = uniforms.view_projection * vec4<f32>(probe, 1.0);
        if (clip.w <= 0.0) {
            continue;
        }
        let probe_uv = vec2<f32>(
            clip.x / clip.w * 0.5 + 0.5,
            0.5 - clip.y / clip.w * 0.5,
        );
        if (any(probe_uv < vec2<f32>(0.0)) || any(probe_uv > vec2<f32>(1.0))) {
            continue;
        }
        let surface = world_position(probe_uv * uniforms.uv_scale);
        if (surface.w < 0.5) {
            continue;
        }
        let toward = surface.xyz - here.xyz;
        let distance = length(toward);
        if (distance < 1.0e-6 || distance > radius) {
            continue;
        }

        let facing = dot(toward / distance, normal);
        if (facing > 0.15) {
            occlusion = occlusion + (1.0 - distance / radius);
        }
    }
    let visibility = 1.0 - uniforms.params.y * occlusion / f32(SAMPLE_COUNT);
    return vec4<f32>(clamp(visibility, 0.0, 1.0));
}
"#;

pub(crate) const AO_BLUR_WGSL: &str = concat!(
    color_grading_wgsl!(),
    r#"
struct Uniforms {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    params: vec4<f32>,
    sizes: vec4<f32>,
    uv_scale: vec2<f32>,
    texel: vec2<f32>,
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> uniforms: Uniforms;
@group(0) @binding(3) var scene_texture: texture_2d<f32>;

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

@fragment
fn fs_blur(in: VertexOut) -> @location(0) vec4<f32> {
    var total = 0.0;
    for (var y = 0; y < 2; y = y + 1) {
        for (var x = 0; x < 2; x = x + 1) {
            let quadrant = vec2<f32>(f32(x), f32(y)) * 2.0 - vec2<f32>(1.5);
            total = total
                + textureSampleLevel(source_texture, source_sampler, in.uv + quadrant * uniforms.texel, 0.0).r;
        }
    }
    return vec4<f32>(total / 4.0);
}

@fragment
fn fs_apply(in: VertexOut) -> @location(0) vec4<f32> {
    let visibility = textureSampleLevel(source_texture, source_sampler, in.uv, 0.0).r;
    let scene = sanitize_radiance(
        textureSampleLevel(scene_texture, source_sampler, in.uv * uniforms.uv_scale, 0.0).rgb
    );
    let exposure = uniforms.params.z;
    let filmic = uniforms.params.w > 0.5;
    let lit = graded_display(scene, exposure, filmic, true);
    let shadowed = graded_display(scene * visibility, exposure, filmic, true);

    let ratio = select(vec3<f32>(1.0), shadowed / lit, lit > vec3<f32>(1.0e-5));
    return vec4<f32>(clamp(ratio, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#
);

pub struct AmbientOcclusion {
    occlusion_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    apply_pipeline: wgpu::RenderPipeline,
    depth_layout: wgpu::BindGroupLayout,
    image_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    size: (u32, u32),
    raw: Option<wgpu::TextureView>,
    blurred: Option<wgpu::TextureView>,
    depth_bind: Option<wgpu::BindGroup>,
    blur_bind: Option<wgpu::BindGroup>,
    apply_bind: Option<wgpu::BindGroup>,

    ready: bool,

    opaque: wgpu::TextureView,
}

impl AmbientOcclusion {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let occlusion_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.ao.occlusion"),
            source: wgpu::ShaderSource::Wgsl(AO_WGSL.into()),
        });
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.ao.blur"),
            source: wgpu::ShaderSource::Wgsl(AO_BLUR_WGSL.into()),
        });
        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<AoUniform>() as u64),
            },
            count: None,
        };
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.ao.depth-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
                uniform_entry(1),
            ],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.ao.image-layout"),
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
                uniform_entry(2),
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
            ],
        });
        let depth_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vkit.ao.depth-pipeline-layout"),
                bind_group_layouts: &[Some(&depth_layout)],
                immediate_size: 0,
            });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vkit.ao.image-pipeline-layout"),
                bind_group_layouts: &[Some(&image_layout)],
                immediate_size: 0,
            });

        Self {
            occlusion_pipeline: offscreen_pipeline(
                device,
                &occlusion_shader,
                &depth_pipeline_layout,
                "fs_occlusion",
                "vkit.ao.occlusion",
            ),
            blur_pipeline: offscreen_pipeline(
                device,
                &image_shader,
                &image_pipeline_layout,
                "fs_blur",
                "vkit.ao.blur",
            ),
            apply_pipeline: apply_pipeline(
                device,
                &image_shader,
                &image_pipeline_layout,
                surface_format,
            ),
            depth_layout,
            image_layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vkit.ao.sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }),
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vkit.ao.uniforms"),
                size: size_of::<AoUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            size: (0, 0),
            raw: None,
            blurred: None,
            depth_bind: None,
            blur_bind: None,
            apply_bind: None,
            ready: false,
            opaque: white_texture(device, queue),
        }
    }

    pub fn visibility_view(&self) -> &wgpu::TextureView {
        self.blurred.as_ref().unwrap_or(&self.opaque)
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        scene: &wgpu::TextureView,
        block: (u32, u32),
        replaced: bool,
    ) {
        let wanted = ao_size(block);
        if !replaced && self.size == wanted && self.raw.is_some() {
            return;
        }
        let target = |label: &'static str| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: wanted.0,
                        height: wanted.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: AO_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let raw = target("vkit.ao.raw");
        let blurred = target("vkit.ao.blurred");
        self.depth_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vkit.ao.depth"),
            layout: &self.depth_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniforms.as_entire_binding(),
                },
            ],
        }));
        self.blur_bind = Some(self.image_bind_group(device, &raw, scene, "vkit.ao.blur-source"));
        self.apply_bind =
            Some(self.image_bind_group(device, &blurred, scene, "vkit.ao.apply-source"));
        self.raw = Some(raw);
        self.blurred = Some(blurred);
        self.size = wanted;
    }

    fn image_bind_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        scene: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.image_layout,
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
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
            ],
        })
    }

    pub fn clear_ready(&mut self) {
        self.ready = false;
    }

    pub fn build(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        settings: AmbientOcclusionSettings,
        view: AoView,
        block: (u32, u32),
        live: (u32, u32),
    ) {
        self.ready = false;
        let (Some(raw), Some(blurred)) = (self.raw.as_ref(), self.blurred.as_ref()) else {
            return;
        };
        if !settings.contributes() {
            let _ = full_pass(encoder, blurred, "vkit.ao.clear");
            return;
        }
        let (Some(depth_bind), Some(blur_bind)) =
            (self.depth_bind.as_ref(), self.blur_bind.as_ref())
        else {
            return;
        };
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&AoUniform {
                view_projection: view.view_projection,
                inverse_view_projection: view.inverse_view_projection,
                eye: [view.eye[0], view.eye[1], view.eye[2], 0.0],
                params: [
                    settings.world_radius(view.frame_radius),
                    settings.intensity.clamp(0.0, 1.0),
                    view.exposure,
                    view.filmic,
                ],
                sizes: [
                    block.0 as f32,
                    block.1 as f32,
                    self.size.0 as f32,
                    self.size.1 as f32,
                ],
                uv_scale: [
                    live.0 as f32 / block.0.max(1) as f32,
                    live.1 as f32 / block.1.max(1) as f32,
                ],
                texel: [1.0 / self.size.0 as f32, 1.0 / self.size.1 as f32],
            }),
        );
        {
            let mut pass = full_pass(encoder, raw, "vkit.ao.occlusion");
            pass.set_pipeline(&self.occlusion_pipeline);
            pass.set_bind_group(0, depth_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = full_pass(encoder, blurred, "vkit.ao.blur");
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, blur_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.ready = true;
    }

    pub fn paint(&self, pass: &mut wgpu::RenderPass<'static>) {
        let Some(bind) = self.apply_bind.as_ref() else {
            return;
        };
        if !self.ready {
            return;
        }
        pass.set_pipeline(&self.apply_pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AoView {
    pub view_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub eye: [f32; 3],

    pub frame_radius: f32,

    pub exposure: f32,

    pub filmic: f32,
}

fn white_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vkit.ao.opaque"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: AO_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &[u8::MAX],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(1),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn offscreen_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &'static str,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: AO_FORMAT,
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
}

fn apply_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vkit.ao.apply"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_apply"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::Src,
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
            count: MSAA_SAMPLES,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

fn full_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::RenderPass<'static> {
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
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
    fn the_ao_uniform_is_the_same_size_in_all_three_declarations() {
        for (name, source) in [("ao", AO_WGSL), ("ao-blur", AO_BLUR_WGSL)] {
            assert_eq!(
                wgsl_struct_size(name, source, "Uniforms"),
                std::mem::size_of::<AoUniform>(),
                "{name}: AoUniform differs between Rust and WGSL"
            );
        }
    }

    fn dither(x: u32, y: u32) -> f32 {
        let mixed = (x & 3) ^ (y & 3);
        let y = y & 3;
        let index = ((mixed & 1) << 3) | ((y & 1) << 2) | (mixed & 2) | ((y & 2) >> 1);
        f32::from(u8::try_from(index).unwrap()) / 16.0
    }

    #[test]
    fn the_rotation_pattern_repeats_equally_in_both_directions() {
        for y in 0..8_u32 {
            for x in 0..8_u32 {
                assert_eq!(dither(x, y), dither(x + 4, y), "x period at ({x}, {y})");
                assert_eq!(dither(x, y), dither(x, y + 4), "y period at ({x}, {y})");
            }
        }

        assert_ne!(dither(0, 0), dither(1, 0));
        assert_ne!(dither(0, 0), dither(0, 1));
    }

    #[test]
    fn one_tile_holds_every_rotation_exactly_once() {
        let mut seen: Vec<f32> = (0..4)
            .flat_map(|y| (0..4).map(move |x| dither(x, y)))
            .collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        seen.dedup();
        assert_eq!(seen.len(), 16, "{seen:?}");
        assert!(seen.iter().all(|value| (0.0..1.0).contains(value)));
    }

    fn tap_length(index: u32) -> f32 {
        const SAMPLE_COUNT: u32 = 12;
        let shuffled = f32::from(u8::try_from((index * 5) % SAMPLE_COUNT).unwrap())
            / f32::from(u8::try_from(SAMPLE_COUNT).unwrap());
        0.25 + (1.0 - 0.25) * shuffled
    }

    #[test]
    fn the_taps_spread_through_the_hemisphere_instead_of_onto_its_shell() {
        let lengths: Vec<f32> = (0..12).map(tap_length).collect();
        let shortest = lengths.iter().copied().fold(f32::MAX, f32::min);
        let longest = lengths.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            shortest >= 0.2,
            "a tap collapsed onto the origin: {shortest}"
        );
        assert!(longest <= 1.0, "a tap escaped the radius: {longest}");
        assert!(
            longest - shortest > 0.5,
            "the taps are still effectively a shell: {shortest}..{longest}"
        );

        let mut distinct = lengths.clone();
        distinct.sort_by(|a, b| a.partial_cmp(b).unwrap());
        distinct.dedup();
        assert_eq!(distinct.len(), 12, "two taps share a distance: {lengths:?}");

        let descents = lengths.windows(2).filter(|pair| pair[1] < pair[0]).count();
        assert!(
            descents >= 4,
            "distance tracks elevation too closely ({descents} descents in {lengths:?})"
        );
    }

    fn depth_texel(uv: f32, depth_size: u32) -> i32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "mirroring the shader's own vec2<i32> conversion"
        )]
        {
            (uv.clamp(0.0, 1.0) * depth_size as f32) as i32
        }
    }

    #[test]
    fn one_ao_pixel_does_not_span_a_whole_number_of_depth_texels() {
        let live = (1000_u32, 800_u32);
        let block = crate::hdr_target::block_size(live.0, live.1);
        let ao = ao_size(block);
        let scale = f64::from(live.0) / f64::from(block.0);
        let strides: std::collections::BTreeSet<i32> = (0..ao.0 - 1)
            .map(|index| {
                let at = |i: u32| {
                    #[expect(clippy::cast_possible_truncation, reason = "mirrors the shader")]
                    {
                        ((f64::from(i) + 0.5) / f64::from(ao.0) * scale * f64::from(block.0)) as i32
                    }
                };
                at(index + 1) - at(index)
            })
            .collect();
        assert!(
            strides.len() > 1,
            "if this ever became uniform the hazard would be gone, but it is              not: {strides:?}"
        );
    }

    #[test]
    fn a_texel_centre_unprojects_back_to_its_own_texel() {
        for size in [128_u32, 384, 512, 640, 896, 1024, 1664, 1920] {
            for texel in 0..size {
                #[expect(clippy::cast_precision_loss, reason = "mirrors the shader's f32")]
                let uv = (texel as f32 + 0.5) / size as f32;
                assert_eq!(
                    depth_texel(uv, size),
                    i32::try_from(texel).unwrap(),
                    "size {size} texel {texel} came back as a different texel"
                );
            }
        }
    }

    #[test]
    fn the_radius_scales_with_what_the_camera_framed() {
        let settings = AmbientOcclusionSettings::default();
        let small = settings.world_radius(1.0);
        let large = settings.world_radius(100.0);
        assert!(
            (large / small - 100.0).abs() < 1.0e-3,
            "{small} and {large} are not the same fraction"
        );
    }

    #[test]
    fn a_degenerate_frame_still_gives_a_usable_radius() {
        let settings = AmbientOcclusionSettings::default();
        for frame in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let radius = settings.world_radius(frame);
            assert!(radius > 0.0 && radius.is_finite(), "{frame} gave {radius}");
        }
    }

    #[test]
    fn occlusion_that_would_change_nothing_reports_that_it_contributes_nothing() {
        let on = AmbientOcclusionSettings::default();
        assert!(on.contributes());
        for off in [
            AmbientOcclusionSettings {
                enabled: false,
                ..on
            },
            AmbientOcclusionSettings {
                intensity: 0.0,
                ..on
            },
            AmbientOcclusionSettings { radius: 0.0, ..on },
        ] {
            assert!(!off.contributes());
        }
    }

    #[test]
    fn the_occlusion_pipelines_build_on_an_available_adapter() {
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
                label: Some("vkit.ao-pipeline-test"),
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
        let mut occlusion =
            AmbientOcclusion::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);

        let target = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: 1280,
                        height: 768,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let depth = target(
            "vkit.ao-test.depth",
            wgpu::TextureFormat::Depth32Float,
            crate::renderer::MSAA_SAMPLES,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let scene = target(
            "vkit.ao-test.scene",
            crate::hdr_target::HDR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        occlusion.resize(&device, &depth, &scene, (1280, 768), true);
        let failure = pollster::block_on(scope.pop());
        assert!(failure.is_none(), "occlusion failed to build: {failure:?}");
    }

    #[test]
    fn the_occlusion_target_is_never_zero_sized() {
        for block in [(1, 1), (0, 0), (128, 128), (1920, 1080)] {
            let (width, height) = ao_size(block);
            assert!(width >= 1 && height >= 1, "{block:?} gave {width}x{height}");
        }
        assert_eq!(ao_size((1920, 1088)), (960, 544));
    }
}
