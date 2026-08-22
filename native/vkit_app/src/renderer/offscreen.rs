//! A surface of our own to draw the scene on.
//!
//! Everything three-dimensional here has, until now, drawn straight into
//! egui's render pass. That works, and it costs one thing: the sample count is
//! egui's. It is fixed when the `Painter` is built, it is baked into egui's own
//! pipelines, and there is no way to change it while the program runs — so
//! asking for 8x meant asking for a restart.
//!
//! A target of our own moves that decision to us. The scene is drawn into a
//! multisampled colour buffer with its own depth attachment, resolved once, and
//! the resolved texture is what egui is handed. egui's own sample count then
//! stops mattering — its shapes are anti-aliased by its tessellator, not by
//! multisampling — and ours becomes a number that can change between frames.
//!
//! [`crate::hair_portrait`] already worked this way for thumbnails. This is the
//! same machinery, sized to a rectangle rather than a square, and asked for a
//! sample count rather than reading the process-wide one.

use crate::renderer::DEPTH_FORMAT;

/// The colour, depth and resolve attachments the scene is drawn through.
///
/// Rebuilt when the size or the sample count changes, and not otherwise: the
/// textures are the expensive part and a viewport that is not being resized
/// keeps the same ones frame after frame.
pub struct OffscreenTarget {
    multisampled: Option<wgpu::TextureView>,
    resolved: wgpu::Texture,
    resolved_view: wgpu::TextureView,
    depth: wgpu::TextureView,
    width: u32,
    height: u32,
    samples: u32,
    format: wgpu::TextureFormat,
}

impl std::fmt::Debug for OffscreenTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OffscreenTarget")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("samples", &self.samples)
            .finish_non_exhaustive()
    }
}

impl OffscreenTarget {
    /// At one sample there is no multisampled buffer at all: the scene is drawn
    /// straight into the resolve texture. A single-sample colour attachment
    /// with a resolve target of its own is not a pass wgpu will accept, and
    /// allocating a 1x "multisampled" buffer to copy out of would be a second
    /// full-size texture for nothing.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let samples = samples.max(1);
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let multisampled = (samples > 1).then(|| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("vkit.offscreen.multisampled"),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
        let resolved = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vkit.offscreen.resolved"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let resolved_view = resolved.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("vkit.offscreen.depth"),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            multisampled,
            resolved,
            resolved_view,
            depth,
            width,
            height,
            samples,
            format,
        }
    }

    /// Give back a target of this shape, reusing the textures when the shape
    /// has not moved.
    ///
    /// This is where a changed sample count takes effect: the caller passes the
    /// count it wants every frame, and the frame it differs is the frame the
    /// attachments are rebuilt at it.
    #[must_use]
    pub fn reshaped(
        self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Self {
        if self.matches(format, width, height, samples) {
            return self;
        }
        Self::new(device, format, width, height, samples)
    }

    #[must_use]
    pub fn matches(
        &self,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        samples: u32,
    ) -> bool {
        self.format == format
            && self.width == width.max(1)
            && self.height == height.max(1)
            && self.samples == samples.max(1)
    }

    /// The single-sample texture the scene ends up in, for whoever draws it.
    #[must_use]
    pub const fn resolved(&self) -> &wgpu::Texture {
        &self.resolved
    }

    /// Open the pass the whole scene is drawn in — one pass, one depth buffer,
    /// so the layers depth-test against each other the way they do today.
    pub fn begin(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        clear: wgpu::Color,
    ) -> wgpu::RenderPass<'static> {
        let (view, resolve_target) = match self.multisampled.as_ref() {
            Some(multisampled) => (multisampled, Some(&self.resolved_view)),
            None => (&self.resolved_view, None),
        };
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vkit.offscreen.scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        pass.set_viewport(0.0, 0.0, self.width as f32, self.height as f32, 0.0, 1.0);
        pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    fn device() -> Option<(wgpu::Device, wgpu::Queue, wgpu::Adapter)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("vkit.offscreen-test"),
                required_features: adapter
                    .features()
                    .intersection(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES),
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            }),
        )
        .ok()?;
        Some((device, queue, adapter))
    }

    /// Draw nothing but the clear, read the pixels back, and check the whole
    /// rectangle carries the colour that was asked for. It is the smallest
    /// thing that proves the attachments are wired: a resolve target that is
    /// not written, or a multisampled buffer that is not resolved, comes back
    /// black rather than green.
    fn clear_and_read(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Vec<u8> {
        let target = OffscreenTarget::new(device, FORMAT, width, height, samples);
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        drop(target.begin(
            &mut encoder,
            wgpu::Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
        ));

        let row_bytes = width * 4;
        let padded_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target.resolved(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        {
            let mapped = staging.slice(..).get_mapped_range();
            for row in 0..height {
                let start = (row * padded_row) as usize;
                pixels.extend_from_slice(&mapped[start..start + row_bytes as usize]);
            }
        }
        staging.unmap();
        pixels
    }

    #[test]
    fn every_sample_count_the_adapter_carries_resolves_to_the_same_picture() {
        let Some((device, queue, adapter)) = device() else {
            return;
        };
        for samples in crate::renderer::MSAA_CHOICES {
            if crate::renderer::resolve_msaa_samples(&adapter, FORMAT, samples) != samples {
                continue;
            }
            let pixels = clear_and_read(&device, &queue, 37, 21, samples);
            assert_eq!(pixels.len(), 37 * 21 * 4, "{samples}x came back short");
            for (index, texel) in pixels.chunks_exact(4).enumerate() {
                assert!(
                    texel[1] > 200 && texel[0] < 60 && texel[2] < 60 && texel[3] > 200,
                    "{samples}x: texel {index} came back {texel:?} rather than the clear",
                );
            }
        }
    }

    /// One sample has no multisampled buffer to resolve out of — the scene goes
    /// straight into the resolve texture — and more than one does. Both have to
    /// come back as a picture, which is what the test above checks; this pins
    /// the arrangement so the reason stays visible.
    #[test]
    fn a_single_sample_target_allocates_no_second_buffer() {
        let Some((device, _queue, _adapter)) = device() else {
            return;
        };
        let single = OffscreenTarget::new(&device, FORMAT, 8, 8, 1);
        assert!(single.multisampled.is_none());
        assert!(single.matches(FORMAT, 8, 8, 1));

        let many = OffscreenTarget::new(&device, FORMAT, 8, 8, 4);
        assert!(many.multisampled.is_some());
        assert!(many.matches(FORMAT, 8, 8, 4));
    }

    /// The sample count changes between frames, so the target has to notice.
    /// Reusing the textures when nothing moved is the whole reason `reshaped`
    /// exists; rebuilding them when the count moves is the reason the setting
    /// can take effect without a restart.
    #[test]
    fn a_target_is_kept_when_its_shape_holds_and_rebuilt_when_it_does_not() {
        let Some((device, _queue, _adapter)) = device() else {
            return;
        };
        let target = OffscreenTarget::new(&device, FORMAT, 64, 32, 4);
        assert!(target.matches(FORMAT, 64, 32, 4), "it must match itself");

        let kept = target.reshaped(&device, FORMAT, 64, 32, 4);
        assert!(kept.matches(FORMAT, 64, 32, 4), "nothing moved");

        let resized = kept.reshaped(&device, FORMAT, 65, 32, 4);
        assert!(!resized.matches(FORMAT, 64, 32, 4), "the width moved");
        assert!(resized.matches(FORMAT, 65, 32, 4));

        let recounted = resized.reshaped(&device, FORMAT, 65, 32, 1);
        assert!(
            recounted.matches(FORMAT, 65, 32, 1),
            "the count has to take"
        );
        assert!(recounted.multisampled.is_none());
    }

    #[test]
    fn a_zero_sized_rectangle_is_still_a_target() {
        let Some((device, _queue, _adapter)) = device() else {
            return;
        };
        // A pane collapsed to nothing must not ask wgpu for a zero-extent
        // texture, which is a validation error rather than an empty picture.
        let target = OffscreenTarget::new(&device, FORMAT, 0, 0, 4);
        assert!(target.matches(FORMAT, 1, 1, 4));
    }
}
