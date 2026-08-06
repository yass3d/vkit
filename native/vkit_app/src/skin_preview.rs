use std::{io::Cursor, sync::Arc};

use image::ImageReader;

const MAX_PREVIEW_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PREVIEW_EDGE: u32 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkinUvOrientation {
    ObjFlipV,

    VamCacheDirectV,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SkinChannel {
    Face = 0,

    Torso = 1,
    Sclera = 2,
    Iris = 3,
    Pupil = 4,
    Cornea = 5,
    EyeReflection = 6,
    Lacrimal = 7,
    Tear = 8,
    InnerMouth = 9,
    Teeth = 10,
    Gums = 11,
    Tongue = 12,
    Eyelashes = 13,
}

impl SkinChannel {
    pub fn texture_uv(self, uv: [f32; 2], source_orientation: SkinUvOrientation) -> [f32; 2] {
        let direct_v = source_orientation == SkinUvOrientation::VamCacheDirectV
            && matches!(
                self,
                Self::Sclera
                    | Self::Iris
                    | Self::Pupil
                    | Self::Lacrimal
                    | Self::InnerMouth
                    | Self::Teeth
                    | Self::Gums
                    | Self::Tongue
                    | Self::Eyelashes
            );
        [uv[0], if direct_v { uv[1] } else { 1.0 - uv[1] }]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinCorner {
    pub vertex_id: u32,
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinTriangle {
    pub source_triangle_id: u32,
    pub corners: [SkinCorner; 3],
    pub channel: SkinChannel,
}

#[derive(Clone, Debug)]
pub struct SkinPreviewGeometry {
    pub revision: u64,
    pub triangles: Arc<Vec<SkinTriangle>>,
}

impl SkinPreviewGeometry {
    pub fn new(revision: u64, triangles: Vec<SkinTriangle>) -> Result<Self, String> {
        if triangles.is_empty() {
            return Err("skin UV map contains no face or head triangles".to_owned());
        }
        for (triangle_index, triangle) in triangles.iter().enumerate() {
            for (corner_index, corner) in triangle.corners.iter().enumerate() {
                if !corner.uv.iter().all(|value| value.is_finite()) {
                    return Err(format!(
                        "skin UV triangle {triangle_index} corner {corner_index} is non-finite"
                    ));
                }
            }
        }
        Ok(Self {
            revision,
            triangles: Arc::new(triangles),
        })
    }
}

#[derive(Clone, Debug)]
pub struct SkinImage {
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba8: Arc<Vec<u8>>,
    pub uv_orientation: SkinUvOrientation,
}

impl SkinImage {
    pub fn solid(revision: u64, rgba: [u8; 4]) -> Self {
        Self {
            revision,
            width: 1,
            height: 1,
            rgba8: Arc::new(rgba.to_vec()),
            uv_orientation: SkinUvOrientation::ObjFlipV,
        }
    }

    pub fn new(revision: u64, width: u32, height: u32, rgba8: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("skin preview image dimensions must be non-zero".to_owned());
        }
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "skin preview image dimensions overflow".to_owned())?;
        if rgba8.len() != expected {
            return Err(format!(
                "skin preview image has {} RGBA bytes; expected {expected}",
                rgba8.len()
            ));
        }
        Ok(Self {
            revision,
            width,
            height,
            rgba8: Arc::new(rgba8),
            uv_orientation: SkinUvOrientation::ObjFlipV,
        })
    }

    pub fn decode_bounded(revision: u64, encoded: &[u8], max_edge: u32) -> Result<Self, String> {
        if encoded.is_empty() || encoded.len() > MAX_PREVIEW_SOURCE_BYTES {
            return Err(format!(
                "skin image byte count {} is outside 1..={MAX_PREVIEW_SOURCE_BYTES}",
                encoded.len()
            ));
        }
        if max_edge == 0 || max_edge > MAX_PREVIEW_EDGE {
            return Err(format!(
                "skin preview edge budget {max_edge} is outside 1..={MAX_PREVIEW_EDGE}"
            ));
        }
        let reader = ImageReader::new(Cursor::new(encoded))
            .with_guessed_format()
            .map_err(|error| format!("skin image format detection failed: {error}"))?;
        let mut image = reader
            .decode()
            .map_err(|error| format!("skin image decode failed: {error}"))?;
        if image.width() > max_edge || image.height() > max_edge {
            image = image.thumbnail(max_edge, max_edge);
        }
        let rgba = image.into_rgba8();
        Self::new(revision, rgba.width(), rgba.height(), rgba.into_raw())
    }

    pub fn decode_vam_cache_bounded(
        revision: u64,
        meta_path: &std::path::Path,
        max_edge: u32,
    ) -> Result<Self, String> {
        if max_edge == 0 || max_edge > MAX_PREVIEW_EDGE {
            return Err(format!(
                "skin preview edge budget {max_edge} is outside 1..={MAX_PREVIEW_EDGE}"
            ));
        }
        let meta_bytes = std::fs::read(meta_path)
            .map_err(|error| format!("read VaM cache metadata failed: {error}"))?;
        if meta_bytes.len() > 64 * 1024 {
            return Err("VaM cache metadata exceeds 64 KiB".to_owned());
        }
        let meta: serde_json::Value = serde_json::from_slice(&meta_bytes)
            .map_err(|error| format!("parse VaM cache metadata failed: {error}"))?;
        let number = |key: &str| {
            meta.get(key).and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str()?.parse::<u64>().ok())
            })
        };
        let mut width = u32::try_from(number("width").ok_or("VaM cache width is missing")?)
            .map_err(|_| "VaM cache width exceeds u32")?;
        let mut height = u32::try_from(number("height").ok_or("VaM cache height is missing")?)
            .map_err(|_| "VaM cache height exceeds u32")?;
        if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
            return Err(format!("VaM cache dimensions {width}x{height} are invalid"));
        }
        let format = meta
            .get("format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase();
        if !matches!(format.as_str(), "DXT1" | "DXT5" | "RGB24" | "RGBA32") {
            return Err(format!("unsupported VaM cache format {format:?}"));
        }
        let data_path = meta_path.with_extension("vamcache");
        let mut offset = 0_u64;
        while width > max_edge || height > max_edge {
            offset = offset
                .checked_add(cache_level_size(width, height, &format)?)
                .ok_or("VaM cache mip offset overflow")?;
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
        let level_size = cache_level_size(width, height, &format)?;
        if level_size == 0 || level_size > MAX_PREVIEW_SOURCE_BYTES as u64 {
            return Err(format!("VaM cache mip contains {level_size} bytes"));
        }
        use std::io::{Read as _, Seek as _, SeekFrom};
        let mut file = std::fs::File::open(&data_path)
            .map_err(|error| format!("open VaM cache texture failed: {error}"))?;
        let total = file
            .metadata()
            .map_err(|error| format!("inspect VaM cache texture failed: {error}"))?
            .len();
        if offset.checked_add(level_size).is_none_or(|end| end > total) {
            return Err(format!(
                "VaM cache mip range {offset}..{} exceeds {total} bytes",
                offset.saturating_add(level_size)
            ));
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| format!("seek VaM cache texture failed: {error}"))?;
        let mut encoded = vec![0_u8; level_size as usize];
        file.read_exact(&mut encoded)
            .map_err(|error| format!("read VaM cache texture failed: {error}"))?;
        let rgba8 = match format.as_str() {
            "DXT1" => decode_dxt(&encoded, width, height, false)?,
            "DXT5" => decode_dxt(&encoded, width, height, true)?,
            "RGB24" => decode_raw(&encoded, width, height, 3)?,
            "RGBA32" => decode_raw(&encoded, width, height, 4)?,
            other => return Err(format!("unsupported VaM cache texture format {other}")),
        };
        let mut image = Self::new(revision, width, height, rgba8)?;
        image.uv_orientation = SkinUvOrientation::VamCacheDirectV;
        Ok(image)
    }
}

fn cache_level_size(width: u32, height: u32, format: &str) -> Result<u64, String> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or("VaM cache level dimensions overflow")?;
    match format {
        "DXT1" | "DXT5" => {
            let blocks = u64::from(width.div_ceil(4))
                .checked_mul(u64::from(height.div_ceil(4)))
                .ok_or("VaM cache block count overflow")?;
            blocks
                .checked_mul(if format == "DXT5" { 16 } else { 8 })
                .ok_or_else(|| "VaM cache block byte count overflow".to_owned())
        }
        "RGB24" => pixels
            .checked_mul(3)
            .ok_or_else(|| "VaM cache RGB byte count overflow".to_owned()),
        "RGBA32" => pixels
            .checked_mul(4)
            .ok_or_else(|| "VaM cache RGBA byte count overflow".to_owned()),
        _ => Err(format!("unsupported VaM cache format {format:?}")),
    }
}

fn decode_raw(encoded: &[u8], width: u32, height: u32, channels: usize) -> Result<Vec<u8>, String> {
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "VaM raw cache pixel count exceeds usize")?;
    let expected = pixel_count
        .checked_mul(channels)
        .ok_or("VaM raw cache byte count overflow")?;
    if encoded.len() != expected {
        return Err(format!(
            "VaM raw cache has {} bytes; expected {expected}",
            encoded.len()
        ));
    }
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    for pixel in encoded.chunks_exact(channels) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2]]);
        rgba.push(if channels == 4 { pixel[3] } else { 255 });
    }
    Ok(rgba)
}

fn decode_dxt(encoded: &[u8], width: u32, height: u32, has_alpha: bool) -> Result<Vec<u8>, String> {
    vkit_core::pixels::decode_dxt(encoded, width, height, has_alpha)
}

#[derive(Clone, Debug)]
pub struct SkinPreview {
    pub revision: u64,
    pub geometry: Arc<SkinPreviewGeometry>,
    pub face: Arc<SkinImage>,
    pub torso: Arc<SkinImage>,
    pub sclera: Arc<SkinImage>,
    pub iris: Arc<SkinImage>,
    pub lacrimal: Arc<SkinImage>,
    pub inner_mouth: Arc<SkinImage>,
    pub teeth: Arc<SkinImage>,
    pub gums: Arc<SkinImage>,
    pub tongue: Arc<SkinImage>,
    pub eyelashes: Arc<SkinImage>,

    pub face_surface: SkinSurfaceMap,
    pub torso_surface: SkinSurfaceMap,

    pub mouth_surface_atlas: SkinSurfaceMap,
    pub sclera_surface: SkinSurfaceMap,
    pub iris_surface: SkinSurfaceMap,
    pub lacrimal_surface: SkinSurfaceMap,

    pub auxiliary_colors: [[u8; 4]; 8],

    pub auxiliary_textured: [bool; 8],
}

impl SkinPreview {
    pub fn uv_orientation(&self, channel: SkinChannel) -> SkinUvOrientation {
        match channel {
            SkinChannel::Sclera | SkinChannel::Pupil => self.sclera.uv_orientation,
            SkinChannel::Iris => self.iris.uv_orientation,
            SkinChannel::Lacrimal => self.lacrimal.uv_orientation,
            SkinChannel::InnerMouth => self.inner_mouth.uv_orientation,
            SkinChannel::Teeth => self.teeth.uv_orientation,
            SkinChannel::Gums => self.gums.uv_orientation,
            SkinChannel::Tongue => self.tongue.uv_orientation,
            SkinChannel::Eyelashes => self.eyelashes.uv_orientation,
            SkinChannel::Face
            | SkinChannel::Torso
            | SkinChannel::Cornea
            | SkinChannel::EyeReflection
            | SkinChannel::Tear => SkinUvOrientation::ObjFlipV,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkinSurfaceMap {
    pub packed: Arc<SkinImage>,
}

impl SkinSurfaceMap {
    #[cfg(test)]
    pub fn neutral(revision: u64) -> Self {
        Self {
            packed: Arc::new(SkinImage::solid(revision, [128, 128, 96, 140])),
        }
    }

    pub fn matte(revision: u64) -> Self {
        Self {
            packed: Arc::new(SkinImage::solid(revision, [128, 128, 12, 8])),
        }
    }

    pub fn neutral_mouth_atlas(revision: u64) -> Self {
        Self {
            packed: Arc::new(
                SkinImage::new(revision, 2, 2, [128, 128, 96, 140].repeat(4))
                    .expect("2x2 neutral mouth atlas is valid"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn image_contract_rejects_truncated_rgba() {
        assert!(SkinImage::new(1, 2, 2, vec![0; 15]).is_err());
        assert!(SkinImage::new(1, 2, 2, vec![0; 16]).is_ok());
    }

    #[test]
    fn preview_decoder_accepts_png_and_downscales_large_inputs() {
        let source = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let decoded = SkinImage::decode_bounded(7, &bytes, MAX_PREVIEW_EDGE).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.rgba8.len(), 16);
        assert_eq!(decoded.uv_orientation, SkinUvOrientation::ObjFlipV);
    }

    #[test]
    fn bounded_preview_decoder_honors_channel_edge_budget() {
        let source = image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let decoded = SkinImage::decode_bounded(8, &bytes, 2).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.rgba8.len(), 8);
    }

    #[test]
    fn bounded_preview_decoder_rejects_invalid_edge_budget() {
        assert!(SkinImage::decode_bounded(8, &[1], 0).is_err());
        assert!(SkinImage::decode_bounded(8, &[1], MAX_PREVIEW_EDGE.saturating_add(1)).is_err());
    }

    #[test]
    fn uv_contract_rejects_non_finite_values() {
        let triangle = SkinTriangle {
            source_triangle_id: 0,
            channel: SkinChannel::Face,
            corners: [
                SkinCorner {
                    vertex_id: 0,
                    uv: [0.0, 0.0],
                },
                SkinCorner {
                    vertex_id: 1,
                    uv: [f32::NAN, 0.0],
                },
                SkinCorner {
                    vertex_id: 2,
                    uv: [1.0, 1.0],
                },
            ],
        };
        assert!(SkinPreviewGeometry::new(1, vec![triangle]).is_err());
    }

    #[test]
    fn eye_and_lash_orientation_follows_the_decoded_texture_source() {
        let standard = SkinImage::new(3, 1, 2, vec![10, 10, 10, 255, 240, 240, 240, 255]).unwrap();
        let mut vam_cache = standard.clone();
        vam_cache.uv_orientation = SkinUvOrientation::VamCacheDirectV;
        let sample = |image: &SkinImage, channel: SkinChannel| {
            let uv = channel.texture_uv([0.0, 0.20], image.uv_orientation);
            let y = (uv[1] * image.height as f32)
                .floor()
                .clamp(0.0, image.height.saturating_sub(1) as f32) as usize;
            image.rgba8[y * 4]
        };
        for channel in [
            SkinChannel::Sclera,
            SkinChannel::Iris,
            SkinChannel::Pupil,
            SkinChannel::Lacrimal,
            SkinChannel::Eyelashes,
        ] {
            assert_eq!(sample(&standard, channel), 240, "standard {channel:?}");
            assert_eq!(sample(&vam_cache, channel), 10, "VaM cache {channel:?}");
        }
        assert_eq!(sample(&vam_cache, SkinChannel::Face), 240);
        assert_eq!(sample(&vam_cache, SkinChannel::InnerMouth), 10);
    }

    #[test]
    fn pupil_orientation_follows_the_full_eye_atlas_not_the_iris_selector() {
        let mut full_eye = SkinImage::solid(10, [255; 4]);
        full_eye.uv_orientation = SkinUvOrientation::VamCacheDirectV;
        let full_eye = Arc::new(full_eye);
        let iris = Arc::new(SkinImage::solid(11, [255; 4]));
        let shared = Arc::new(SkinImage::solid(12, [255; 4]));
        let mut mouth = SkinImage::solid(13, [255; 4]);
        mouth.uv_orientation = SkinUvOrientation::VamCacheDirectV;
        let mouth = Arc::new(mouth);
        let geometry = Arc::new(
            SkinPreviewGeometry::new(
                10,
                vec![SkinTriangle {
                    source_triangle_id: 0,
                    channel: SkinChannel::Face,
                    corners: [
                        SkinCorner {
                            vertex_id: 0,
                            uv: [0.0, 0.0],
                        },
                        SkinCorner {
                            vertex_id: 1,
                            uv: [1.0, 0.0],
                        },
                        SkinCorner {
                            vertex_id: 2,
                            uv: [0.0, 1.0],
                        },
                    ],
                }],
            )
            .unwrap(),
        );
        let preview = SkinPreview {
            revision: 10,
            geometry,
            face: Arc::clone(&shared),
            torso: Arc::clone(&shared),
            sclera: Arc::clone(&full_eye),
            iris: Arc::clone(&iris),
            lacrimal: Arc::clone(&full_eye),
            inner_mouth: Arc::clone(&mouth),
            teeth: Arc::clone(&mouth),
            gums: Arc::clone(&mouth),
            tongue: Arc::clone(&mouth),
            eyelashes: Arc::clone(&shared),
            face_surface: SkinSurfaceMap::neutral(20),
            torso_surface: SkinSurfaceMap::neutral(21),
            mouth_surface_atlas: SkinSurfaceMap::neutral_mouth_atlas(22),
            sclera_surface: SkinSurfaceMap::neutral(23),
            iris_surface: SkinSurfaceMap::neutral(24),
            lacrimal_surface: SkinSurfaceMap::neutral(25),
            auxiliary_colors: [[255; 4]; 8],
            auxiliary_textured: [true; 8],
        };

        assert_eq!(
            preview.uv_orientation(SkinChannel::Pupil),
            SkinUvOrientation::VamCacheDirectV
        );
        assert_eq!(
            preview.uv_orientation(SkinChannel::Iris),
            SkinUvOrientation::ObjFlipV
        );
        for channel in [
            SkinChannel::InnerMouth,
            SkinChannel::Teeth,
            SkinChannel::Gums,
            SkinChannel::Tongue,
        ] {
            assert_eq!(
                preview.uv_orientation(channel),
                SkinUvOrientation::VamCacheDirectV
            );
        }
    }

    #[test]
    fn decodes_bounded_vam_dxt1_cache_sidecar() {
        let directory =
            std::env::temp_dir().join(format!("vkit-vam-cache-dxt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let meta = directory.join("eye.vamcachemeta");
        fs::write(
            &meta,
            br#"{"type":"image","width":"4","height":"4","format":"DXT1"}"#,
        )
        .unwrap();
        let mut block = Vec::new();
        block.extend_from_slice(&0xf800_u16.to_le_bytes());
        block.extend_from_slice(&0x07e0_u16.to_le_bytes());
        block.extend_from_slice(&0_u32.to_le_bytes());
        fs::write(directory.join("eye.vamcache"), block).unwrap();
        let decoded = SkinImage::decode_vam_cache_bounded(9, &meta, MAX_PREVIEW_EDGE).unwrap();
        assert_eq!((decoded.width, decoded.height), (4, 4));
        assert_eq!(decoded.uv_orientation, SkinUvOrientation::VamCacheDirectV);
        assert_eq!(&decoded.rgba8[..4], &[255, 0, 0, 255]);
        assert!(
            decoded
                .rgba8
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
