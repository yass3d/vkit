use crate::semantic_landmarks::{FACE_DETECTOR_MODEL, SemanticLandmarkError};

use std::sync::{Arc, OnceLock};

use tract_tflite::prelude::*;

pub const DETECTOR_INPUT_WIDTH: usize = 128;
pub const DETECTOR_INPUT_HEIGHT: usize = 128;
const RGB_CHANNELS: usize = 3;

const ANCHOR_COUNT: usize = 896;

const REGRESSION_VALUES: usize = 16;
const KEYPOINT_COUNT: usize = 6;

const MINIMUM_SCORE: f32 = 0.5;

const SUPPRESSION_IOU: f32 = 0.3;

const COORDINATE_SCALE: f32 = 128.0;

const LOGIT_LIMIT: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectedFace {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub score: f32,

    pub keypoints: [[f32; 2]; KEYPOINT_COUNT],
}

impl DetectedFace {
    fn intersection_over_union(self, other: Self) -> f32 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let overlap = (right - left).max(0.0) * (bottom - top).max(0.0);
        let union = self.width * self.height + other.width * other.height - overlap;
        if union > 0.0 { overlap / union } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    x: f32,
    y: f32,
}

fn anchors() -> Vec<Anchor> {
    const LAYERS: [(usize, usize); 2] = [(16, 2), (8, 6)];
    let mut anchors = Vec::with_capacity(ANCHOR_COUNT);
    for (grid, per_cell) in LAYERS {
        for row in 0..grid {
            for column in 0..grid {
                let x = (column as f32 + 0.5) / grid as f32;
                let y = (row as f32 + 0.5) / grid as f32;
                for _ in 0..per_cell {
                    anchors.push(Anchor { x, y });
                }
            }
        }
    }
    anchors
}

fn logistic(logit: f32) -> f32 {
    1.0 / (1.0 + (-logit.clamp(-LOGIT_LIMIT, LOGIT_LIMIT)).exp())
}

pub struct FaceDetectorEngine {
    model: Arc<TypedRunnableModel>,
    anchors: Vec<Anchor>,
}

impl std::fmt::Debug for FaceDetectorEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FaceDetectorEngine")
            .field("model", &FACE_DETECTOR_MODEL.name)
            .finish_non_exhaustive()
    }
}

static DETECTOR: OnceLock<Result<FaceDetectorEngine, SemanticLandmarkError>> = OnceLock::new();

impl FaceDetectorEngine {
    pub fn global() -> Result<&'static Self, SemanticLandmarkError> {
        DETECTOR
            .get_or_init(Self::build)
            .as_ref()
            .map_err(Clone::clone)
    }

    fn build() -> Result<Self, SemanticLandmarkError> {
        let bytes = FACE_DETECTOR_MODEL.decompress_verified()?;
        let model = tract_tflite::tflite()
            .model_for_read(&mut std::io::Cursor::new(bytes))
            .and_then(|model| model.into_optimized())
            .and_then(|model| model.into_runnable())
            .map_err(|error| SemanticLandmarkError::ModelInitialization(error.to_string()))?;
        Ok(Self {
            model,
            anchors: anchors(),
        })
    }

    pub fn detect_rgb_f32(
        &self,
        rgb: &[f32],
        width: usize,
        height: usize,
    ) -> Result<Vec<DetectedFace>, SemanticLandmarkError> {
        if width != DETECTOR_INPUT_WIDTH || height != DETECTOR_INPUT_HEIGHT {
            return Err(SemanticLandmarkError::InvalidDimensions { width, height });
        }
        let expected = width * height * RGB_CHANNELS;
        if rgb.len() != expected {
            return Err(SemanticLandmarkError::InvalidInputLength {
                actual: rgb.len(),
                expected,
            });
        }
        let input = Tensor::from_shape(&[1, height, width, RGB_CHANNELS], rgb)
            .map_err(|error| SemanticLandmarkError::Inference(error.to_string()))?;
        let outputs = self
            .model
            .run(tvec!(input.into()))
            .map_err(|error| SemanticLandmarkError::Inference(error.to_string()))?;
        let boxes = outputs[0]
            .to_plain_array_view::<f32>()
            .map_err(|error| SemanticLandmarkError::InvalidOutput(error.to_string()))?;
        let scores = outputs[1]
            .to_plain_array_view::<f32>()
            .map_err(|error| SemanticLandmarkError::InvalidOutput(error.to_string()))?;
        let boxes = boxes.iter().copied().collect::<Vec<f32>>();
        let scores = scores.iter().copied().collect::<Vec<f32>>();
        if boxes.len() != ANCHOR_COUNT * REGRESSION_VALUES || scores.len() != ANCHOR_COUNT {
            return Err(SemanticLandmarkError::InvalidOutput(format!(
                "detector emitted {} box values and {} scores; expected {} and {ANCHOR_COUNT}",
                boxes.len(),
                scores.len(),
                ANCHOR_COUNT * REGRESSION_VALUES,
            )));
        }
        Ok(suppress_overlaps(decode(&boxes, &scores, &self.anchors)))
    }
}

fn decode(boxes: &[f32], scores: &[f32], anchors: &[Anchor]) -> Vec<DetectedFace> {
    let mut found = Vec::new();
    for (index, anchor) in anchors.iter().enumerate() {
        let score = logistic(scores[index]);
        if score < MINIMUM_SCORE {
            continue;
        }
        let raw = &boxes[index * REGRESSION_VALUES..(index + 1) * REGRESSION_VALUES];
        let center_x = raw[0] / COORDINATE_SCALE + anchor.x;
        let center_y = raw[1] / COORDINATE_SCALE + anchor.y;
        let width = raw[2] / COORDINATE_SCALE;
        let height = raw[3] / COORDINATE_SCALE;
        let keypoints = std::array::from_fn(|point| {
            [
                raw[4 + point * 2] / COORDINATE_SCALE + anchor.x,
                raw[5 + point * 2] / COORDINATE_SCALE + anchor.y,
            ]
        });
        found.push(DetectedFace {
            x: center_x - width * 0.5,
            y: center_y - height * 0.5,
            width,
            height,
            score,
            keypoints,
        });
    }
    found
}

fn suppress_overlaps(mut candidates: Vec<DetectedFace>) -> Vec<DetectedFace> {
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<DetectedFace> = Vec::new();
    for candidate in candidates {
        if kept
            .iter()
            .any(|face| face.intersection_over_union(candidate) > SUPPRESSION_IOU)
        {
            continue;
        }
        kept.push(candidate);
    }
    kept
}

pub fn detect_faces_rgb_f32(
    rgb: &[f32],
    width: usize,
    height: usize,
) -> Result<Vec<DetectedFace>, SemanticLandmarkError> {
    FaceDetectorEngine::global()?.detect_rgb_f32(rgb, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_anchor_grid_matches_the_networks_box_count() {
        assert_eq!(anchors().len(), ANCHOR_COUNT);
    }

    #[test]
    fn every_anchor_sits_inside_the_input_square() {
        for anchor in anchors() {
            assert!((0.0..=1.0).contains(&anchor.x), "anchor x {}", anchor.x);
            assert!((0.0..=1.0).contains(&anchor.y), "anchor y {}", anchor.y);
        }
    }

    #[test]
    fn both_feature_maps_contribute_anchors() {
        let anchors = anchors();
        let coarse = anchors.iter().filter(|anchor| anchor.x < 0.1).count();
        assert_eq!(&anchors[..2].len(), &2);
        assert!(coarse >= 8, "both grids place anchors near the left edge");
    }

    #[test]
    fn the_logistic_is_bounded_for_extreme_logits() {
        assert!(logistic(f32::INFINITY).is_finite());
        assert!(logistic(-1.0e30).is_finite());
        assert!((logistic(0.0) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn overlapping_candidates_collapse_to_the_strongest() {
        let face = |x: f32, score: f32| DetectedFace {
            x,
            y: 0.0,
            width: 0.4,
            height: 0.4,
            score,
            keypoints: [[0.0; 2]; KEYPOINT_COUNT],
        };
        let kept = suppress_overlaps(vec![face(0.0, 0.7), face(0.02, 0.9), face(0.8, 0.8)]);

        assert_eq!(kept.len(), 2, "two distinct faces survive");
        assert!((kept[0].score - 0.9).abs() < 1.0e-6, "strongest first");
        assert!((kept[1].score - 0.8).abs() < 1.0e-6);
    }

    #[test]
    fn a_wrongly_sized_image_is_refused() {
        let engine = FaceDetectorEngine::global().expect("detector builds");
        assert!(matches!(
            engine.detect_rgb_f32(&[0.0; 3], 1, 1),
            Err(SemanticLandmarkError::InvalidDimensions { .. })
        ));
    }
}
