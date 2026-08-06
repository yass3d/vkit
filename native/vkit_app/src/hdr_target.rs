use crate::renderer::{DEPTH_FORMAT, MSAA_SAMPLES};

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub const SIZE_BLOCK: u32 = 128;

pub const MIN_SIZE: u32 = SIZE_BLOCK;

pub const MAX_SIZE: u32 = 8192;

pub fn block_size(width: u32, height: u32) -> (u32, u32) {
    let round = |value: u32| {
        let value = value.clamp(MIN_SIZE, MAX_SIZE);

        let blocks = value.div_ceil(SIZE_BLOCK);
        (blocks * SIZE_BLOCK).min(MAX_SIZE)
    };
    (round(width), round(height))
}

pub fn fits(have: (u32, u32), want: (u32, u32)) -> bool {
    let (have_width, have_height) = have;
    let (want_width, want_height) = want;
    if have_width < want_width || have_height < want_height {
        return false;
    }

    have_width <= want_width.saturating_mul(2) && have_height <= want_height.saturating_mul(2)
}

pub struct SceneHdrTarget {
    size: (u32, u32),

    multisampled: wgpu::TextureView,

    resolved_view: wgpu::TextureView,
    depth: wgpu::TextureView,

    cleared_this_frame: bool,
}

impl SceneHdrTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let (width, height) = block_size(width, height);
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color_descriptor = |label: &'static str, samples: u32, usage: wgpu::TextureUsages| {
            wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage,
                view_formats: &[],
            }
        };
        let multisampled = device
            .create_texture(&color_descriptor(
                "vkit.hdr.multisampled",
                MSAA_SAMPLES,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ))
            .create_view(&wgpu::TextureViewDescriptor::default());
        let resolved_view = device
            .create_texture(&color_descriptor(
                "vkit.hdr.resolved",
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ))
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("vkit.hdr.depth"),
                size: extent,
                mip_level_count: 1,
                sample_count: MSAA_SAMPLES,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,

                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            size: (width, height),
            multisampled,
            resolved_view,
            depth,
            cleared_this_frame: false,
        }
    }

    pub fn ensure(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let want = block_size(width, height);
        if fits(self.size, want) {
            return false;
        }
        *self = Self::new(device, width, height);
        true
    }

    pub const fn size(&self) -> (u32, u32) {
        self.size
    }

    pub const fn resolved_view(&self) -> &wgpu::TextureView {
        &self.resolved_view
    }

    pub const fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth
    }

    pub fn begin_scene_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::RenderPass<'static> {
        let load = if self.cleared_this_frame {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        };
        let depth_load = if self.cleared_this_frame {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(1.0)
        };
        self.cleared_this_frame = true;
        encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vkit.hdr.scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.multisampled,
                    depth_slice: None,
                    resolve_target: Some(&self.resolved_view),
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime()
    }

    pub const fn has_scene(&self) -> bool {
        self.cleared_this_frame
    }

    pub const fn end_frame(&mut self) {
        self.cleared_this_frame = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_round_up_to_a_block_and_never_reach_zero() {
        assert_eq!(block_size(1, 1), (MIN_SIZE, MIN_SIZE));
        assert_eq!(block_size(0, 0), (MIN_SIZE, MIN_SIZE));
        assert_eq!(block_size(SIZE_BLOCK, SIZE_BLOCK), (SIZE_BLOCK, SIZE_BLOCK));
        assert_eq!(
            block_size(SIZE_BLOCK + 1, SIZE_BLOCK + 1),
            (SIZE_BLOCK * 2, SIZE_BLOCK * 2)
        );
        assert_eq!(block_size(1920, 1080), (1920, 1152));

        let (width, height) = block_size(u32::MAX, MAX_SIZE - 1);
        assert_eq!((width, height), (MAX_SIZE, MAX_SIZE));
    }

    #[test]
    fn dragging_an_edge_across_a_block_reallocates_once() {
        let mut allocations = 0;
        let mut have = block_size(800, 600);
        for width in 800..800 + SIZE_BLOCK * 2 {
            let want = block_size(width, 600);
            if !fits(have, want) {
                allocations += 1;
                have = want;
            }
        }
        assert!(
            allocations <= 2,
            "a two-block drag reallocated {allocations} times"
        );
    }

    #[test]
    fn restoring_a_maximised_window_gives_the_memory_back() {
        let large = block_size(3840, 2160);
        let small = block_size(800, 600);
        assert!(!fits(large, small), "kept a 4K target for an 800x600 view");

        assert!(fits(large, block_size(3200, 1800)));
    }

    #[test]
    fn a_viewport_larger_than_its_target_is_never_served_by_it() {
        let have = block_size(1280, 720);
        assert!(!fits(have, block_size(have.0 + 1, have.1)));
        assert!(!fits(have, block_size(have.0, have.1 + 1)));
        assert!(fits(have, block_size(have.0, have.1)));
    }
}
