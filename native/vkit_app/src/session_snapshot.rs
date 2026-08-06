use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::state::FigureSex;

pub const SNAPSHOT_VERSION: u32 = 1;

const SCULPT_EPSILON: f64 = 1.0e-9;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionSnapshot {
    pub version: u32,
    #[serde(default)]
    pub figure_sex: FigureSex,

    #[serde(default)]
    pub look_id: Option<String>,

    #[serde(default)]
    pub morph_values: BTreeMap<String, f32>,
    #[serde(default)]
    pub eye_closure: f32,
    #[serde(default)]
    pub sculpt: SparseDisplacement,
    #[serde(default)]
    pub texture_layers: Vec<TextureLayerRecord>,
}

impl SessionSnapshot {
    pub fn has_work(&self) -> bool {
        !self.morph_values.is_empty()
            || !self.sculpt.is_empty()
            || self
                .texture_layers
                .iter()
                .any(|layer| layer.source_path.is_some())
            || self.eye_closure != 0.0
    }

    pub const fn is_readable(&self) -> bool {
        self.version == SNAPSHOT_VERSION
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SparseDisplacement {
    #[serde(default)]
    pub vertex_count: u32,
    #[serde(default)]
    pub indices: Vec<u32>,

    #[serde(default)]
    pub offsets: Vec<[f32; 3]>,
}

impl SparseDisplacement {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn from_dense(dense: &[[f64; 3]]) -> Self {
        let mut indices = Vec::new();
        let mut offsets = Vec::new();
        for (index, offset) in dense.iter().enumerate() {
            if offset.iter().any(|axis| axis.abs() > SCULPT_EPSILON) {
                indices.push(index as u32);
                offsets.push([offset[0] as f32, offset[1] as f32, offset[2] as f32]);
            }
        }
        Self {
            vertex_count: dense.len() as u32,
            indices,
            offsets,
        }
    }

    pub fn to_dense(&self, expected_vertex_count: usize) -> Option<Vec<[f64; 3]>> {
        if self.vertex_count as usize != expected_vertex_count
            || self.indices.len() != self.offsets.len()
        {
            return None;
        }
        let mut dense = vec![[0.0_f64; 3]; expected_vertex_count];
        for (index, offset) in self.indices.iter().zip(&self.offsets) {
            let slot = dense.get_mut(*index as usize)?;
            *slot = [
                f64::from(offset[0]),
                f64::from(offset[1]),
                f64::from(offset[2]),
            ];
        }
        Some(dense)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TextureLayerRecord {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub source_path: Option<PathBuf>,
    #[serde(default)]
    pub source_mode: u8,
    #[serde(default)]
    pub channel: u8,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: u8,
    #[serde(default)]
    pub mirror: u8,
    #[serde(default = "one")]
    pub normal_strength: f32,
    #[serde(default)]
    pub scalar_invert: bool,
    #[serde(default)]
    pub mask_base: u8,
    #[serde(default)]
    pub adjustments: ColorAdjustmentRecord,
    #[serde(default)]
    pub pins: Vec<PinRecord>,
    #[serde(default)]
    pub mask: Option<RunLengthMask>,
}

const fn yes() -> bool {
    true
}

const fn one() -> f32 {
    1.0
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ColorAdjustmentRecord {
    #[serde(default)]
    pub exposure: f32,
    #[serde(default)]
    pub contrast: f32,
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub hue_degrees: f32,
    #[serde(default)]
    pub temperature: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PinRecord {
    #[serde(default)]
    pub source: Option<[f32; 2]>,
    #[serde(default)]
    pub target_triangle: Option<u32>,
    #[serde(default)]
    pub target_barycentric: [f64; 3],
    #[serde(default)]
    pub target_uv: [f32; 2],
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RunLengthMask {
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,

    #[serde(default)]
    pub runs: Vec<(u8, u32)>,
}

impl RunLengthMask {
    pub fn encode(width: u32, height: u32, alpha: &[u8]) -> Self {
        let mut runs: Vec<(u8, u32)> = Vec::new();
        for value in alpha.iter().copied() {
            match runs.last_mut() {
                Some((run_value, count)) if *run_value == value && *count < u32::MAX => {
                    *count += 1;
                }
                _ => runs.push((value, 1)),
            }
        }
        Self {
            width,
            height,
            runs,
        }
    }

    pub fn decode(&self) -> Option<Vec<u8>> {
        let expected = (self.width as usize).checked_mul(self.height as usize)?;
        let total: usize = self
            .runs
            .iter()
            .try_fold(0_usize, |sum, (_, count)| sum.checked_add(*count as usize))?;
        if total != expected {
            return None;
        }
        let mut alpha = Vec::with_capacity(expected);
        for (value, count) in &self.runs {
            alpha.extend(std::iter::repeat_n(*value, *count as usize));
        }
        Some(alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sculpt_survives_the_round_trip_and_only_the_moved_vertices_are_stored() {
        let mut dense = vec![[0.0_f64; 3]; 1000];
        dense[7] = [0.5, -0.25, 0.125];
        dense[900] = [1.0, 2.0, 3.0];

        dense[500] = [1.0e-12, 0.0, 0.0];

        let sparse = SparseDisplacement::from_dense(&dense);
        assert_eq!(sparse.indices, vec![7, 900]);
        let restored = sparse.to_dense(1000).expect("same head");
        assert_eq!(restored[7], [0.5, -0.25, 0.125]);
        assert_eq!(restored[900], [1.0, 2.0, 3.0]);
        assert_eq!(restored[500], [0.0; 3]);
    }

    #[test]
    fn a_sculpt_taken_against_another_head_is_refused_rather_than_applied() {
        let sparse = SparseDisplacement::from_dense(&[[1.0, 0.0, 0.0], [0.0; 3]]);
        assert!(sparse.to_dense(2).is_some());
        assert!(sparse.to_dense(3).is_none());
        assert!(sparse.to_dense(1).is_none());
    }

    #[test]
    fn an_unsculpted_head_stores_no_vertices() {
        let sparse = SparseDisplacement::from_dense(&[[0.0; 3]; 50_000]);
        assert!(sparse.is_empty());
        assert_eq!(
            sparse.to_dense(50_000).map(|dense| dense.len()),
            Some(50_000)
        );
    }

    #[test]
    fn a_mask_survives_the_round_trip() {
        let mut alpha = vec![0_u8; 64 * 32];
        for (index, value) in alpha.iter_mut().enumerate() {
            *value = if index % 7 == 0 {
                255
            } else {
                u8::try_from(index % 251).unwrap()
            };
        }
        let coded = RunLengthMask::encode(64, 32, &alpha);
        assert_eq!(coded.decode().as_deref(), Some(alpha.as_slice()));
    }

    #[test]
    fn a_uniform_mask_costs_one_run() {
        let coded = RunLengthMask::encode(2048, 2048, &vec![255_u8; 2048 * 2048]);
        assert_eq!(coded.runs, vec![(255, 2048 * 2048)]);
        assert_eq!(coded.decode().map(|alpha| alpha.len()), Some(2048 * 2048));
    }

    #[test]
    fn runs_that_do_not_add_up_are_refused() {
        let short = RunLengthMask {
            width: 4,
            height: 4,
            runs: vec![(255, 8)],
        };
        assert!(short.decode().is_none());
        let long = RunLengthMask {
            width: 4,
            height: 4,
            runs: vec![(255, 16), (0, 1)],
        };
        assert!(long.decode().is_none());
        let overflowing = RunLengthMask {
            width: 4,
            height: 4,
            runs: vec![(255, u32::MAX), (0, u32::MAX)],
        };
        assert!(overflowing.decode().is_none());
    }

    #[test]
    fn a_session_with_no_work_in_it_reports_none() {
        let empty = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            ..SessionSnapshot::default()
        };
        assert!(!empty.has_work());
        for populated in [
            SessionSnapshot {
                morph_values: BTreeMap::from([("brow".to_owned(), 0.4)]),
                ..empty.clone()
            },
            SessionSnapshot {
                sculpt: SparseDisplacement::from_dense(&[[1.0, 0.0, 0.0]]),
                ..empty.clone()
            },
            SessionSnapshot {
                texture_layers: vec![TextureLayerRecord {
                    source_path: Some(std::path::PathBuf::from("face.png")),
                    ..TextureLayerRecord::default()
                }],
                ..empty.clone()
            },
            SessionSnapshot {
                eye_closure: 0.3,
                ..empty.clone()
            },
        ] {
            assert!(populated.has_work());
        }
    }

    #[test]
    fn the_snapshot_survives_json() {
        let snapshot = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            figure_sex: FigureSex::Male,
            look_id: Some("var:Author.Look.1:/Custom/x.vap".to_owned()),
            morph_values: BTreeMap::from([("nose".to_owned(), -0.25)]),
            eye_closure: 0.5,
            sculpt: SparseDisplacement::from_dense(&[[0.0; 3], [0.1, 0.2, 0.3]]),
            texture_layers: vec![TextureLayerRecord {
                name: "freckles".to_owned(),
                source_path: Some(PathBuf::from("C:/x/y.png")),
                opacity: 0.6,
                mask: Some(RunLengthMask::encode(2, 2, &[0, 0, 255, 255])),
                pins: vec![PinRecord {
                    source: Some([0.25, 0.75]),
                    target_triangle: Some(12),
                    ..PinRecord::default()
                }],
                ..TextureLayerRecord::default()
            }],
        };
        let text = serde_json::to_string(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(restored, snapshot);
        assert!(restored.is_readable());
    }

    #[test]
    fn an_unreadable_version_is_recognised_and_absent_fields_default() {
        let future: SessionSnapshot = serde_json::from_str(r#"{"version": 9999}"#).unwrap();
        assert!(!future.is_readable());
        assert!(!future.has_work());

        let minimal: SessionSnapshot =
            serde_json::from_str(&format!(r#"{{"version": {SNAPSHOT_VERSION}}}"#)).unwrap();
        assert!(minimal.is_readable());
        assert_eq!(minimal.figure_sex, FigureSex::default());
        assert!(minimal.texture_layers.is_empty());
    }
}
