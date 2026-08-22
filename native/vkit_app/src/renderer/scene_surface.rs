//! The surface every three-dimensional layer draws on, and the blit that puts
//! it back in front of egui.
//!
//! One target for the whole frame, not one per pane. Each layer sets a viewport
//! and a scissor to its own rectangle inside it — which is exactly what egui
//! does for a paint callback today — so panes keep their own corner of one
//! colour buffer and one depth buffer. That shared depth buffer is the reason
//! the layers can depth-test against each other, and the reason this cannot be
//! done a layer at a time: skin, scalp and hair are one scene.
//!
//! The first layer of a frame clears; the rest load. Then each layer's `paint`
//! blits the finished surface over its own rectangle. Every layer blitting is a
//! few extra full-rectangle draws of an opaque texture per frame, which is
//! nothing beside the hair, and it buys a rule with no state in it: whichever
//! layer is painting, what it puts on screen is the whole scene, because egui
//! runs every `prepare` before it runs any `paint`.

use crate::renderer::{DEPTH_FORMAT, OffscreenTarget, msaa_samples};

/// A layer's place in the frame: which rectangle, and which frame.
///
/// Carried on the callback so `prepare` knows it. Only `paint_callback` has the
/// rectangle, and it takes the callback by value, so filling this in there costs
/// the places that build callbacks nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneSpot {
    pub rect: egui::Rect,
    pub frame: u64,
}

impl Default for SceneSpot {
    /// A spot with no pixels: what a callback carries until `paint_callback`
    /// fills it in, and what the thumbnail path leaves it as because it never
    /// goes through egui at all.
    fn default() -> Self {
        Self {
            rect: egui::Rect::NOTHING,
            frame: 0,
        }
    }
}

impl SceneSpot {
    #[must_use]
    pub fn of(ui: &egui::Ui, rect: egui::Rect) -> Self {
        Self {
            rect,
            frame: ui.ctx().cumulative_pass_nr(),
        }
    }
}

/// Where a layer draws inside the frame's surface, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePlacement {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ScenePlacement {
    #[must_use]
    pub fn from_egui(rect: egui::Rect, pixels_per_point: f32) -> Self {
        let scale = |value: f32| (value * pixels_per_point).round().max(0.0) as u32;
        Self {
            x: scale(rect.min.x),
            y: scale(rect.min.y),
            width: scale(rect.width()).max(1),
            height: scale(rect.height()).max(1),
        }
    }

    /// Trimmed to what the surface actually has, so a pane hanging off the edge
    /// of a shrinking window cannot ask for pixels that are not there.
    #[must_use]
    fn clipped(self, width: u32, height: u32) -> Option<Self> {
        if self.x >= width || self.y >= height {
            return None;
        }
        let clipped = Self {
            x: self.x,
            y: self.y,
            width: self.width.min(width - self.x),
            height: self.height.min(height - self.y),
        };
        (clipped.width > 0 && clipped.height > 0).then_some(clipped)
    }
}

const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var scene: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the clip volume. The scissor egui has already set
    // for this callback is what trims it to the layer's rectangle.
    let x = f32((vertex_index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vertex_index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, -y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // The surface is the size of the frame, so the fragment's own pixel is the
    // texel: no sampler, no filtering, no half-pixel to get wrong.
    return textureLoad(scene, vec2<i32>(position.xy), 0);
}
"#;

pub struct SceneSurface {
    target: Option<OffscreenTarget>,
    bind_group: Option<wgpu::BindGroup>,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    cleared_for: Option<u64>,
}

impl std::fmt::Debug for SceneSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SceneSurface")
            .field("target", &self.target)
            .field("cleared_for", &self.cleared_for)
            .finish_non_exhaustive()
    }
}

impl SceneSurface {
    #[must_use]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, egui_samples: u32) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vkit.scene-surface.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vkit.scene-surface.blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vkit.scene-surface.pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vkit.scene-surface.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // The scene is composited over the panel egui has already
                    // painted, exactly as it was when it drew into egui's own
                    // pass. Premultiplied because that is what the layers leave:
                    // each blends over a transparent surface, so what comes out
                    // of the resolve already has its alpha folded in.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            // egui's pass carries a depth attachment, so the blit has to declare
            // one. It neither reads nor writes it: the scene it is carrying has
            // already resolved its own depth.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: egui_samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        Self {
            target: None,
            bind_group: None,
            layout,
            pipeline,
            format,
            cleared_for: None,
        }
    }

    /// Open the pass a layer draws in, with the viewport and scissor set to its
    /// rectangle.
    ///
    /// The first call of a frame clears the surface; the rest load what is
    /// already there. `None` means there is nothing to draw into — a pane with
    /// no pixels, or a rectangle entirely off the surface.
    pub fn begin(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        surface: [u32; 2],
        placement: ScenePlacement,
        frame: u64,
    ) -> Option<wgpu::RenderPass<'static>> {
        let [width, height] = [surface[0].max(1), surface[1].max(1)];
        let samples = msaa_samples();
        let rebuilt = !self
            .target
            .as_ref()
            .is_some_and(|target| target.matches(self.format, width, height, samples));
        if rebuilt {
            let target = OffscreenTarget::new(device, self.format, width, height, samples);
            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vkit.scene-surface.bind-group"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(target.resolved_view()),
                }],
            }));
            self.target = Some(target);
            // A surface that was just made holds nothing, whatever frame it is.
            self.cleared_for = None;
        }
        let placement = placement.clipped(width, height)?;
        let target = self.target.as_ref()?;
        let first_of_the_frame = self.cleared_for != Some(frame);
        self.cleared_for = Some(frame);

        // Empty, not a colour: what the layers do not cover has to stay the
        // panel egui painted underneath.
        let mut pass = target.begin_layer(
            encoder,
            first_of_the_frame.then_some(wgpu::Color::TRANSPARENT),
        );
        pass.set_viewport(
            placement.x as f32,
            placement.y as f32,
            placement.width as f32,
            placement.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(placement.x, placement.y, placement.width, placement.height);
        Some(pass)
    }

    /// Put the finished surface back in front of egui, over whatever rectangle
    /// egui has scissored this callback to.
    pub fn blit(&self, pass: &mut wgpu::RenderPass<'static>) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
