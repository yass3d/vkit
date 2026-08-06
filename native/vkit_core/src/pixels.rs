use rayon::prelude::*;

#[derive(Clone, Copy)]
pub struct RgbaView<'a> {
    pub rgba8: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl<'a> RgbaView<'a> {
    pub fn new(rgba8: &'a [u8], width: u32, height: u32) -> Result<Self, String> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "RGBA view dimensions overflow".to_owned())?;
        if width == 0 || height == 0 || rgba8.len() != expected {
            return Err(format!(
                "RGBA view {}x{} does not match {} bytes",
                width,
                height,
                rgba8.len()
            ));
        }
        Ok(Self {
            rgba8,
            width,
            height,
        })
    }
}

fn nearest_axis_map(source: u32, target: u32) -> Vec<usize> {
    (0..target)
        .map(|value| {
            ((u64::from(value) * u64::from(source)) / u64::from(target))
                .min(u64::from(source.saturating_sub(1))) as usize
        })
        .collect()
}

fn box_axis_ranges(source: u32, target: u32) -> Vec<(usize, usize)> {
    (0..target)
        .map(|value| {
            let start = (u64::from(value) * u64::from(source)) / u64::from(target);
            let end = (u64::from(value + 1) * u64::from(source)).div_ceil(u64::from(target));
            let start = start.min(u64::from(source.saturating_sub(1))) as usize;
            let end = (end.max(start as u64 + 1)).min(u64::from(source.max(1))) as usize;
            (start, end)
        })
        .collect()
}

pub fn resize_rgba_box(source: RgbaView<'_>, width: u32, height: u32) -> Vec<u8> {
    let column_ranges = box_axis_ranges(source.width, width);
    let row_ranges = box_axis_ranges(source.height, height);
    let stride = source.width as usize * 4;
    let output_stride = width as usize * 4;
    let mut output = vec![0_u8; output_stride * height as usize];
    output
        .par_chunks_mut(output_stride)
        .zip(row_ranges.par_iter())
        .for_each(|(row, &(row_start, row_end))| {
            for (pixel, &(column_start, column_end)) in row.chunks_exact_mut(4).zip(&column_ranges)
            {
                let mut sums = [0_u64; 4];
                for row in row_start..row_end {
                    let base = row * stride;
                    for column in column_start..column_end {
                        let offset = base + column * 4;
                        for (channel, sum) in sums.iter_mut().enumerate() {
                            *sum += u64::from(source.rgba8[offset + channel]);
                        }
                    }
                }
                let count = ((row_end - row_start) * (column_end - column_start)) as u64;
                pixel.copy_from_slice(&sums.map(|sum| ((sum + count / 2) / count) as u8));
            }
        });
    output
}

pub fn resize_rgba_box_premultiplied(source: RgbaView<'_>, width: u32, height: u32) -> Vec<u8> {
    let mut premultiplied = source.rgba8.to_vec();
    for pixel in premultiplied.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let view = RgbaView::new(&premultiplied, source.width, source.height)
        .expect("the premultiplied copy keeps the source's shape");
    let mut resized = resize_rgba_box(view, width, height);
    for pixel in resized.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    resized
}

#[cfg(test)]
mod premultiplied_resize {
    use super::*;

    #[test]
    fn transparent_neighbours_do_not_darken_the_average() {
        let source = [255_u8, 0, 0, 255, 0, 0, 0, 0];
        let view = RgbaView::new(&source, 2, 1).expect("two texels");
        let resized = resize_rgba_box_premultiplied(view, 1, 1);
        assert_eq!(resized[3], 128, "alpha averages plainly");
        assert!(
            resized[0] >= 253 && resized[1] == 0 && resized[2] == 0,
            "the red survived the transparent neighbour: {resized:?}"
        );
    }
}

pub fn halve_rgba_box(source: RgbaView<'_>) -> (Vec<u8>, u32, u32) {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    (resize_rgba_box(source, width, height), width, height)
}

pub fn resize_rgba_nearest(source: RgbaView<'_>, width: u32, height: u32) -> Vec<u8> {
    let column_map = nearest_axis_map(source.width, width);
    let row_map = nearest_axis_map(source.height, height);
    let mut output = Vec::with_capacity(width as usize * height as usize * 4);
    for &source_row in &row_map {
        let row = &source.rgba8[source_row * source.width as usize * 4..];
        for &source_column in &column_map {
            let offset = source_column * 4;
            output.extend_from_slice(&row[offset..offset + 4]);
        }
    }
    output
}

pub fn flip_rows_in_place(rgba8: &mut [u8], width: u32, height: u32) {
    let stride = width as usize * 4;
    if stride == 0 || height < 2 || rgba8.len() != stride * height as usize {
        return;
    }
    let (mut top, mut bottom) = (0_usize, (height as usize - 1) * stride);
    let mut swap = vec![0_u8; stride];
    while top < bottom {
        swap.copy_from_slice(&rgba8[top..top + stride]);
        rgba8.copy_within(bottom..bottom + stride, top);
        rgba8[bottom..bottom + stride].copy_from_slice(&swap);
        top += stride;
        bottom -= stride;
    }
}

#[inline]
pub fn linear_luminance(pixel: [u8; 4]) -> u8 {
    ((u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19) / 256) as u8
}

pub fn looks_like_dxt5nm(rgba: &[u8]) -> bool {
    const TANGENT_MIDPOINT_LOW: u64 = 80;
    const TANGENT_MIDPOINT_HIGH: u64 = 176;

    let total = rgba.len() / 4;
    if total == 0 {
        return false;
    }
    let stride = (total / 4096).max(1);
    let (mut sampled, mut pinned_red) = (0_u64, 0_u64);
    let (mut alpha_low, mut alpha_high) = (255_u8, 0_u8);
    let (mut green_low, mut green_high) = (255_u8, 0_u8);
    let (mut alpha_sum, mut green_sum) = (0_u64, 0_u64);
    for index in (0..total).step_by(stride) {
        let pixel = &rgba[index * 4..index * 4 + 4];
        sampled += 1;
        if pixel[0] >= 250 {
            pinned_red += 1;
        }
        alpha_low = alpha_low.min(pixel[3]);
        alpha_high = alpha_high.max(pixel[3]);
        green_low = green_low.min(pixel[1]);
        green_high = green_high.max(pixel[1]);
        alpha_sum += u64::from(pixel[3]);
        green_sum += u64::from(pixel[1]);
    }
    let alpha_mean = alpha_sum / sampled;
    let green_mean = green_sum / sampled;
    pinned_red * 100 >= sampled * 99
        && alpha_high.saturating_sub(alpha_low) > 8
        && green_high.saturating_sub(green_low) > 8
        && (TANGENT_MIDPOINT_LOW..=TANGENT_MIDPOINT_HIGH).contains(&alpha_mean)
        && (TANGENT_MIDPOINT_LOW..=TANGENT_MIDPOINT_HIGH).contains(&green_mean)
}

pub fn unswizzle_dxt5nm_in_place(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let x = pixel[3];
        let y = pixel[1];
        let normal_x = f32::from(x) / 127.5 - 1.0;
        let normal_y = f32::from(y) / 127.5 - 1.0;
        let normal_z = (1.0 - normal_x.mul_add(normal_x, normal_y * normal_y))
            .max(0.0)
            .sqrt();
        pixel[0] = x;
        pixel[1] = y;
        pixel[2] = ((normal_z + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
        pixel[3] = 255;
    }
}

fn scale_tangent_axis(encoded: u8, strength: f32) -> u8 {
    if (strength - 1.0).abs() < f32::EPSILON {
        return encoded;
    }
    let centred = f32::from(encoded) - 127.5;
    (centred.mul_add(strength.clamp(0.0, 4.0), 127.5))
        .round()
        .clamp(0.0, 255.0) as u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePackSettings {
    pub default_specular: u8,

    pub default_gloss: u8,

    pub normal_strength: f32,
}

pub fn pack_surface_map(
    normal: Option<RgbaView<'_>>,
    specular: Option<RgbaView<'_>>,
    gloss: Option<RgbaView<'_>>,
    width: u32,
    height: u32,
    settings: SurfacePackSettings,
) -> Vec<u8> {
    let SurfacePackSettings {
        default_specular,
        default_gloss,
        normal_strength,
    } = settings;
    let mut packed = vec![0_u8; width as usize * height as usize * 4];

    type PlannedSource<'a> = (RgbaView<'a>, Vec<(usize, usize)>, Vec<(usize, usize)>);
    fn plan<'a>(view: Option<RgbaView<'a>>, width: u32, height: u32) -> Option<PlannedSource<'a>> {
        view.map(|view| {
            (
                view,
                box_axis_ranges(view.width, width),
                box_axis_ranges(view.height, height),
            )
        })
    }
    let normal = plan(normal, width, height);
    let specular = plan(specular, width, height);
    let gloss = plan(gloss, width, height);
    packed
        .par_chunks_mut(width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                let sample = |source: &Option<PlannedSource<'_>>| {
                    source.as_ref().map(|(view, columns, rows)| {
                        let (row_start, row_end) = rows[y];
                        let (column_start, column_end) = columns[x];
                        let stride = view.width as usize * 4;
                        let mut sums = [0_u64; 4];
                        for source_row in row_start..row_end {
                            let base = source_row * stride;
                            for source_column in column_start..column_end {
                                let offset = base + source_column * 4;
                                for (channel, sum) in sums.iter_mut().enumerate() {
                                    *sum += u64::from(view.rgba8[offset + channel]);
                                }
                            }
                        }
                        let count = ((row_end - row_start) * (column_end - column_start)) as u64;
                        sums.map(|sum| ((sum + count / 2) / count) as u8)
                    })
                };
                let normal_pixel = sample(&normal).unwrap_or([128, 128, 255, 255]);

                pixel[0] = scale_tangent_axis(normal_pixel[0], normal_strength);
                pixel[1] = scale_tangent_axis(normal_pixel[1], normal_strength);
                pixel[2] = sample(&specular).map_or(default_specular, linear_luminance);
                pixel[3] = sample(&gloss).map_or(default_gloss, linear_luminance);
            }
        });
    packed
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasTile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn blit_resized_tile(
    atlas: &mut [u8],
    atlas_width: u32,
    source: RgbaView<'_>,
    tile: AtlasTile,
) {
    let AtlasTile {
        x: tile_x,
        y: tile_y,
        width: tile_width,
        height: tile_height,
    } = tile;
    let column_map = nearest_axis_map(source.width, tile_width);
    let row_map = nearest_axis_map(source.height, tile_height);
    for (y, &source_row) in row_map.iter().enumerate() {
        let row = &source.rgba8[source_row * source.width as usize * 4..];
        let destination_start =
            ((tile_y as usize + y) * atlas_width as usize + tile_x as usize) * 4;
        let destination =
            &mut atlas[destination_start..destination_start + tile_width as usize * 4];
        for (pixel, &source_column) in destination.chunks_exact_mut(4).zip(&column_map) {
            let offset = source_column * 4;
            pixel.copy_from_slice(&row[offset..offset + 4]);
        }
    }
}

pub fn decode_dxt(
    encoded: &[u8],
    width: u32,
    height: u32,
    has_alpha: bool,
) -> Result<Vec<u8>, String> {
    let block_size = if has_alpha { 16 } else { 8 };
    let blocks = u64::from(width.div_ceil(4))
        .checked_mul(u64::from(height.div_ceil(4)))
        .ok_or("DXT block count overflow")?;
    let expected =
        usize::try_from(blocks * block_size as u64).map_err(|_| "DXT byte count exceeds usize")?;
    if encoded.len() != expected {
        return Err(format!(
            "DXT data has {} bytes; expected {expected} for {width}x{height}",
            encoded.len()
        ));
    }
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "DXT pixel count exceeds usize")?;
    let mut rgba = vec![0_u8; pixel_count * 4];
    let mut block_offset = 0;
    for block_y in 0..height.div_ceil(4) {
        for block_x in 0..width.div_ceil(4) {
            let block = &encoded[block_offset..block_offset + block_size];
            block_offset += block_size;
            let (alpha_palette, alpha_bits, color) = if has_alpha {
                (
                    Some(dxt5_alpha_palette(block[0], block[1])),
                    u64::from_le_bytes([
                        block[2], block[3], block[4], block[5], block[6], block[7], 0, 0,
                    ]),
                    &block[8..16],
                )
            } else {
                (None, 0, &block[..8])
            };
            let colors = dxt_color_palette(color, has_alpha);
            let color_bits = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
            for local_y in 0..4_u32 {
                let y = block_y * 4 + local_y;
                if y >= height {
                    continue;
                }
                for local_x in 0..4_u32 {
                    let x = block_x * 4 + local_x;
                    if x >= width {
                        continue;
                    }
                    let source = local_y * 4 + local_x;
                    let color_index = ((color_bits >> (source * 2)) & 3) as usize;
                    let mut pixel = colors[color_index];
                    if let Some(alpha_palette) = alpha_palette {
                        let alpha_index = ((alpha_bits >> (source * 3)) & 7) as usize;
                        pixel[3] = alpha_palette[alpha_index];
                    }
                    let destination = (u64::from(y) * u64::from(width) + u64::from(x)) as usize * 4;
                    rgba[destination..destination + 4].copy_from_slice(&pixel);
                }
            }
        }
    }
    Ok(rgba)
}

fn dxt_color_palette(block: &[u8], force_four_colors: bool) -> [[u8; 4]; 4] {
    let first = u16::from_le_bytes([block[0], block[1]]);
    let second = u16::from_le_bytes([block[2], block[3]]);
    let expand = |value: u16| {
        [
            (((value >> 11) & 31) * 255 / 31) as u8,
            (((value >> 5) & 63) * 255 / 63) as u8,
            ((value & 31) * 255 / 31) as u8,
            255,
        ]
    };
    let first_color = expand(first);
    let second_color = expand(second);
    let mix = |a: [u8; 4], b: [u8; 4], a_weight: u16, b_weight: u16, divisor: u16| {
        std::array::from_fn(|channel| {
            ((u16::from(a[channel]) * a_weight + u16::from(b[channel]) * b_weight) / divisor) as u8
        })
    };
    if force_four_colors || first > second {
        [
            first_color,
            second_color,
            mix(first_color, second_color, 2, 1, 3),
            mix(first_color, second_color, 1, 2, 3),
        ]
    } else {
        [
            first_color,
            second_color,
            mix(first_color, second_color, 1, 1, 2),
            [0, 0, 0, 0],
        ]
    }
}

fn dxt5_alpha_palette(first: u8, second: u8) -> [u8; 8] {
    let mut palette = [0_u8; 8];
    palette[0] = first;
    palette[1] = second;
    if first > second {
        for index in 1..=6 {
            palette[index + 1] = ((u16::from(first) * (7 - index) as u16
                + u16::from(second) * index as u16)
                / 7) as u8;
        }
    } else {
        for index in 1..=4 {
            palette[index + 1] = ((u16::from(first) * (5 - index) as u16
                + u16::from(second) * index as u16)
                / 5) as u8;
        }
        palette[6] = 0;
        palette[7] = 255;
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_resize_and_tile_blit_preserve_row_order() {
        let source_bytes = [10, 0, 0, 255, 0, 20, 0, 255, 0, 0, 30, 255, 40, 40, 40, 255];
        let source = RgbaView::new(&source_bytes, 2, 2).unwrap();
        let up = resize_rgba_nearest(source, 4, 4);
        assert_eq!(&up[..4], &[10, 0, 0, 255]);
        assert_eq!(&up[12..16], &[0, 20, 0, 255]);
        assert_eq!(&up[48..52], &[0, 0, 30, 255]);
        assert_eq!(&up[60..64], &[40, 40, 40, 255]);

        let mut atlas = vec![0_u8; 4 * 4 * 4];
        blit_resized_tile(
            &mut atlas,
            4,
            source,
            AtlasTile {
                x: 2,
                y: 2,
                width: 2,
                height: 2,
            },
        );
        assert_eq!(&atlas[(2 * 4 + 2) * 4..(2 * 4 + 3) * 4], &[10, 0, 0, 255]);
        assert_eq!(&atlas[(3 * 4 + 3) * 4..(3 * 4 + 4) * 4], &[40, 40, 40, 255]);
    }

    #[test]
    fn dxt5nm_normals_are_detected_and_restored() {
        let mut swizzled = Vec::new();
        for step in 0..64_u8 {
            swizzled.extend_from_slice(&[255, 100 + step, step.wrapping_mul(3), 96 + step]);
        }
        assert!(looks_like_dxt5nm(&swizzled));

        unswizzle_dxt5nm_in_place(&mut swizzled);
        let first = &swizzled[0..4];
        assert_eq!(first[0], 96, "tangent X returns to red");
        assert_eq!(first[1], 100, "tangent Y stays in green");
        assert_eq!(first[3], 255, "alpha becomes opaque");

        let x = f32::from(first[0]) / 127.5 - 1.0;
        let y = f32::from(first[1]) / 127.5 - 1.0;
        assert!(x * x + y * y < 1.0);
        assert!(first[2] > 128, "reconstructed Z points outward");
    }

    #[test]
    fn ordinary_colour_and_normal_maps_are_left_alone() {
        let conventional: Vec<u8> = (0..64_u8)
            .flat_map(|step| [120 + step % 16, 130 + step % 16, 250, 255])
            .collect();
        assert!(!looks_like_dxt5nm(&conventional));

        let diffuse: Vec<u8> = (0..64_u8)
            .flat_map(|step| [200 + step % 40, 90, 80, 10 + step])
            .collect();
        assert!(!looks_like_dxt5nm(&diffuse));

        let white = vec![255_u8; 64 * 4];
        assert!(!looks_like_dxt5nm(&white));

        let saturated: Vec<u8> = (0..64_u8)
            .flat_map(|step| [255, 150 + step % 20, 140 + step % 20, 255])
            .collect();
        assert!(!looks_like_dxt5nm(&saturated));
    }

    #[test]
    fn surface_pack_matches_channel_semantics() {
        let normal_bytes = [230, 17, 250, 255].repeat(4);
        let specular_bytes = [80, 80, 80, 255].repeat(4);
        let gloss_bytes = [200, 200, 200, 255].repeat(4);
        let packed = pack_surface_map(
            Some(RgbaView::new(&normal_bytes, 2, 2).unwrap()),
            Some(RgbaView::new(&specular_bytes, 2, 2).unwrap()),
            Some(RgbaView::new(&gloss_bytes, 2, 2).unwrap()),
            2,
            2,
            SurfacePackSettings {
                default_specular: 96,
                default_gloss: 140,
                normal_strength: 1.0,
            },
        );
        assert_eq!(&packed[..4], &[230, 17, 80, 200]);
        let defaults = pack_surface_map(
            None,
            None,
            None,
            1,
            1,
            SurfacePackSettings {
                default_specular: 96,
                default_gloss: 140,
                normal_strength: 1.0,
            },
        );
        assert_eq!(defaults, vec![128, 128, 96, 140]);
    }

    #[test]
    fn normal_strength_halves_the_slope_and_leaves_the_other_channels() {
        let normal_bytes = [230, 17, 250, 255].repeat(4);
        let specular_bytes = [80, 80, 80, 255].repeat(4);
        let gloss_bytes = [200, 200, 200, 255].repeat(4);
        let pack = |strength: f32| {
            pack_surface_map(
                Some(RgbaView::new(&normal_bytes, 2, 2).unwrap()),
                Some(RgbaView::new(&specular_bytes, 2, 2).unwrap()),
                Some(RgbaView::new(&gloss_bytes, 2, 2).unwrap()),
                2,
                2,
                SurfacePackSettings {
                    default_specular: 96,
                    default_gloss: 140,
                    normal_strength: strength,
                },
            )
        };
        let full = pack(1.0);
        let half = pack(0.5);

        assert_eq!(&half[..4], &[179, 72, 80, 200]);
        for axis in 0..2 {
            let full_slope = f32::from(full[axis]) - 127.5;
            let half_slope = f32::from(half[axis]) - 127.5;
            assert!(
                (half_slope * 2.0 - full_slope).abs() <= 1.0,
                "axis {axis}: {half_slope} should be half of {full_slope}"
            );
        }

        let flat = pack_surface_map(
            None,
            None,
            None,
            1,
            1,
            SurfacePackSettings {
                default_specular: 96,
                default_gloss: 140,
                normal_strength: 0.5,
            },
        );
        assert_eq!(&flat[..2], &[128, 128]);
    }

    #[test]
    fn row_flip_is_an_involution() {
        let mut rgba = (0_u8..48).collect::<Vec<_>>();
        let original = rgba.clone();
        flip_rows_in_place(&mut rgba, 2, 6);
        assert_ne!(rgba, original);
        assert_eq!(&rgba[..8], &original[40..48]);
        flip_rows_in_place(&mut rgba, 2, 6);
        assert_eq!(rgba, original);
    }

    #[test]
    fn dxt1_solid_block_decodes_to_its_endpoint_color() {
        let mut block = Vec::new();
        block.extend_from_slice(&0xf800_u16.to_le_bytes());
        block.extend_from_slice(&0x07e0_u16.to_le_bytes());
        block.extend_from_slice(&0_u32.to_le_bytes());
        let rgba = decode_dxt(&block, 4, 4, false).unwrap();
        assert!(rgba.chunks_exact(4).all(|pixel| pixel == [255, 0, 0, 255]));
    }

    #[test]
    fn dxt5_alpha_palette_carries_interpolated_alpha() {
        let mut block = Vec::new();
        block.push(255);
        block.push(0);
        block.extend_from_slice(&[0b0000_1000, 0, 0, 0, 0, 0]);
        block.extend_from_slice(&0xf800_u16.to_le_bytes());
        block.extend_from_slice(&0x07e0_u16.to_le_bytes());
        block.extend_from_slice(&0_u32.to_le_bytes());
        let rgba = decode_dxt(&block, 4, 4, true).unwrap();
        assert_eq!(rgba[3], 255);
        assert_eq!(rgba[7], 0);
    }
}
