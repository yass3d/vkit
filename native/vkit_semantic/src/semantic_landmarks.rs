use std::io::{Cursor, Read};
use std::sync::{Arc, OnceLock};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tract_tflite::prelude::*;

pub const MODEL_INPUT_WIDTH: usize = 256;
pub const MODEL_INPUT_HEIGHT: usize = 256;
pub const SEMANTIC_LANDMARK_COUNT: usize = 478;
const RGB_CHANNELS: usize = 3;
const LANDMARK_OUTPUT_VALUES: usize = SEMANTIC_LANDMARK_COUNT * 3;

const FACE_DETECTOR_GZIP: &[u8] = include_bytes!("../models/face_detector.tflite.gz");
const FACE_LANDMARK_GZIP: &[u8] = include_bytes!("../models/face_landmarks_detector.tflite.gz");

const FACE_DETECTOR_SHA256: [u8; 32] = [
    0xb4, 0x57, 0x8f, 0x35, 0x94, 0x0b, 0xf5, 0xa1, 0xa6, 0x55, 0x21, 0x4a, 0x1c, 0xce, 0x5c, 0xab,
    0x13, 0xeb, 0xa7, 0x3c, 0x12, 0x97, 0xcd, 0x78, 0xe1, 0xa0, 0x4c, 0x23, 0x80, 0xb0, 0x15, 0x2f,
];
const FACE_LANDMARK_SHA256: [u8; 32] = [
    0xc7, 0xd5, 0x42, 0x04, 0xce, 0x04, 0x48, 0x47, 0x4c, 0x7f, 0x3f, 0xa9, 0xaf, 0x49, 0x47, 0x87,
    0xc0, 0x96, 0x5c, 0xbd, 0xd6, 0xf2, 0x0f, 0xc7, 0x28, 0x67, 0xe4, 0x30, 0x46, 0xbd, 0x43, 0xd5,
];

#[derive(Clone, Copy, Debug)]
pub struct EmbeddedModelAsset {
    pub name: &'static str,
    pub source_bundle_url: &'static str,
    pub decompressed_len: usize,
    pub decompressed_sha256: &'static str,
    compressed_bytes: &'static [u8],
    expected_sha256: [u8; 32],
}

impl EmbeddedModelAsset {
    pub const fn compressed_bytes(self) -> &'static [u8] {
        self.compressed_bytes
    }

    pub fn decompress_verified(self) -> Result<Vec<u8>, SemanticLandmarkError> {
        let mut bytes = Vec::with_capacity(self.decompressed_len);
        GzDecoder::new(self.compressed_bytes)
            .read_to_end(&mut bytes)
            .map_err(|error| SemanticLandmarkError::ModelAsset {
                model: self.name,
                detail: format!("gzip decode failed: {error}"),
            })?;
        if bytes.len() != self.decompressed_len {
            return Err(SemanticLandmarkError::ModelAsset {
                model: self.name,
                detail: format!(
                    "expected {} decompressed bytes, found {}",
                    self.decompressed_len,
                    bytes.len()
                ),
            });
        }
        let digest = Sha256::digest(&bytes);
        if digest.as_slice() != self.expected_sha256 {
            return Err(SemanticLandmarkError::ModelAsset {
                model: self.name,
                detail: format!("SHA-256 mismatch; expected {}", self.decompressed_sha256),
            });
        }
        Ok(bytes)
    }
}

const SOURCE_BUNDLE_URL: &str = "https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/latest/face_landmarker.task";

pub const FACE_DETECTOR_MODEL: EmbeddedModelAsset = EmbeddedModelAsset {
    name: "face_detector.tflite",
    source_bundle_url: SOURCE_BUNDLE_URL,
    decompressed_len: 229_746,
    decompressed_sha256: "B4578F35940BF5A1A655214A1CCE5CAB13EBA73C1297CD78E1A04C2380B0152F",
    compressed_bytes: FACE_DETECTOR_GZIP,
    expected_sha256: FACE_DETECTOR_SHA256,
};

pub const FACE_LANDMARK_MODEL: EmbeddedModelAsset = EmbeddedModelAsset {
    name: "face_landmarks_detector.tflite",
    source_bundle_url: SOURCE_BUNDLE_URL,
    decompressed_len: 2_553_590,
    decompressed_sha256: "C7D54204CE0448474C7F3FA9AF494787C0965CBDD6F20FC72867E43046BD43D5",
    compressed_bytes: FACE_LANDMARK_GZIP,
    expected_sha256: FACE_LANDMARK_SHA256,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SemanticLandmark {
    pub x: f32,

    pub y: f32,

    pub z: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticLandmarkPrediction {
    pub landmarks: [SemanticLandmark; SEMANTIC_LANDMARK_COUNT],

    pub presence_logit: f32,
    pub presence_probability: f32,

    pub tongue_out_probability: f32,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SemanticLandmarkError {
    #[error("semantic model asset {model} is invalid: {detail}")]
    ModelAsset { model: &'static str, detail: String },
    #[error("semantic model initialization failed: {0}")]
    ModelInitialization(String),
    #[error("semantic model inference failed: {0}")]
    Inference(String),
    #[error("landmark input must be 256 x 256 RGB float32, received {width} x {height}")]
    InvalidDimensions { width: usize, height: usize },
    #[error("landmark input has {actual} values; expected {expected}")]
    InvalidInputLength { actual: usize, expected: usize },
    #[error("landmark input contains a non-finite value at index {index}")]
    NonFiniteInput { index: usize },
    #[error("semantic model returned an invalid output: {0}")]
    InvalidOutput(String),
}

pub struct SemanticLandmarkEngine {
    landmark_model: Arc<TypedRunnableModel>,
}

impl std::fmt::Debug for SemanticLandmarkEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SemanticLandmarkEngine")
            .field("model", &FACE_LANDMARK_MODEL.name)
            .finish_non_exhaustive()
    }
}

impl SemanticLandmarkEngine {
    pub fn global() -> Result<&'static Self, SemanticLandmarkError> {
        static ENGINE: OnceLock<Result<SemanticLandmarkEngine, SemanticLandmarkError>> =
            OnceLock::new();
        match ENGINE.get_or_init(Self::load) {
            Ok(engine) => Ok(engine),
            Err(error) => Err(error.clone()),
        }
    }

    fn load() -> Result<Self, SemanticLandmarkError> {
        let model_bytes = FACE_LANDMARK_MODEL.decompress_verified()?;
        let mut reader = Cursor::new(model_bytes);
        let landmark_model = tract_tflite::tflite()
            .model_for_read(&mut reader)
            .and_then(|model| model.into_optimized())
            .and_then(|model| model.into_runnable())
            .map_err(|error| SemanticLandmarkError::ModelInitialization(error.to_string()))?;
        Ok(Self { landmark_model })
    }

    pub fn detect_landmarks_rgb_f32(
        &self,
        rgb: &[f32],
        width: usize,
        height: usize,
    ) -> Result<SemanticLandmarkPrediction, SemanticLandmarkError> {
        validate_input(rgb, width, height)?;
        let input = Tensor::from_shape(&[1, height, width, RGB_CHANNELS], rgb)
            .map_err(|error| SemanticLandmarkError::Inference(error.to_string()))?;
        let outputs = self
            .landmark_model
            .run(tvec!(input.into()))
            .map_err(|error| SemanticLandmarkError::Inference(error.to_string()))?;
        decode_landmark_outputs(&outputs, width, height)
    }
}

pub fn detect_landmarks_rgb_f32(
    rgb: &[f32],
    width: usize,
    height: usize,
) -> Result<SemanticLandmarkPrediction, SemanticLandmarkError> {
    SemanticLandmarkEngine::global()?.detect_landmarks_rgb_f32(rgb, width, height)
}

fn validate_input(rgb: &[f32], width: usize, height: usize) -> Result<(), SemanticLandmarkError> {
    if width != MODEL_INPUT_WIDTH || height != MODEL_INPUT_HEIGHT {
        return Err(SemanticLandmarkError::InvalidDimensions { width, height });
    }
    let expected = width * height * RGB_CHANNELS;
    if rgb.len() != expected {
        return Err(SemanticLandmarkError::InvalidInputLength {
            actual: rgb.len(),
            expected,
        });
    }
    if let Some(index) = rgb.iter().position(|value| !value.is_finite()) {
        return Err(SemanticLandmarkError::NonFiniteInput { index });
    }
    Ok(())
}

fn decode_landmark_outputs(
    outputs: &[TValue],
    width: usize,
    height: usize,
) -> Result<SemanticLandmarkPrediction, SemanticLandmarkError> {
    if outputs.len() != 3 {
        return Err(SemanticLandmarkError::InvalidOutput(format!(
            "expected 3 tensors, found {}",
            outputs.len()
        )));
    }

    let raw_landmarks = outputs[0]
        .to_plain_array_view::<f32>()
        .map_err(|error| SemanticLandmarkError::InvalidOutput(error.to_string()))?;
    if raw_landmarks.len() != LANDMARK_OUTPUT_VALUES {
        return Err(SemanticLandmarkError::InvalidOutput(format!(
            "expected {LANDMARK_OUTPUT_VALUES} landmark values, found {}",
            raw_landmarks.len()
        )));
    }
    let raw_values: Vec<f32> = raw_landmarks.iter().copied().collect();
    let mut landmarks = [SemanticLandmark::default(); SEMANTIC_LANDMARK_COUNT];
    for (landmark, raw) in landmarks.iter_mut().zip(raw_values.chunks_exact(3)) {
        *landmark = SemanticLandmark {
            x: raw[0] / width as f32,
            y: raw[1] / height as f32,
            z: raw[2] / width as f32,
        };
    }

    let presence_logit = scalar_output(&outputs[1], "presence")?;
    let tongue_out_probability = scalar_output(&outputs[2], "tongue-out")?;
    if !landmarks
        .iter()
        .flat_map(|point| [point.x, point.y, point.z])
        .chain([presence_logit, tongue_out_probability])
        .all(f32::is_finite)
    {
        return Err(SemanticLandmarkError::InvalidOutput(
            "one or more values are non-finite".to_owned(),
        ));
    }

    Ok(SemanticLandmarkPrediction {
        landmarks,
        presence_logit,
        presence_probability: sigmoid(presence_logit),
        tongue_out_probability,
    })
}

fn scalar_output(tensor: &TValue, name: &str) -> Result<f32, SemanticLandmarkError> {
    let view = tensor
        .to_plain_array_view::<f32>()
        .map_err(|error| SemanticLandmarkError::InvalidOutput(error.to_string()))?;
    if view.len() != 1 {
        return Err(SemanticLandmarkError::InvalidOutput(format!(
            "{name} tensor contains {} values instead of 1",
            view.len()
        )));
    }
    Ok(*view.iter().next().expect("scalar length checked"))
}

fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_models_match_their_published_hashes() {
        assert_eq!(
            FACE_DETECTOR_MODEL.decompress_verified().unwrap().len(),
            FACE_DETECTOR_MODEL.decompressed_len
        );
        assert_eq!(
            FACE_LANDMARK_MODEL.decompress_verified().unwrap().len(),
            FACE_LANDMARK_MODEL.decompressed_len
        );
    }

    #[test]
    fn input_validation_is_explicit() {
        let error = validate_input(&[], 128, 128).unwrap_err();
        assert!(matches!(
            error,
            SemanticLandmarkError::InvalidDimensions {
                width: 128,
                height: 128
            }
        ));
        let error = validate_input(&[], 256, 256).unwrap_err();
        assert!(matches!(
            error,
            SemanticLandmarkError::InvalidInputLength { .. }
        ));
    }

    #[test]
    fn sigmoid_is_stable_at_extreme_logits() {
        assert_eq!(sigmoid(1000.0), 1.0);
        assert_eq!(sigmoid(-1000.0), 0.0);
        assert_eq!(sigmoid(0.0), 0.5);
    }

    #[test]
    fn decoder_normalizes_xyz_and_preserves_semantic_order() {
        let mut raw = vec![0.0f32; LANDMARK_OUTPUT_VALUES];
        for (index, point) in raw.chunks_exact_mut(3).enumerate() {
            point[0] = index as f32;
            point[1] = index as f32 * 2.0;
            point[2] = -(index as f32);
        }
        let outputs = tvec!(
            Tensor::from_shape(&[1, 1, 1, LANDMARK_OUTPUT_VALUES], &raw)
                .unwrap()
                .into(),
            tensor4(&[[[[2.0f32]]]]).into(),
            tensor2(&[[0.25f32]]).into(),
        );
        let prediction = decode_landmark_outputs(&outputs, 256, 256).unwrap();
        assert_eq!(prediction.landmarks.len(), SEMANTIC_LANDMARK_COUNT);
        assert_eq!(prediction.landmarks[0], SemanticLandmark::default());
        assert_eq!(prediction.landmarks[477].x, 477.0 / 256.0);
        assert_eq!(prediction.landmarks[477].y, 954.0 / 256.0);
        assert_eq!(prediction.landmarks[477].z, -477.0 / 256.0);
        assert_eq!(prediction.presence_logit, 2.0);
        assert!((prediction.presence_probability - 0.880_797).abs() < 1.0e-6);
        assert_eq!(prediction.tongue_out_probability, 0.25);
    }
}
