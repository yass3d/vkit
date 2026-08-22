use crate::renderer::{OffscreenTarget, msaa_samples};

#[must_use]
pub(crate) const fn portrait_scene_key(seed: u64) -> u64 {
    0x0050_4f52_5452_4149 ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

pub(crate) const PORTRAIT_CLOSENESS: f32 = 0.72;

pub(crate) const PORTRAIT_SIDE: u32 = crate::thumbnail::THUMBNAIL_SIDE;

/// A square of [`OffscreenTarget`], which is what the viewport draws on too.
///
/// The thumbnail path has always rendered this way — its own multisampled
/// buffer, its own depth, resolved once — and that is exactly the shape the
/// viewport needs to stop borrowing egui's sample count.
pub(crate) struct PortraitTarget {
    target: OffscreenTarget,
    side: u32,
}

impl PortraitTarget {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, side: u32) -> Self {
        Self {
            target: OffscreenTarget::new(device, format, side, side, msaa_samples()),
            side,
        }
    }

    /// Reuse the canvas when it still has the shape wanted, which is every run
    /// but the first and the one after the sample count moves.
    pub(crate) fn reshaped(
        self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        side: u32,
    ) -> Self {
        Self {
            target: self
                .target
                .reshaped(device, format, side, side, msaa_samples()),
            side,
        }
    }

    fn resolved(&self) -> &wgpu::Texture {
        self.target.resolved()
    }

    fn begin(&self, encoder: &mut wgpu::CommandEncoder) -> wgpu::RenderPass<'static> {
        self.target.begin(encoder, viewport_ground())
    }
}

pub(crate) struct PortraitScene {
    pub(crate) skin: Option<crate::renderer::SkinPaintCallback>,
    pub(crate) scalps: Vec<crate::hair_renderer::ScalpPaintCallback>,
    pub(crate) hair: Vec<crate::hair_renderer::HairPaintCallback>,
}

fn viewport_ground() -> wgpu::Color {
    let ground = crate::theme::COLOR_MUTED;
    let channel = |value: u8| {
        let linear = f64::from(value) / 255.0;
        if linear <= 0.04045 {
            linear / 12.92
        } else {
            ((linear + 0.055) / 1.055).powf(2.4)
        }
    };
    wgpu::Color {
        r: channel(ground.r()),
        g: channel(ground.g()),
        b: channel(ground.b()),
        a: 1.0,
    }
}

pub(crate) fn render_portrait(
    render_state: &egui_wgpu::RenderState,
    target: &PortraitTarget,
    scene: &PortraitScene,
) -> Result<image::RgbaImage, String> {
    let device = &render_state.device;
    let queue = &render_state.queue;
    let side = target.side;

    let row_bytes = side * 4;
    let padded_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vkit.portrait.readback"),
        size: u64::from(padded_row) * u64::from(side),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("vkit.portrait"),
    });

    {
        let mut renderer = render_state.renderer.write();
        let resources = &mut renderer.callback_resources;
        if let Some(skin) = scene.skin.as_ref()
            && let Some(resources) = resources.get_mut::<crate::renderer::SkinRenderResources>()
        {
            resources.prepare_scene(device, queue, skin);
        }
        if let Some(scalp) = resources.get_mut::<crate::hair_renderer::ScalpRenderResources>() {
            for callback in &scene.scalps {
                scalp.prepare(device, queue, callback);
            }
        }
        if let Some(resources) = resources.get_mut::<crate::hair_renderer::HairRenderResources>() {
            for callback in &scene.hair {
                resources.prepare(device, queue, &mut encoder, callback);
            }
        }
    }

    {
        let renderer = render_state.renderer.read();
        let resources = &renderer.callback_resources;
        let mut pass = target.begin(&mut encoder);
        if let Some(skin) = scene.skin.as_ref()
            && let Some(painter) = resources.get::<crate::renderer::SkinRenderResources>()
        {
            painter.paint(&mut pass, skin.scene_key);
        }
        if let Some(scalp) = resources.get::<crate::hair_renderer::ScalpRenderResources>() {
            for callback in &scene.scalps {
                scalp.paint(&mut pass, callback.scene_key);
            }
        }
        if let Some(painter) = resources.get::<crate::hair_renderer::HairRenderResources>() {
            for callback in &scene.hair {
                painter.paint(&mut pass, callback.scene_key);
            }
        }
    }

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
                rows_per_image: Some(side),
            },
        },
        wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("the portrait never came back: {error}"))?;

    let swap_red_and_blue = matches!(
        render_state.target_format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    {
        let mapped = staging.slice(..).get_mapped_range();
        for row in 0..side {
            let start = (row * padded_row) as usize;
            let line = &mapped[start..start + row_bytes as usize];
            for texel in line.chunks_exact(4) {
                if swap_red_and_blue {
                    pixels.extend_from_slice(&[texel[2], texel[1], texel[0], texel[3]]);
                } else {
                    pixels.extend_from_slice(texel);
                }
            }
        }
    }
    staging.unmap();

    image::RgbaImage::from_raw(side, side, pixels)
        .ok_or_else(|| "the portrait did not fill its square".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portrait_stands_on_a_ground_that_shows_dark_hair() {
        let ground = viewport_ground();
        assert_eq!(ground.a, 1.0, "a transparent clear becomes black in a JPEG");
        for channel in [ground.r, ground.g, ground.b] {
            assert!(
                channel > 0.15,
                "black hair on a dark ground is the shape of nothing"
            );
            assert!(
                channel < 0.7,
                "the ground must not compete with the subject"
            );
        }
    }

    #[test]
    fn the_readback_row_is_aligned_for_the_side_we_ask_for() {
        let row = PORTRAIT_SIDE * 4;
        assert_eq!(
            row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
            row,
            "an unaligned row makes the copy silently skew the image"
        );
    }
}
