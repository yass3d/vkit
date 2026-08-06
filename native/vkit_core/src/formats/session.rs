use std::collections::HashSet;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{FormatError, Mesh, Result, canonical_mesh_hash_hex};

pub const LANDMARK_FORMAT: &str = "vkit.landmark_pairs";

pub const LANDMARK_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshBinding {
    pub canonical_sha256: String,
    pub vertex_count: usize,
    pub triangle_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_uri: Option<String>,
}

impl MeshBinding {
    pub fn validate(&mut self) -> Result<()> {
        if self.canonical_sha256.len() != 64
            || !self
                .canonical_sha256
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(session_error(
                "canonical_sha256 must be a 64-character hexadecimal digest",
            ));
        }
        self.canonical_sha256.make_ascii_lowercase();
        Ok(())
    }

    pub fn validate_mesh(&self, mesh: &Mesh, label: &str) -> Result<()> {
        mesh.validate()?;
        let actual_hash = canonical_mesh_hash_hex(&mesh.vertices, &mesh.triangles)?;
        let mut mismatches = Vec::new();
        if self.vertex_count != mesh.vertices.len() {
            mismatches.push(format!(
                "vertex_count expected {}, got {}",
                self.vertex_count,
                mesh.vertices.len()
            ));
        }
        if self.triangle_count != mesh.triangles.len() {
            mismatches.push(format!(
                "triangle_count expected {}, got {}",
                self.triangle_count,
                mesh.triangles.len()
            ));
        }
        if self.canonical_sha256 != actual_hash {
            mismatches.push(format!(
                "canonical_sha256 expected {}, got {actual_hash}",
                self.canonical_sha256
            ));
        }
        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(session_error(format!(
                "{label} binding mismatch: {}",
                mismatches.join("; ")
            )))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceAttachment {
    pub triangle_vertex_ids: [u32; 3],
    pub barycentric: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primitive_id: Option<u32>,
}

impl SurfaceAttachment {
    pub fn validate_and_normalize(&mut self) -> Result<()> {
        if !self.barycentric.iter().all(|value| value.is_finite()) {
            return Err(session_error("surface barycentric weights must be finite"));
        }
        if self
            .barycentric
            .iter()
            .any(|value| *value < -1.0e-7 || *value > 1.0 + 1.0e-7)
        {
            return Err(session_error(
                "surface barycentric weights must lie between zero and one",
            ));
        }
        let sum: f64 = self.barycentric.iter().sum();
        if (sum - 1.0).abs() > 1.0e-6 {
            return Err(session_error("surface barycentric weights must sum to one"));
        }
        for value in &mut self.barycentric {
            *value = value.clamp(0.0, 1.0) / sum;
        }

        let normalized_sum: f64 = self.barycentric.iter().sum();
        for value in &mut self.barycentric {
            *value /= normalized_sum;
        }
        Ok(())
    }

    pub fn resolve(&self, vertices: &[[f64; 3]]) -> Result<[f64; 3]> {
        let mut attachment = self.clone();
        attachment.validate_and_normalize()?;
        for &vertex_id in &attachment.triangle_vertex_ids {
            if vertex_id as usize >= vertices.len() {
                return Err(session_error(format!(
                    "surface attachment references vertex {vertex_id}, but the mesh has {} vertices",
                    vertices.len()
                )));
            }
        }
        let mut point = [0.0; 3];
        for corner in 0..3 {
            let vertex = vertices[attachment.triangle_vertex_ids[corner] as usize];
            if !vertex.iter().all(|coordinate| coordinate.is_finite()) {
                return Err(session_error(
                    "surface attachment mesh contains a non-finite vertex",
                ));
            }
            for axis in 0..3 {
                point[axis] += attachment.barycentric[corner] * vertex[axis];
            }
        }
        Ok(point)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LandmarkEndpoint {
    pub coordinate_space: String,
    pub point: [f64; 3],
    pub surface: SurfaceAttachment,
    #[serde(default = "empty_json_object", skip_serializing_if = "is_empty_object")]
    pub pick: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_anchor: Option<String>,
}

impl LandmarkEndpoint {
    pub fn validate(&mut self) -> Result<()> {
        if self.coordinate_space.trim().is_empty() {
            return Err(session_error("endpoint coordinate_space must not be empty"));
        }
        if !self.point.iter().all(|value| value.is_finite()) {
            return Err(session_error(
                "endpoint point must contain three finite coordinates",
            ));
        }
        if !self.pick.is_object() {
            return Err(session_error("endpoint pick must be a JSON object"));
        }
        self.surface.validate_and_normalize()
    }

    pub fn resolve(
        &self,
        vertices: &[[f64; 3]],
        validate_cached_point: bool,
        tolerance: f64,
    ) -> Result<[f64; 3]> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(session_error(
                "endpoint resolution tolerance must be finite and non-negative",
            ));
        }
        let resolved = self.surface.resolve(vertices)?;
        if validate_cached_point {
            let error = resolved
                .iter()
                .zip(self.point)
                .map(|(actual, cached)| (actual - cached).powi(2))
                .sum::<f64>()
                .sqrt();
            if error > tolerance {
                return Err(session_error(format!(
                    "cached endpoint differs from its surface attachment by {error}"
                )));
            }
        }
        Ok(resolved)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstraintSettings {
    pub enabled: bool,
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance_cm: Option<f64>,
}

impl ConstraintSettings {
    fn validate(&self, label: &str) -> Result<()> {
        if !self.weight.is_finite() || self.weight < 0.0 {
            return Err(session_error(format!(
                "{label} weight must be finite and non-negative"
            )));
        }
        if self
            .tolerance_cm
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(session_error(format!(
                "{label} tolerance_cm must be finite and non-negative"
            )));
        }
        Ok(())
    }
}

fn default_alignment() -> ConstraintSettings {
    ConstraintSettings {
        enabled: false,
        weight: 0.0,
        tolerance_cm: None,
    }
}

fn default_fit() -> ConstraintSettings {
    ConstraintSettings {
        enabled: true,
        weight: 1.0,
        tolerance_cm: None,
    }
}

fn default_true() -> bool {
    true
}

fn default_confidence() -> f64 {
    1.0
}

fn default_status() -> String {
    "draft".to_owned()
}

fn default_source() -> String {
    "manual".to_owned()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LandmarkPair {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub region: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub override_auto_ids: Vec<u32>,
    #[serde(default = "default_alignment")]
    pub alignment: ConstraintSettings,
    #[serde(default = "default_fit")]
    pub fit: ConstraintSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<LandmarkEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<LandmarkEndpoint>,
}

impl LandmarkPair {
    pub fn validate(&mut self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(session_error("landmark id must not be empty"));
        }
        if self.region.trim().is_empty() {
            return Err(session_error(format!(
                "landmark {:?} region must not be empty",
                self.id
            )));
        }
        if !matches!(self.status.as_str(), "draft" | "complete") {
            return Err(session_error(format!(
                "landmark {:?} status must be draft or complete",
                self.id
            )));
        }
        if self.source.trim().is_empty() {
            return Err(session_error(format!(
                "landmark {:?} source must not be empty",
                self.id
            )));
        }
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(session_error(format!(
                "landmark {:?} confidence must lie between zero and one",
                self.id
            )));
        }
        let unique_override_count = self
            .override_auto_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .len();
        if unique_override_count != self.override_auto_ids.len() {
            return Err(session_error(format!(
                "landmark {:?} override_auto_ids must be unique",
                self.id
            )));
        }
        if self.status == "complete" && (self.scan.is_none() || self.template.is_none()) {
            return Err(session_error(format!(
                "complete landmark {:?} requires scan and template endpoints",
                self.id
            )));
        }
        if let Some(scan) = &mut self.scan {
            scan.validate()?;
        }
        if let Some(template) = &mut self.template {
            template.validate()?;
        }
        self.alignment.validate("alignment")?;
        self.fit.validate("fit")
    }

    pub fn is_complete_enabled(&self) -> bool {
        self.status == "complete" && self.enabled && self.scan.is_some() && self.template.is_some()
    }

    pub fn is_alignment_constraint(&self) -> bool {
        self.is_complete_enabled()
            && self.alignment.enabled
            && self.alignment.weight > 0.0
            && self.confidence > 0.0
    }

    pub fn is_fit_constraint(&self) -> bool {
        self.is_complete_enabled()
            && self.fit.enabled
            && self.fit.weight > 0.0
            && self.confidence > 0.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeshes {
    pub scan: MeshBinding,
    pub template: MeshBinding,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LegacyLandmarkSessionV1 {
    pub format: String,
    pub schema_version: u32,
    pub meshes: SessionMeshes,
    #[serde(default = "empty_json_object")]
    pub views: Value,
    #[serde(default)]
    pub pairs: Vec<LandmarkPair>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_preset: Option<String>,
    #[serde(default = "empty_json_object")]
    pub metadata: Value,
}

impl LegacyLandmarkSessionV1 {
    pub fn from_reader(reader: impl Read) -> Result<Self> {
        let mut session: Self = serde_json::from_reader(reader)?;
        session.validate()?;
        Ok(session)
    }

    pub fn from_slice(encoded: &[u8]) -> Result<Self> {
        let mut session: Self = serde_json::from_slice(encoded)?;
        session.validate()?;
        Ok(session)
    }

    pub fn write_pretty(&self, mut writer: impl Write) -> Result<()> {
        let mut checked = self.clone();
        checked.validate()?;
        serde_json::to_writer_pretty(&mut writer, &checked)?;
        writeln!(writer)?;
        Ok(())
    }

    pub fn validate(&mut self) -> Result<()> {
        if self.format != LANDMARK_FORMAT {
            return Err(session_error(format!(
                "format must be {LANDMARK_FORMAT:?}, got {:?}",
                self.format
            )));
        }
        if self.schema_version != LANDMARK_SCHEMA_VERSION {
            return Err(session_error(format!(
                "unsupported schema_version {}",
                self.schema_version
            )));
        }
        if !self.views.is_object() || !self.metadata.is_object() {
            return Err(session_error("views and metadata must be JSON objects"));
        }
        self.meshes.scan.validate()?;
        self.meshes.template.validate()?;
        let mut ids = HashSet::with_capacity(self.pairs.len());
        for pair in &mut self.pairs {
            pair.validate()?;
            if !ids.insert(pair.id.clone()) {
                return Err(session_error(format!(
                    "duplicate landmark ID {:?}",
                    pair.id
                )));
            }
        }
        Ok(())
    }

    pub fn validate_meshes(&self, scan: &Mesh, template: &Mesh) -> Result<()> {
        self.meshes.scan.validate_mesh(scan, "scan mesh")?;
        self.meshes
            .template
            .validate_mesh(template, "template mesh")
    }

    pub fn alignment_pair_count(&self) -> usize {
        self.pairs
            .iter()
            .filter(|pair| pair.is_alignment_constraint())
            .count()
    }

    pub fn fit_pair_count(&self) -> usize {
        self.pairs
            .iter()
            .filter(|pair| pair.is_fit_constraint())
            .count()
    }
}

fn empty_json_object() -> Value {
    Value::Object(Default::default())
}

fn is_empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(|object| object.is_empty())
}

fn session_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidSession(message.into())
}
