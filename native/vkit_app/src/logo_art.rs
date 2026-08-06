use std::sync::OnceLock;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

const RASTER_SIZE: u32 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Logo {
    KoFi,
}

impl Logo {
    const fn source(self) -> &'static str {
        match self {
            Self::KoFi => include_str!("../resources/icons/ko-fi.svg"),
        }
    }

    const fn texture_name(self) -> &'static str {
        match self {
            Self::KoFi => "vkit.logo.ko-fi",
        }
    }
}

pub fn texture(context: &Context, logo: Logo) -> Option<TextureHandle> {
    static KO_FI: OnceLock<Option<TextureHandle>> = OnceLock::new();
    let slot = match logo {
        Logo::KoFi => &KO_FI,
    };
    slot.get_or_init(|| {
        let image = rasterize(logo.source())?;
        Some(context.load_texture(logo.texture_name(), image, TextureOptions::LINEAR))
    })
    .clone()
}

fn rasterize(source: &str) -> Option<ColorImage> {
    let tree = resvg::usvg::Tree::from_str(source, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let span = size.width().max(size.height());
    if span <= 0.0 {
        return None;
    }

    let scale = RASTER_SIZE as f32 / span;
    let offset_x = (RASTER_SIZE as f32 - size.width() * scale) * 0.5;
    let offset_y = (RASTER_SIZE as f32 - size.height() * scale) * 0.5;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(RASTER_SIZE, RASTER_SIZE)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_translate(offset_x, offset_y).pre_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let pixels = pixmap
        .pixels()
        .iter()
        .map(|pixel| {
            egui::Color32::from_rgba_unmultiplied(
                pixel.demultiply().red(),
                pixel.demultiply().green(),
                pixel.demultiply().blue(),
                pixel.alpha(),
            )
        })
        .collect();
    Some(ColorImage {
        size: [RASTER_SIZE as usize, RASTER_SIZE as usize],
        pixels,
        source_size: egui::Vec2::splat(RASTER_SIZE as f32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_logo_rasterises_to_something_visible() {
        assert_rasterises(Logo::KoFi);
    }

    fn assert_rasterises(logo: Logo) {
        let image =
            rasterize(logo.source()).unwrap_or_else(|| panic!("{logo:?} did not rasterise"));
        assert_eq!(image.size, [RASTER_SIZE as usize, RASTER_SIZE as usize]);
        let opaque = image.pixels.iter().filter(|pixel| pixel.a() > 32).count();
        assert!(
            opaque > (RASTER_SIZE * RASTER_SIZE / 20) as usize,
            "{logo:?} rasterised to {opaque} visible pixels, which is a blank"
        );

        let coloured = image
            .pixels
            .iter()
            .filter(|pixel| pixel.a() > 32 && (pixel.r().abs_diff(pixel.b()) > 24))
            .count();
        assert!(coloured > 0, "{logo:?} rasterised without colour");
    }
}
