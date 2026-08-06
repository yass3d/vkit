use std::fs::File;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use super::dsf::{MAX_DSF_BYTES, dsf_error, read_limited};
use super::{Result, finite_point_with};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DsfMorphFigure {
    #[default]
    Genesis2Female,
    Genesis2Male,
}

impl DsfMorphFigure {
    pub const fn parent_geometry_url(self) -> &'static str {
        match self {
            Self::Genesis2Female => "/data/DAZ%203D/Genesis%202/Female/Genesis2Female.dsf#geometry",
            Self::Genesis2Male => "/data/DAZ%203D/Genesis%202/Male/Genesis2Male.dsf#geometry",
        }
    }

    const fn figure_directory(self) -> &'static str {
        match self {
            Self::Genesis2Female => "Female",
            Self::Genesis2Male => "Male",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DsfMorphDelta {
    pub vertex_index: u32,
    pub delta_cm: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct DsfMorphAsset {
    pub id: String,

    pub label: String,

    pub group: String,

    pub region: String,
    pub figure: DsfMorphFigure,

    pub vertex_count: usize,
    pub deltas: Vec<DsfMorphDelta>,
}

impl DsfMorphAsset {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(dsf_error("morph asset id must not be blank"));
        }
        if self.vertex_count == 0 {
            return Err(dsf_error("morph asset base vertex count must be positive"));
        }
        for (row, delta) in self.deltas.iter().enumerate() {
            if delta.vertex_index as usize >= self.vertex_count {
                return Err(dsf_error(format!(
                    "morph delta row {row} references vertex {}, but the base has {} vertices",
                    delta.vertex_index, self.vertex_count
                )));
            }
            finite_point_with(delta.delta_cm, || format!("DSF morph delta row {row}"))
                .map_err(|error| dsf_error(error.to_string()))?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct MorphDocument<'a> {
    file_version: &'static str,
    asset_info: AssetInfo,
    modifier_library: [ModifierEntry<'a>; 1],
}

#[derive(Serialize)]
struct AssetInfo {
    id: String,
    #[serde(rename = "type")]
    asset_type: &'static str,
    contributor: Contributor,
    revision: &'static str,
}

#[derive(Serialize)]
struct Contributor {
    author: &'static str,
    email: &'static str,
    website: &'static str,
}

#[derive(Serialize)]
struct ModifierEntry<'a> {
    id: &'a str,
    name: &'a str,
    parent: &'static str,
    presentation: Presentation<'a>,
    channel: Channel<'a>,
    region: &'a str,
    group: &'a str,
    morph: Morph,
}

#[derive(Serialize)]
struct Presentation<'a> {
    #[serde(rename = "type")]
    presentation_type: &'static str,
    label: &'a str,
    description: &'static str,
    icon_large: &'static str,
    colors: [[f64; 3]; 2],
}

#[derive(Serialize)]
struct Channel<'a> {
    id: &'static str,
    #[serde(rename = "type")]
    channel_type: &'static str,
    name: &'static str,
    label: &'a str,
    visible: bool,
    value: f64,
    min: f64,
    max: f64,
    clamped: bool,
    display_as_percent: bool,
    step_size: f64,
}

#[derive(Serialize)]
struct Morph {
    vertex_count: u64,
    deltas: Deltas,
}

#[derive(Serialize)]
struct Deltas {
    count: u64,
    values: Vec<(u32, f64, f64, f64)>,
}

pub fn encode_dsf_morph(asset: &DsfMorphAsset) -> Result<Vec<u8>> {
    asset.validate()?;
    let document = MorphDocument {
        file_version: "0.6.0.0",
        asset_info: AssetInfo {
            id: format!(
                "/data/Vkit/Genesis%202/{}/Morphs/{}.dsf",
                asset.figure.figure_directory(),
                asset.id
            ),
            asset_type: "modifier",
            contributor: Contributor {
                author: "Vkit",
                email: "",
                website: "",
            },
            revision: "1.0",
        },
        modifier_library: [ModifierEntry {
            id: &asset.id,
            name: &asset.id,
            parent: asset.figure.parent_geometry_url(),
            presentation: Presentation {
                presentation_type: "Modifier/Shape",
                label: &asset.label,
                description: "",
                icon_large: "",
                colors: [[0.35, 0.35, 0.35], [0.6, 0.6, 0.6]],
            },
            channel: Channel {
                id: "value",
                channel_type: "float",
                name: "Value",
                label: &asset.label,
                visible: true,
                value: 0.0,
                min: 0.0,
                max: 1.0,
                clamped: false,
                display_as_percent: true,
                step_size: 0.01,
            },
            region: &asset.region,
            group: &asset.group,
            morph: Morph {
                vertex_count: asset.vertex_count as u64,
                deltas: Deltas {
                    count: asset.deltas.len() as u64,
                    values: asset
                        .deltas
                        .iter()
                        .map(|delta| {
                            (
                                delta.vertex_index,
                                delta.delta_cm[0],
                                delta.delta_cm[1],
                                delta.delta_cm[2],
                            )
                        })
                        .collect(),
                },
            },
        }],
    };
    Ok(serde_json::to_vec_pretty(&document)?)
}

#[derive(Deserialize)]
struct MorphDocumentReader {
    #[serde(default)]
    modifier_library: Vec<ModifierEntryReader>,
}

#[derive(Deserialize)]
struct ModifierEntryReader {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    presentation: Option<PresentationReader>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    group: Option<String>,
    morph: Option<MorphReader>,
}

#[derive(Deserialize)]
struct PresentationReader {
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
struct MorphReader {
    vertex_count: Option<u64>,
    deltas: Option<DeltasReader>,
}

#[derive(Deserialize)]
struct DeltasReader {
    count: Option<u64>,
    #[serde(default)]
    values: Vec<(i64, f64, f64, f64)>,
}

pub fn load_dsf_morph_path(path: impl AsRef<Path>) -> Result<DsfMorphAsset> {
    parse_dsf_morph(File::open(path)?)
}

pub fn parse_dsf_morph(reader: impl Read) -> Result<DsfMorphAsset> {
    let encoded = read_limited(reader, MAX_DSF_BYTES, "encoded DSF")?;
    let json = if encoded.starts_with(&[0x1f, 0x8b]) {
        read_limited(
            GzDecoder::new(encoded.as_slice()),
            MAX_DSF_BYTES,
            "expanded DSF",
        )?
    } else {
        encoded
    };
    let document: MorphDocumentReader = serde_json::from_slice(&json)?;
    let entry = document
        .modifier_library
        .into_iter()
        .find(|entry| entry.morph.is_some())
        .ok_or_else(|| dsf_error("document contains no modifier_library morph entry"))?;
    let morph = entry
        .morph
        .ok_or_else(|| dsf_error("modifier entry has no morph block"))?;
    let deltas_block = morph
        .deltas
        .ok_or_else(|| dsf_error("morph block has no deltas"))?;
    let vertex_count = usize::try_from(
        morph
            .vertex_count
            .ok_or_else(|| dsf_error("morph block declares no vertex_count"))?,
    )
    .map_err(|_| dsf_error("morph vertex_count does not fit this platform"))?;
    if let Some(count) = deltas_block.count
        && count != deltas_block.values.len() as u64
    {
        return Err(dsf_error(format!(
            "deltas declare {count} rows but carry {}",
            deltas_block.values.len()
        )));
    }
    let mut deltas = Vec::with_capacity(deltas_block.values.len());
    for (row, (index, x, y, z)) in deltas_block.values.into_iter().enumerate() {
        let vertex_index = u32::try_from(index)
            .map_err(|_| dsf_error(format!("delta row {row} has invalid vertex index {index}")))?;
        deltas.push(DsfMorphDelta {
            vertex_index,
            delta_cm: [x, y, z],
        });
    }
    let id = entry
        .id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| dsf_error("modifier entry has no usable id"))?;
    let label = entry
        .presentation
        .and_then(|presentation| presentation.label)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| id.clone());
    let figure = if entry
        .parent
        .as_deref()
        .is_some_and(|parent| parent.to_ascii_lowercase().contains("genesis2male"))
    {
        DsfMorphFigure::Genesis2Male
    } else {
        DsfMorphFigure::Genesis2Female
    };
    let asset = DsfMorphAsset {
        id,
        label,
        group: entry.group.unwrap_or_else(|| "/Morphs".to_owned()),
        region: entry.region.unwrap_or_else(|| "Actor".to_owned()),
        figure,
        vertex_count,
        deltas,
    };
    asset.validate()?;
    Ok(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_asset() -> DsfMorphAsset {
        DsfMorphAsset {
            id: "Vkit_Test_0123456789ab".to_owned(),
            label: "테스트 모프".to_owned(),
            group: "/Morphs/Vkit".to_owned(),
            region: "Actor".to_owned(),
            figure: DsfMorphFigure::Genesis2Male,
            vertex_count: 21_556,
            deltas: vec![
                DsfMorphDelta {
                    vertex_index: 0,
                    delta_cm: [0.25, -0.5, 1.0],
                },
                DsfMorphDelta {
                    vertex_index: 21_555,
                    delta_cm: [-0.001, 0.002, -0.003],
                },
            ],
        }
    }

    #[test]
    fn morph_round_trips_exactly_through_the_encoder_and_reader() {
        let asset = synthetic_asset();
        let bytes = encode_dsf_morph(&asset).unwrap();
        let decoded = parse_dsf_morph(bytes.as_slice()).unwrap();
        assert_eq!(decoded, asset);
    }

    #[test]
    fn encoder_rejects_out_of_range_and_nonfinite_deltas() {
        let mut out_of_range = synthetic_asset();
        out_of_range.deltas[0].vertex_index = 21_556;
        assert!(encode_dsf_morph(&out_of_range).is_err());

        let mut nonfinite = synthetic_asset();
        nonfinite.deltas[1].delta_cm[2] = f64::NAN;
        assert!(encode_dsf_morph(&nonfinite).is_err());

        let mut blank_id = synthetic_asset();
        blank_id.id = "   ".to_owned();
        assert!(encode_dsf_morph(&blank_id).is_err());
    }

    #[test]
    fn reader_rejects_mismatched_delta_counts() {
        let asset = synthetic_asset();
        let bytes = encode_dsf_morph(&asset).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let corrupted = text.replace("\"count\": 2", "\"count\": 3");
        assert!(parse_dsf_morph(corrupted.as_bytes()).is_err());
    }

    #[test]
    fn reader_accepts_gzip_compressed_documents() {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;

        let asset = synthetic_asset();
        let bytes = encode_dsf_morph(&asset).unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes).unwrap();
        let compressed = encoder.finish().unwrap();
        assert_eq!(parse_dsf_morph(compressed.as_slice()).unwrap(), asset);
    }

    #[test]
    fn parent_geometry_url_selects_the_figure_family() {
        let mut asset = synthetic_asset();
        asset.figure = DsfMorphFigure::Genesis2Female;
        let bytes = encode_dsf_morph(&asset).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("/data/DAZ%203D/Genesis%202/Female/Genesis2Female.dsf#geometry"));
        let decoded = parse_dsf_morph(text.as_bytes()).unwrap();
        assert_eq!(decoded.figure, DsfMorphFigure::Genesis2Female);
    }
}
