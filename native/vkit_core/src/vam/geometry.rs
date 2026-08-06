use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    G2F_POLYGON_COUNT, G2F_VERTEX_COUNT,
    formats::{
        DazGeometry, G2F_TOPOLOGY_SHA256, HEAD_VISUAL_MATERIALS, ObjFace, OrderedObjMesh,
        ensure_canonical_head_groups, topology_digest,
    },
};

pub const VAM_SHARED_VERTEX_COUNT: usize = G2F_VERTEX_COUNT;
pub const VAM_FEMALE_VERTEX_COUNT: usize = 24_928;
pub const VAM_FEMALE_FACE_COUNT: usize = 44_474;
pub const VAM_MALE_VERTEX_COUNT: usize = 24_801;
pub const VAM_MALE_FACE_COUNT: usize = 44_500;

pub const VAM_METRES_TO_DAZ_CM: f64 = 100.0;

pub const DEFAULT_PROTECTED_EPSILON_CM: f64 = 1.0e-3;

pub const NEUTRAL_BASIS_RMS_LIMIT_CM: f64 = 0.25;
pub const NEUTRAL_BASIS_MAX_LIMIT_CM: f64 = 2.0;

const SIDECAR_SCHEMA_VERSION: u32 = 1;
const MIN_DAZ_HEIGHT_CM: f64 = 150.0;
const MAX_DAZ_HEIGHT_CM: f64 = 220.0;
const MIN_VAM_HEIGHT_M: f64 = 1.5;
const MAX_VAM_HEIGHT_M: f64 = 2.2;
const MIN_AXIS_CORRELATION: f64 = 0.80;
const MIN_AXIS_SCALE_RATIO: f64 = 0.45;
const MAX_AXIS_SCALE_RATIO: f64 = 2.25;

const TOPOLOGY_FINGERPRINT_PREFIX: &[u8] = b"vkit.vam.topology.v1\0";
const VERTEX_FINGERPRINT_PREFIX: &[u8] = b"vkit.vam.vertices.v1\0";
const LABEL_FINGERPRINT_PREFIX: &[u8] = b"vkit.vam.face-labels.v1\0";
const VAM_FEMALE_TOPOLOGY_SHA256: [u8; 32] = [
    0x0d, 0xb0, 0xa2, 0x15, 0xd4, 0x34, 0x96, 0x91, 0xf5, 0xa4, 0xfa, 0x3b, 0x5e, 0x46, 0x8e, 0x5b,
    0xfd, 0x5c, 0x54, 0x59, 0xd2, 0x98, 0xc2, 0x2f, 0x7a, 0x00, 0xf4, 0xd5, 0xad, 0x00, 0x05, 0x06,
];
const VAM_MALE_TOPOLOGY_SHA256: [u8; 32] = [
    0x0b, 0xb3, 0xec, 0x8b, 0x1d, 0xd2, 0x92, 0x2b, 0xeb, 0x81, 0x64, 0xce, 0x86, 0x94, 0xbd, 0xa1,
    0x03, 0xe3, 0x88, 0x55, 0xdf, 0x6d, 0xca, 0xa8, 0x50, 0x72, 0xec, 0xda, 0x76, 0x85, 0x99, 0x68,
];

const REQUIRED_COMMON_MATERIALS: &[&str] = &["Face", "Head", "Neck", "Lips"];
const REQUIRED_FEMALE_GRAFT_MATERIALS: &[&str] = &["defaultMat-1"];
const REQUIRED_MALE_GRAFT_MATERIALS: &[&str] = &["Genitalia", "Anus-1"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GeometryFamily {
    DazGenesis2,
    VirtAMate,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum GeometrySex {
    Female,
    Male,
}

impl GeometrySex {
    pub const fn shortlist_from_counts(vertex_count: usize, face_count: usize) -> Option<Self> {
        match (vertex_count, face_count) {
            (VAM_FEMALE_VERTEX_COUNT, VAM_FEMALE_FACE_COUNT) => Some(Self::Female),
            (VAM_MALE_VERTEX_COUNT, VAM_MALE_FACE_COUNT) => Some(Self::Male),
            _ => None,
        }
    }

    pub const fn expected_counts(self) -> (usize, usize) {
        match self {
            Self::Female => (VAM_FEMALE_VERTEX_COUNT, VAM_FEMALE_FACE_COUNT),
            Self::Male => (VAM_MALE_VERTEX_COUNT, VAM_MALE_FACE_COUNT),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TopologyFingerprint {
    pub vertex_count: usize,
    pub polygon_count: usize,
    pub triangle_count: usize,
    pub sha256: [u8; 32],
}

impl TopologyFingerprint {
    pub fn sha256_hex(&self) -> String {
        self.sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VaMGeometrySidecarReceipt {
    pub schema_version: u32,
    pub anchor_family: GeometryFamily,
    pub basis_family: GeometryFamily,
    pub basis_sex: GeometrySex,
    pub shared_vertex_count: usize,
    pub daz_topology: TopologyFingerprint,
    pub vam_topology: TopologyFingerprint,
    pub daz_vertices_sha256: [u8; 32],
    pub vam_basis_vertices_sha256: [u8; 32],
    pub vam_face_labels_sha256: [u8; 32],
    pub canonical_triangle_count: usize,
    pub shared_canonical_triangle_count: usize,
    pub replaced_canonical_triangle_count: usize,
    pub replacement_shared_triangle_count: usize,
    pub appended_face_count: usize,
    pub protected_shared_vertex_count: usize,
    pub metres_to_centimetres: f64,
    pub protected_epsilon_cm: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtectedRegion {
    SharedGraftSeam,
    AppendedGeometry,
}

#[derive(Debug, Error, PartialEq)]
pub enum VaMGeometryError {
    #[error("invalid canonical DAZ anchor: {0}")]
    InvalidDazAnchor(String),

    #[error("invalid user-owned VaM basis: {0}")]
    InvalidVamBasis(String),

    #[error(
        "{sex:?} VaM basis expects {expected_vertices} vertices and {expected_faces} faces, got {actual_vertices} vertices and {actual_faces} faces"
    )]
    WrongSexOrCounts {
        sex: GeometrySex,
        expected_vertices: usize,
        expected_faces: usize,
        actual_vertices: usize,
        actual_faces: usize,
    },

    #[error("required {family:?} material {material:?} is missing from used faces")]
    MissingRequiredMaterial {
        family: GeometryFamily,
        material: String,
    },

    #[error("VaM face {face_index} has no material label")]
    MissingFaceMaterial { face_index: usize },

    #[error("topology validation failed: {0}")]
    TopologyMismatch(String),

    #[error("the replaced canonical graft region is disconnected from appended VaM geometry")]
    DisconnectedGraftRegion,

    #[error("coordinate-space validation failed: {0}")]
    CoordinateSpace(String),

    #[error("the supplied provider sidecar does not match the validated user-owned pair")]
    SidecarMismatch,

    #[error("target DAZ topology does not match the provider anchor")]
    TargetTopologyMismatch,

    #[error("source VaM topology or face labels do not match the provider basis")]
    SourceTopologyMismatch,

    #[error(
        "protected {region:?} vertex {vertex_index} changed by {displacement_cm:.6} cm (limit {epsilon_cm:.6} cm)"
    )]
    ProtectedRegionChanged {
        region: ProtectedRegion,
        vertex_index: usize,
        displacement_cm: f64,
        epsilon_cm: f64,
    },

    #[error(
        "the mesh is not the neutral Genesis 2 base: RMS deviation {rms_cm:.3} cm (limit {rms_limit_cm} cm), maximum {max_cm:.3} cm at vertex {max_vertex_index} (limit {max_limit_cm} cm); it likely carries a baked character look"
    )]
    NotNeutralBasis {
        rms_cm: f64,
        max_cm: f64,
        max_vertex_index: usize,
        rms_limit_cm: f64,
        max_limit_cm: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeutralityReceipt {
    pub rms_cm: f64,
    pub max_cm: f64,
    pub max_vertex_index: usize,
}

pub fn validate_neutral_shared_prefix(
    candidate: &OrderedObjMesh,
    neutral: &OrderedObjMesh,
) -> GeometryResult<NeutralityReceipt> {
    if neutral.vertices.len() < VAM_SHARED_VERTEX_COUNT
        || candidate.vertices.len() < VAM_SHARED_VERTEX_COUNT
    {
        return Err(VaMGeometryError::InvalidVamBasis(format!(
            "neutrality validation requires at least {VAM_SHARED_VERTEX_COUNT} vertices on both meshes"
        )));
    }
    let mut squared_sum = 0.0_f64;
    let mut max_squared = 0.0_f64;
    let mut max_vertex_index = 0_usize;
    for (index, (candidate_point, neutral_point)) in candidate
        .vertices
        .iter()
        .zip(&neutral.vertices)
        .take(VAM_SHARED_VERTEX_COUNT)
        .enumerate()
    {
        let mut squared = 0.0;
        for axis in 0..3 {
            let difference = (candidate_point[axis] - neutral_point[axis]) * VAM_METRES_TO_DAZ_CM;
            squared += difference * difference;
        }
        squared_sum += squared;
        if squared > max_squared {
            max_squared = squared;
            max_vertex_index = index;
        }
    }
    let receipt = NeutralityReceipt {
        rms_cm: (squared_sum / VAM_SHARED_VERTEX_COUNT as f64).sqrt(),
        max_cm: max_squared.sqrt(),
        max_vertex_index,
    };
    if !receipt.rms_cm.is_finite()
        || !receipt.max_cm.is_finite()
        || receipt.rms_cm > NEUTRAL_BASIS_RMS_LIMIT_CM
        || receipt.max_cm > NEUTRAL_BASIS_MAX_LIMIT_CM
    {
        return Err(VaMGeometryError::NotNeutralBasis {
            rms_cm: receipt.rms_cm,
            max_cm: receipt.max_cm,
            max_vertex_index: receipt.max_vertex_index,
            rms_limit_cm: NEUTRAL_BASIS_RMS_LIMIT_CM,
            max_limit_cm: NEUTRAL_BASIS_MAX_LIMIT_CM,
        });
    }
    Ok(receipt)
}

pub type GeometryResult<T> = std::result::Result<T, VaMGeometryError>;

#[derive(Clone, Debug)]
pub struct VaMGeometryProvider {
    sex: GeometrySex,
    daz_anchor: DazGeometry,
    vam_basis: OrderedObjMesh,
    protected_shared_vertices: Vec<bool>,
    receipt: VaMGeometrySidecarReceipt,
}

impl VaMGeometryProvider {
    pub fn from_user_owned_pair(
        sex: GeometrySex,
        daz_anchor: DazGeometry,
        vam_basis: OrderedObjMesh,
    ) -> GeometryResult<Self> {
        Self::build_with_contract(
            sex,
            daz_anchor,
            vam_basis,
            GeometryFamily::DazGenesis2,
            ProviderContract::production(),
        )
    }

    pub fn from_vam_anchored_pair(
        sex: GeometrySex,
        vam_anchor: DazGeometry,
        vam_basis: OrderedObjMesh,
    ) -> GeometryResult<Self> {
        Self::build_with_contract(
            sex,
            vam_anchor,
            vam_basis,
            GeometryFamily::VirtAMate,
            ProviderContract::production(),
        )
    }

    pub fn from_vam_basis(sex: GeometrySex, vam_basis: OrderedObjMesh) -> GeometryResult<Self> {
        vam_basis
            .validate()
            .map_err(|error| VaMGeometryError::InvalidVamBasis(error.to_string()))?;
        let contract = ProviderContract::production();
        let expected = contract.profile(sex);
        if vam_basis.vertices.len() != expected.vertex_count
            || vam_basis.faces.len() != expected.face_count
        {
            return Err(VaMGeometryError::WrongSexOrCounts {
                sex,
                expected_vertices: expected.vertex_count,
                expected_faces: expected.face_count,
                actual_vertices: vam_basis.vertices.len(),
                actual_faces: vam_basis.faces.len(),
            });
        }
        require_vam_face_materials(&vam_basis)?;
        require_used_vam_materials(
            &vam_basis,
            vam_basis.faces.iter(),
            contract.required_common_materials,
            GeometryFamily::VirtAMate,
            MaterialMatchPolicy::CanonicalVamSuffix,
        )?;
        require_used_vam_materials(
            &vam_basis,
            vam_basis.faces.iter(),
            expected.required_graft_materials,
            GeometryFamily::VirtAMate,
            MaterialMatchPolicy::ExactCaseInsensitive,
        )?;

        let (daz_anchor, protected_shared_vertices, appended_face_count) =
            derive_vam_working_anchor(&vam_basis, expected.required_graft_materials)?;
        validate_coordinate_space(
            &daz_anchor.vertices,
            &vam_basis.vertices[..contract.shared_vertex_count],
            &protected_shared_vertices,
        )?;
        let daz_topology = topology_fingerprint_daz(&daz_anchor)?;
        let vam_topology = topology_fingerprint_obj(&vam_basis);
        let expected_topology = match sex {
            GeometrySex::Female => VAM_FEMALE_TOPOLOGY_SHA256,
            GeometrySex::Male => VAM_MALE_TOPOLOGY_SHA256,
        };
        if vam_topology.sha256 != expected_topology {
            return Err(VaMGeometryError::TopologyMismatch(
                "ordered VaM base topology is not the supported built-in Genesis 2 stream"
                    .to_owned(),
            ));
        }
        let canonical_triangle_count = daz_topology.triangle_count;
        let receipt = VaMGeometrySidecarReceipt {
            schema_version: SIDECAR_SCHEMA_VERSION,
            anchor_family: GeometryFamily::VirtAMate,
            basis_family: GeometryFamily::VirtAMate,
            basis_sex: sex,
            shared_vertex_count: contract.shared_vertex_count,
            daz_topology,
            vam_topology,
            daz_vertices_sha256: vertex_fingerprint(&daz_anchor.vertices),
            vam_basis_vertices_sha256: vertex_fingerprint(&vam_basis.vertices),
            vam_face_labels_sha256: label_fingerprint(&vam_basis.faces),
            canonical_triangle_count,
            shared_canonical_triangle_count: canonical_triangle_count,
            replaced_canonical_triangle_count: 0,
            replacement_shared_triangle_count: 0,
            appended_face_count,
            protected_shared_vertex_count: protected_shared_vertices
                .iter()
                .filter(|&&protected| protected)
                .count(),
            metres_to_centimetres: VAM_METRES_TO_DAZ_CM,
            protected_epsilon_cm: DEFAULT_PROTECTED_EPSILON_CM,
        };
        Ok(Self {
            sex,
            daz_anchor,
            vam_basis,
            protected_shared_vertices,
            receipt,
        })
    }

    pub fn from_neutral_base(sex: GeometrySex, base: OrderedObjMesh) -> GeometryResult<Self> {
        base.validate()
            .map_err(|error| VaMGeometryError::InvalidVamBasis(error.to_string()))?;
        if base.vertices.len() != VAM_SHARED_VERTEX_COUNT || base.faces.len() != G2F_POLYGON_COUNT {
            return Err(VaMGeometryError::InvalidVamBasis(format!(
                "neutral base expects {VAM_SHARED_VERTEX_COUNT} vertices and {G2F_POLYGON_COUNT} polygons, got {} and {}",
                base.vertices.len(),
                base.faces.len()
            )));
        }
        require_vam_face_materials(&base)?;
        require_used_vam_materials(
            &base,
            base.faces.iter(),
            REQUIRED_COMMON_MATERIALS,
            GeometryFamily::VirtAMate,
            MaterialMatchPolicy::CanonicalVamSuffix,
        )?;
        let polygon_streams = base
            .faces
            .iter()
            .map(|face| face.vertex_indices.clone())
            .collect::<Vec<_>>();
        let quad_digest = topology_digest(base.vertices.len(), &polygon_streams)
            .map_err(|error| VaMGeometryError::InvalidVamBasis(error.to_string()))?;
        if quad_digest != G2F_TOPOLOGY_SHA256 {
            return Err(VaMGeometryError::TopologyMismatch(
                "the neutral base polygon stream is not the canonical Genesis 2 quad topology"
                    .to_owned(),
            ));
        }

        let mut polygon_group_indices = Vec::with_capacity(base.faces.len());
        let mut material_group_indices = Vec::with_capacity(base.faces.len());
        let mut polygon_groups = Vec::<String>::new();
        let mut material_groups = Vec::<String>::new();
        let mut polygon_group_lookup = HashMap::<String, u32>::new();
        let mut material_group_lookup = HashMap::<String, u32>::new();
        for (face_index, face) in base.faces.iter().enumerate() {
            let material = face
                .material
                .as_deref()
                .ok_or(VaMGeometryError::MissingFaceMaterial { face_index })?;
            let material = canonical_vam_anchor_label(material);
            let group = canonical_vam_anchor_label(face.group.as_deref().unwrap_or(material));
            polygon_group_indices.push(intern_vam_anchor_label(
                group,
                &mut polygon_groups,
                &mut polygon_group_lookup,
            )?);
            material_group_indices.push(intern_vam_anchor_label(
                material,
                &mut material_groups,
                &mut material_group_lookup,
            )?);
        }
        let anchor_vertices = base
            .vertices
            .iter()
            .map(|point| point.map(|value| value * VAM_METRES_TO_DAZ_CM))
            .collect::<Vec<_>>();
        let mut anchor = DazGeometry::new(
            "Genesis2-VaM-neutral".to_owned(),
            anchor_vertices,
            polygon_streams,
            crate::formats::GroupTable {
                indices: polygon_group_indices,
                names: polygon_groups,
            },
            crate::formats::GroupTable {
                indices: material_group_indices,
                names: material_groups,
            },
            json!({
                "vkit_import": {
                    "source": "VaM-neutral-base",
                    "shared_vertex_order": "Genesis2"
                }
            }),
        )
        .map_err(|error| VaMGeometryError::InvalidDazAnchor(error.to_string()))?;

        ensure_canonical_head_groups(&mut anchor)
            .map_err(|error| VaMGeometryError::InvalidDazAnchor(error.to_string()))?;

        let protected_shared_vertices = vec![false; VAM_SHARED_VERTEX_COUNT];
        validate_coordinate_space(
            &anchor.vertices,
            &base.vertices[..VAM_SHARED_VERTEX_COUNT],
            &protected_shared_vertices,
        )?;
        let daz_topology = topology_fingerprint_daz(&anchor)?;
        let vam_topology = topology_fingerprint_obj(&base);
        let receipt = VaMGeometrySidecarReceipt {
            schema_version: SIDECAR_SCHEMA_VERSION,
            anchor_family: GeometryFamily::VirtAMate,
            basis_family: GeometryFamily::VirtAMate,
            basis_sex: sex,
            shared_vertex_count: VAM_SHARED_VERTEX_COUNT,
            daz_topology,
            vam_topology,
            daz_vertices_sha256: vertex_fingerprint(&anchor.vertices),
            vam_basis_vertices_sha256: vertex_fingerprint(&base.vertices),
            vam_face_labels_sha256: label_fingerprint(&base.faces),
            canonical_triangle_count: daz_topology.triangle_count,
            shared_canonical_triangle_count: daz_topology.triangle_count,
            replaced_canonical_triangle_count: 0,
            replacement_shared_triangle_count: 0,
            appended_face_count: 0,
            protected_shared_vertex_count: 0,
            metres_to_centimetres: VAM_METRES_TO_DAZ_CM,
            protected_epsilon_cm: DEFAULT_PROTECTED_EPSILON_CM,
        };
        Ok(Self {
            sex,
            daz_anchor: anchor,
            vam_basis: base,
            protected_shared_vertices,
            receipt,
        })
    }

    pub const fn sex(&self) -> GeometrySex {
        self.sex
    }

    pub fn daz_anchor(&self) -> &DazGeometry {
        &self.daz_anchor
    }

    pub fn vam_basis(&self) -> &OrderedObjMesh {
        &self.vam_basis
    }

    pub fn receipt(&self) -> &VaMGeometrySidecarReceipt {
        &self.receipt
    }

    pub fn protected_shared_vertices(&self) -> &[bool] {
        &self.protected_shared_vertices
    }

    pub fn uses_vam_derived_anchor(&self) -> bool {
        self.receipt.anchor_family == GeometryFamily::VirtAMate
    }

    pub fn validate_sidecar(&self, sidecar: &VaMGeometrySidecarReceipt) -> GeometryResult<()> {
        if sidecar != &self.receipt {
            return Err(VaMGeometryError::SidecarMismatch);
        }
        Ok(())
    }

    pub fn compose_vam_output(&self, target_daz: &DazGeometry) -> GeometryResult<OrderedObjMesh> {
        target_daz
            .validate()
            .map_err(|error| VaMGeometryError::InvalidDazAnchor(error.to_string()))?;
        let target_topology = topology_fingerprint_daz(target_daz)?;
        if target_topology != self.receipt.daz_topology {
            return Err(VaMGeometryError::TargetTopologyMismatch);
        }

        let mut output = self.vam_basis.clone();
        for (vertex_index, output_vertex) in output
            .vertices
            .iter_mut()
            .take(self.receipt.shared_vertex_count)
            .enumerate()
        {
            if self.protected_shared_vertices[vertex_index] {
                continue;
            }
            for (axis, output_component) in output_vertex.iter_mut().enumerate() {
                let basis_cm = self.vam_basis.vertices[vertex_index][axis]
                    * self.receipt.metres_to_centimetres;
                let delta_cm = target_daz.vertices[vertex_index][axis]
                    - self.daz_anchor.vertices[vertex_index][axis];
                *output_component = (basis_cm + delta_cm) / self.receipt.metres_to_centimetres;
            }
        }
        Ok(output)
    }

    pub fn canonical_proxy_from_vam(
        &self,
        source_vam: &OrderedObjMesh,
    ) -> GeometryResult<DazGeometry> {
        source_vam
            .validate()
            .map_err(|error| VaMGeometryError::InvalidVamBasis(error.to_string()))?;
        if source_vam.faces != self.vam_basis.faces
            || topology_fingerprint_obj(source_vam) != self.receipt.vam_topology
        {
            return Err(VaMGeometryError::SourceTopologyMismatch);
        }

        let epsilon_cm = self.receipt.protected_epsilon_cm;
        for vertex_index in 0..source_vam.vertices.len() {
            let region = if vertex_index >= self.receipt.shared_vertex_count {
                Some(ProtectedRegion::AppendedGeometry)
            } else if self.protected_shared_vertices[vertex_index] {
                Some(ProtectedRegion::SharedGraftSeam)
            } else {
                None
            };
            let Some(region) = region else {
                continue;
            };
            let displacement_cm = distance(
                source_vam.vertices[vertex_index],
                self.vam_basis.vertices[vertex_index],
            ) * self.receipt.metres_to_centimetres;
            if displacement_cm > epsilon_cm {
                return Err(VaMGeometryError::ProtectedRegionChanged {
                    region,
                    vertex_index,
                    displacement_cm,
                    epsilon_cm,
                });
            }
        }

        let mut proxy = self.daz_anchor.clone();
        for (vertex_index, proxy_vertex) in proxy.vertices.iter_mut().enumerate() {
            if self.protected_shared_vertices[vertex_index] {
                continue;
            }
            for (axis, proxy_component) in proxy_vertex.iter_mut().enumerate() {
                *proxy_component = self.daz_anchor.vertices[vertex_index][axis]
                    + (source_vam.vertices[vertex_index][axis]
                        - self.vam_basis.vertices[vertex_index][axis])
                        * self.receipt.metres_to_centimetres;
            }
        }
        Ok(proxy)
    }

    fn build_with_contract(
        sex: GeometrySex,
        daz_anchor: DazGeometry,
        vam_basis: OrderedObjMesh,
        anchor_family: GeometryFamily,
        contract: ProviderContract,
    ) -> GeometryResult<Self> {
        daz_anchor
            .validate()
            .map_err(|error| VaMGeometryError::InvalidDazAnchor(error.to_string()))?;
        vam_basis
            .validate()
            .map_err(|error| VaMGeometryError::InvalidVamBasis(error.to_string()))?;

        if daz_anchor.vertices.len() != contract.shared_vertex_count
            || daz_anchor.faces.len() != contract.daz_polygon_count
        {
            return Err(VaMGeometryError::InvalidDazAnchor(format!(
                "expected {} vertices and {} polygons, got {} vertices and {} polygons",
                contract.shared_vertex_count,
                contract.daz_polygon_count,
                daz_anchor.vertices.len(),
                daz_anchor.faces.len()
            )));
        }
        let daz_digest = topology_digest(daz_anchor.vertices.len(), &daz_anchor.faces)
            .map_err(|error| VaMGeometryError::InvalidDazAnchor(error.to_string()))?;
        if daz_digest != contract.daz_topology_sha256 {
            return Err(VaMGeometryError::InvalidDazAnchor(
                "ordered polygon topology differs from canonical G2F".to_owned(),
            ));
        }

        let expected = contract.profile(sex);
        if vam_basis.vertices.len() != expected.vertex_count
            || vam_basis.faces.len() != expected.face_count
        {
            return Err(VaMGeometryError::WrongSexOrCounts {
                sex,
                expected_vertices: expected.vertex_count,
                expected_faces: expected.face_count,
                actual_vertices: vam_basis.vertices.len(),
                actual_faces: vam_basis.faces.len(),
            });
        }

        require_used_daz_materials(&daz_anchor, contract.required_common_materials)?;
        require_vam_face_materials(&vam_basis)?;
        require_used_vam_materials(
            &vam_basis,
            vam_basis.faces.iter(),
            contract.required_common_materials,
            GeometryFamily::VirtAMate,
            MaterialMatchPolicy::CanonicalVamSuffix,
        )?;

        let canonical_triangles = triangulate_daz(&daz_anchor)?;
        let analysis = analyze_topology(
            &canonical_triangles,
            &vam_basis,
            contract.shared_vertex_count,
            contract.minimum_overlap_numerator,
            contract.minimum_overlap_denominator,
            contract.maximum_protected_numerator,
            contract.maximum_protected_denominator,
        )?;

        let graft_faces = analysis.graft_face_indices.iter().map(|&index| {
            vam_basis
                .faces
                .get(index)
                .expect("analysis only stores validated face indices")
        });
        require_used_vam_materials(
            &vam_basis,
            graft_faces,
            contract.profile(sex).required_graft_materials,
            GeometryFamily::VirtAMate,
            MaterialMatchPolicy::ExactCaseInsensitive,
        )?;

        validate_coordinate_space(
            &daz_anchor.vertices,
            &vam_basis.vertices[..contract.shared_vertex_count],
            &analysis.protected_shared_vertices,
        )?;

        let daz_topology = topology_fingerprint_daz(&daz_anchor)?;
        let vam_topology = topology_fingerprint_obj(&vam_basis);
        let receipt = VaMGeometrySidecarReceipt {
            schema_version: SIDECAR_SCHEMA_VERSION,
            anchor_family,
            basis_family: GeometryFamily::VirtAMate,
            basis_sex: sex,
            shared_vertex_count: contract.shared_vertex_count,
            daz_topology,
            vam_topology,
            daz_vertices_sha256: vertex_fingerprint(&daz_anchor.vertices),
            vam_basis_vertices_sha256: vertex_fingerprint(&vam_basis.vertices),
            vam_face_labels_sha256: label_fingerprint(&vam_basis.faces),
            canonical_triangle_count: canonical_triangles.len(),
            shared_canonical_triangle_count: analysis.shared_canonical_triangle_count,
            replaced_canonical_triangle_count: analysis.replaced_canonical_triangle_count,
            replacement_shared_triangle_count: analysis.replacement_shared_triangle_count,
            appended_face_count: analysis.appended_face_count,
            protected_shared_vertex_count: analysis
                .protected_shared_vertices
                .iter()
                .filter(|&&protected| protected)
                .count(),
            metres_to_centimetres: VAM_METRES_TO_DAZ_CM,
            protected_epsilon_cm: DEFAULT_PROTECTED_EPSILON_CM,
        };

        Ok(Self {
            sex,
            daz_anchor,
            vam_basis,
            protected_shared_vertices: analysis.protected_shared_vertices,
            receipt,
        })
    }
}

#[derive(Clone, Copy)]
struct ExpectedProfile {
    vertex_count: usize,
    face_count: usize,
    required_graft_materials: &'static [&'static str],
}

#[derive(Clone, Copy)]
struct ProviderContract {
    shared_vertex_count: usize,
    daz_polygon_count: usize,
    daz_topology_sha256: [u8; 32],
    female: ExpectedProfile,
    male: ExpectedProfile,
    required_common_materials: &'static [&'static str],
    minimum_overlap_numerator: usize,
    minimum_overlap_denominator: usize,
    maximum_protected_numerator: usize,
    maximum_protected_denominator: usize,
}

impl ProviderContract {
    const fn production() -> Self {
        Self {
            shared_vertex_count: VAM_SHARED_VERTEX_COUNT,
            daz_polygon_count: G2F_POLYGON_COUNT,
            daz_topology_sha256: G2F_TOPOLOGY_SHA256,
            female: ExpectedProfile {
                vertex_count: VAM_FEMALE_VERTEX_COUNT,
                face_count: VAM_FEMALE_FACE_COUNT,
                required_graft_materials: REQUIRED_FEMALE_GRAFT_MATERIALS,
            },
            male: ExpectedProfile {
                vertex_count: VAM_MALE_VERTEX_COUNT,
                face_count: VAM_MALE_FACE_COUNT,
                required_graft_materials: REQUIRED_MALE_GRAFT_MATERIALS,
            },
            required_common_materials: REQUIRED_COMMON_MATERIALS,
            minimum_overlap_numerator: 85,
            minimum_overlap_denominator: 100,
            maximum_protected_numerator: 35,
            maximum_protected_denominator: 100,
        }
    }

    const fn profile(self, sex: GeometrySex) -> ExpectedProfile {
        match sex {
            GeometrySex::Female => self.female,
            GeometrySex::Male => self.male,
        }
    }
}

struct TopologyAnalysis {
    protected_shared_vertices: Vec<bool>,
    graft_face_indices: Vec<usize>,
    shared_canonical_triangle_count: usize,
    replaced_canonical_triangle_count: usize,
    replacement_shared_triangle_count: usize,
    appended_face_count: usize,
}

fn derive_vam_working_anchor(
    vam_basis: &OrderedObjMesh,
    graft_materials: &[&str],
) -> GeometryResult<(DazGeometry, Vec<bool>, usize)> {
    let mut faces = Vec::new();
    let mut polygon_group_indices = Vec::new();
    let mut material_group_indices = Vec::new();
    let mut polygon_groups = Vec::<String>::new();
    let mut material_groups = Vec::<String>::new();
    let mut polygon_group_lookup = HashMap::<String, u32>::new();
    let mut material_group_lookup = HashMap::<String, u32>::new();
    let mut protected = vec![false; VAM_SHARED_VERTEX_COUNT];
    let mut appended_face_count = 0usize;

    for (face_index, face) in vam_basis.faces.iter().enumerate() {
        if face.vertex_indices.len() != 3 {
            return Err(VaMGeometryError::TopologyMismatch(format!(
                "VaM face {face_index} has {} corners; expected ordered triangles",
                face.vertex_indices.len()
            )));
        }
        let material = face
            .material
            .as_deref()
            .ok_or(VaMGeometryError::MissingFaceMaterial { face_index })?;
        let touches_appended = face
            .vertex_indices
            .iter()
            .any(|&vertex| vertex as usize >= VAM_SHARED_VERTEX_COUNT);
        let graft_material = graft_materials
            .iter()
            .any(|required| MaterialMatchPolicy::ExactCaseInsensitive.matches(material, required));
        if touches_appended {
            appended_face_count += 1;
        }
        if touches_appended || graft_material {
            for &vertex in &face.vertex_indices {
                if (vertex as usize) < VAM_SHARED_VERTEX_COUNT {
                    protected[vertex as usize] = true;
                }
            }
        }
        if touches_appended {
            continue;
        }

        let material = canonical_vam_anchor_label(material);
        let group = canonical_vam_anchor_label(face.group.as_deref().unwrap_or(material));
        let polygon_group =
            intern_vam_anchor_label(group, &mut polygon_groups, &mut polygon_group_lookup)?;
        let material_group =
            intern_vam_anchor_label(material, &mut material_groups, &mut material_group_lookup)?;
        faces.push(face.vertex_indices.clone());
        polygon_group_indices.push(polygon_group);
        material_group_indices.push(material_group);
    }
    if faces.is_empty() {
        return Err(VaMGeometryError::TopologyMismatch(
            "VaM basis has no faces wholly inside the shared Genesis 2 vertex prefix".to_owned(),
        ));
    }
    let vertices = vam_basis.vertices[..VAM_SHARED_VERTEX_COUNT]
        .iter()
        .map(|point| point.map(|value| value * VAM_METRES_TO_DAZ_CM))
        .collect();
    let mut anchor = DazGeometry::new(
        "Genesis2-VaM-runtime".to_owned(),
        vertices,
        faces,
        crate::formats::GroupTable {
            indices: polygon_group_indices,
            names: polygon_groups,
        },
        crate::formats::GroupTable {
            indices: material_group_indices,
            names: material_groups,
        },
        json!({
            "vkit_import": {
                "source": "VaM-runtime-base",
                "shared_vertex_order": "Genesis2"
            }
        }),
    )
    .map_err(|error| VaMGeometryError::InvalidDazAnchor(error.to_string()))?;

    let _ = ensure_canonical_head_groups(&mut anchor);
    Ok((anchor, protected, appended_face_count))
}

fn canonical_vam_anchor_label(label: &str) -> &str {
    HEAD_VISUAL_MATERIALS
        .iter()
        .copied()
        .find(|candidate| MaterialMatchPolicy::CanonicalVamSuffix.matches(label, candidate))
        .unwrap_or(label)
}

fn intern_vam_anchor_label(
    label: &str,
    labels: &mut Vec<String>,
    lookup: &mut HashMap<String, u32>,
) -> GeometryResult<u32> {
    if let Some(&index) = lookup.get(label) {
        return Ok(index);
    }
    let index = u32::try_from(labels.len()).map_err(|_| {
        VaMGeometryError::TopologyMismatch("VaM label table exceeds u32".to_owned())
    })?;
    let owned = label.to_owned();
    labels.push(owned.clone());
    lookup.insert(owned, index);
    Ok(index)
}

fn analyze_topology(
    canonical_triangles: &[[u32; 3]],
    vam_basis: &OrderedObjMesh,
    shared_vertex_count: usize,
    minimum_overlap_numerator: usize,
    minimum_overlap_denominator: usize,
    maximum_protected_numerator: usize,
    maximum_protected_denominator: usize,
) -> GeometryResult<TopologyAnalysis> {
    let mut canonical_lookup = HashMap::<[u32; 3], usize>::new();
    for (index, &triangle) in canonical_triangles.iter().enumerate() {
        let key = oriented_triangle_key(triangle);
        if canonical_lookup.insert(key, index).is_some() {
            return Err(VaMGeometryError::TopologyMismatch(
                "canonical DAZ triangulation contains a duplicate oriented triangle".to_owned(),
            ));
        }
    }

    let mut referenced_vertices = vec![false; vam_basis.vertices.len()];
    let mut matched_canonical = vec![false; canonical_triangles.len()];
    let mut replacement_faces = Vec::<(usize, [u32; 3])>::new();
    let mut appended_faces = Vec::<usize>::new();
    let mut appended_seam_vertices = vec![false; shared_vertex_count];
    let mut seen_vam_triangles = HashSet::<[u32; 3]>::new();

    for (face_index, face) in vam_basis.faces.iter().enumerate() {
        if face.vertex_indices.len() != 3 {
            return Err(VaMGeometryError::TopologyMismatch(format!(
                "VaM face {face_index} has {} corners; the full-mesh provider requires ordered triangles",
                face.vertex_indices.len()
            )));
        }
        let triangle = [
            face.vertex_indices[0],
            face.vertex_indices[1],
            face.vertex_indices[2],
        ];
        for &vertex in &triangle {
            referenced_vertices[vertex as usize] = true;
        }
        let key = oriented_triangle_key(triangle);
        if !seen_vam_triangles.insert(key) {
            return Err(VaMGeometryError::TopologyMismatch(format!(
                "VaM face {face_index} duplicates an earlier oriented triangle"
            )));
        }

        if triangle
            .iter()
            .any(|&vertex| vertex as usize >= shared_vertex_count)
        {
            appended_faces.push(face_index);
            for &vertex in &triangle {
                if (vertex as usize) < shared_vertex_count {
                    appended_seam_vertices[vertex as usize] = true;
                }
            }
            continue;
        }

        if let Some(&canonical_index) = canonical_lookup.get(&key) {
            if matched_canonical[canonical_index] {
                return Err(VaMGeometryError::TopologyMismatch(format!(
                    "canonical triangle {canonical_index} appears more than once in VaM"
                )));
            }
            matched_canonical[canonical_index] = true;
        } else {
            replacement_faces.push((face_index, triangle));
        }
    }

    let shared_canonical_triangle_count =
        matched_canonical.iter().filter(|&&matched| matched).count();
    if shared_canonical_triangle_count * minimum_overlap_denominator
        < canonical_triangles.len() * minimum_overlap_numerator
    {
        return Err(VaMGeometryError::TopologyMismatch(format!(
            "only {shared_canonical_triangle_count} of {} canonical triangles overlap the VaM body",
            canonical_triangles.len()
        )));
    }

    let missing_triangle_indices = matched_canonical
        .iter()
        .enumerate()
        .filter_map(|(index, &matched)| (!matched).then_some(index))
        .collect::<Vec<_>>();
    let mut protected_shared_vertices = appended_seam_vertices.clone();
    for vertex_index in 0..shared_vertex_count {
        if !referenced_vertices[vertex_index] {
            protected_shared_vertices[vertex_index] = true;
        }
    }
    for &triangle_index in &missing_triangle_indices {
        for &vertex in &canonical_triangles[triangle_index] {
            protected_shared_vertices[vertex as usize] = true;
        }
    }
    if protected_shared_vertices
        .iter()
        .filter(|&&protected| protected)
        .count()
        * maximum_protected_denominator
        > shared_vertex_count * maximum_protected_numerator
    {
        return Err(VaMGeometryError::TopologyMismatch(
            "the inferred graft seam protects an implausibly large share of the canonical body"
                .to_owned(),
        ));
    }

    for &(face_index, triangle) in &replacement_faces {
        if triangle
            .iter()
            .any(|&vertex| !protected_shared_vertices[vertex as usize])
        {
            return Err(VaMGeometryError::TopologyMismatch(format!(
                "replacement shared face {face_index} escapes the inferred graft boundary"
            )));
        }
    }
    validate_missing_region_connectivity(
        canonical_triangles,
        &missing_triangle_indices,
        &appended_seam_vertices,
    )?;

    let mut graft_face_indices = appended_faces.clone();
    graft_face_indices.extend(replacement_faces.iter().map(|(face_index, _)| *face_index));
    graft_face_indices.sort_unstable();

    Ok(TopologyAnalysis {
        protected_shared_vertices,
        graft_face_indices,
        shared_canonical_triangle_count,
        replaced_canonical_triangle_count: missing_triangle_indices.len(),
        replacement_shared_triangle_count: replacement_faces.len(),
        appended_face_count: appended_faces.len(),
    })
}

fn validate_missing_region_connectivity(
    canonical_triangles: &[[u32; 3]],
    missing_triangle_indices: &[usize],
    appended_seam_vertices: &[bool],
) -> GeometryResult<()> {
    if missing_triangle_indices.is_empty() {
        return Ok(());
    }
    let mut vertex_to_missing = HashMap::<u32, Vec<usize>>::new();
    for (local_index, &triangle_index) in missing_triangle_indices.iter().enumerate() {
        for &vertex in &canonical_triangles[triangle_index] {
            vertex_to_missing
                .entry(vertex)
                .or_default()
                .push(local_index);
        }
    }

    let mut visited = vec![false; missing_triangle_indices.len()];
    let mut queue = VecDeque::new();
    for (local_index, &triangle_index) in missing_triangle_indices.iter().enumerate() {
        if canonical_triangles[triangle_index]
            .iter()
            .any(|&vertex| appended_seam_vertices[vertex as usize])
        {
            visited[local_index] = true;
            queue.push_back(local_index);
        }
    }
    if queue.is_empty() {
        return Err(VaMGeometryError::DisconnectedGraftRegion);
    }
    while let Some(local_index) = queue.pop_front() {
        let triangle_index = missing_triangle_indices[local_index];
        for &vertex in &canonical_triangles[triangle_index] {
            if let Some(neighbours) = vertex_to_missing.get(&vertex) {
                for &neighbour in neighbours {
                    if !visited[neighbour] {
                        visited[neighbour] = true;
                        queue.push_back(neighbour);
                    }
                }
            }
        }
    }
    if visited.iter().any(|&value| !value) {
        return Err(VaMGeometryError::DisconnectedGraftRegion);
    }
    Ok(())
}

fn triangulate_daz(geometry: &DazGeometry) -> GeometryResult<Vec<[u32; 3]>> {
    let mut triangles = Vec::new();
    for (face_index, face) in geometry.faces.iter().enumerate() {
        if face.len() < 3 {
            return Err(VaMGeometryError::InvalidDazAnchor(format!(
                "face {face_index} has fewer than three vertices"
            )));
        }
        for corner in 1..face.len() - 1 {
            triangles.push([face[0], face[corner], face[corner + 1]]);
        }
    }
    Ok(triangles)
}

fn topology_fingerprint_daz(geometry: &DazGeometry) -> GeometryResult<TopologyFingerprint> {
    Ok(topology_fingerprint(
        geometry.vertices.len(),
        geometry.faces.iter().map(Vec::as_slice),
    ))
}

fn topology_fingerprint_obj(mesh: &OrderedObjMesh) -> TopologyFingerprint {
    topology_fingerprint(
        mesh.vertices.len(),
        mesh.faces.iter().map(|face| face.vertex_indices.as_slice()),
    )
}

fn topology_fingerprint<'a>(
    vertex_count: usize,
    faces: impl ExactSizeIterator<Item = &'a [u32]>,
) -> TopologyFingerprint {
    let polygon_count = faces.len();
    let mut triangle_count = 0usize;
    let mut digest = Sha256::new();
    digest.update(TOPOLOGY_FINGERPRINT_PREFIX);
    digest.update((vertex_count as u64).to_le_bytes());
    digest.update((polygon_count as u64).to_le_bytes());
    for face in faces {
        digest.update((face.len() as u64).to_le_bytes());
        triangle_count += face.len().saturating_sub(2);
        for &index in face {
            digest.update(index.to_le_bytes());
        }
    }
    TopologyFingerprint {
        vertex_count,
        polygon_count,
        triangle_count,
        sha256: digest.finalize().into(),
    }
}

pub(crate) fn vertex_fingerprint(vertices: &[[f64; 3]]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(VERTEX_FINGERPRINT_PREFIX);
    digest.update((vertices.len() as u64).to_le_bytes());
    for point in vertices {
        for &coordinate in point {
            let normalized = if coordinate == 0.0 { 0.0 } else { coordinate };
            digest.update(normalized.to_bits().to_le_bytes());
        }
    }
    digest.finalize().into()
}

fn label_fingerprint(faces: &[ObjFace]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(LABEL_FINGERPRINT_PREFIX);
    digest.update((faces.len() as u64).to_le_bytes());
    for face in faces {
        update_optional_string(&mut digest, face.group.as_deref());
        update_optional_string(&mut digest, face.material.as_deref());
    }
    digest.finalize().into()
}

fn update_optional_string(digest: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update((value.len() as u64).to_le_bytes());
            digest.update(value.as_bytes());
        }
        None => digest.update([0]),
    }
}

fn oriented_triangle_key([a, b, c]: [u32; 3]) -> [u32; 3] {
    let rotations = [[a, b, c], [b, c, a], [c, a, b]];
    *rotations.iter().min().expect("three rotations")
}

fn require_used_daz_materials(geometry: &DazGeometry, required: &[&str]) -> GeometryResult<()> {
    let used = geometry
        .material_group_indices
        .iter()
        .filter_map(|&index| geometry.material_groups.get(index as usize))
        .collect::<Vec<_>>();
    for &material in required {
        if !used
            .iter()
            .any(|actual| actual.eq_ignore_ascii_case(material))
        {
            return Err(VaMGeometryError::MissingRequiredMaterial {
                family: GeometryFamily::DazGenesis2,
                material: material.to_owned(),
            });
        }
    }
    Ok(())
}

fn require_vam_face_materials(mesh: &OrderedObjMesh) -> GeometryResult<()> {
    if let Some((face_index, _)) = mesh
        .faces
        .iter()
        .enumerate()
        .find(|(_, face)| face.material.as_deref().is_none_or(str::is_empty))
    {
        return Err(VaMGeometryError::MissingFaceMaterial { face_index });
    }
    Ok(())
}

fn require_used_vam_materials<'a>(
    _mesh: &OrderedObjMesh,
    faces: impl IntoIterator<Item = &'a ObjFace>,
    required: &[&str],
    family: GeometryFamily,
    match_policy: MaterialMatchPolicy,
) -> GeometryResult<()> {
    let used = faces
        .into_iter()
        .filter_map(|face| face.material.as_deref())
        .collect::<Vec<_>>();
    for &material in required {
        if !used
            .iter()
            .any(|actual| match_policy.matches(actual, material))
        {
            return Err(VaMGeometryError::MissingRequiredMaterial {
                family,
                material: material.to_owned(),
            });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MaterialMatchPolicy {
    ExactCaseInsensitive,
    CanonicalVamSuffix,
}

impl MaterialMatchPolicy {
    fn matches(self, actual: &str, required: &str) -> bool {
        if actual.eq_ignore_ascii_case(required) {
            return true;
        }
        if self != Self::CanonicalVamSuffix || actual.len() <= required.len() + 1 {
            return false;
        }
        let (prefix, suffix) = actual.split_at(required.len());
        prefix.eq_ignore_ascii_case(required)
            && suffix.strip_prefix('-').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
            })
    }
}

fn validate_coordinate_space(
    daz_cm: &[[f64; 3]],
    vam_m: &[[f64; 3]],
    protected: &[bool],
) -> GeometryResult<()> {
    let daz_bounds = Bounds::from_points(daz_cm)?;
    let vam_bounds = Bounds::from_points(vam_m)?;
    daz_bounds.require_y_up("DAZ anchor")?;
    vam_bounds.require_y_up("VaM basis")?;
    let daz_height = daz_bounds.extents()[1];
    let vam_height = vam_bounds.extents()[1];
    if !(MIN_DAZ_HEIGHT_CM..=MAX_DAZ_HEIGHT_CM).contains(&daz_height) {
        return Err(VaMGeometryError::CoordinateSpace(format!(
            "DAZ Y height {daz_height:.6} cm is outside {MIN_DAZ_HEIGHT_CM}..={MAX_DAZ_HEIGHT_CM} cm"
        )));
    }
    if !(MIN_VAM_HEIGHT_M..=MAX_VAM_HEIGHT_M).contains(&vam_height) {
        return Err(VaMGeometryError::CoordinateSpace(format!(
            "VaM Y height {vam_height:.6} m is outside {MIN_VAM_HEIGHT_M}..={MAX_VAM_HEIGHT_M} m"
        )));
    }
    let normalized_height_ratio = vam_height * VAM_METRES_TO_DAZ_CM / daz_height;
    if !(0.70..=1.40).contains(&normalized_height_ratio) {
        return Err(VaMGeometryError::CoordinateSpace(format!(
            "DAZ/VaM normalized height ratio {normalized_height_ratio:.6} is implausible"
        )));
    }

    let editable = protected
        .iter()
        .enumerate()
        .filter_map(|(index, &is_protected)| (!is_protected).then_some(index))
        .collect::<Vec<_>>();
    if editable.len() < 4 {
        return Err(VaMGeometryError::CoordinateSpace(
            "too few non-graft shared vertices remain for coordinate validation".to_owned(),
        ));
    }
    for axis in 0..3 {
        let daz_axis = editable.iter().map(|&index| daz_cm[index][axis]);
        let vam_axis = editable
            .iter()
            .map(|&index| vam_m[index][axis] * VAM_METRES_TO_DAZ_CM);
        let (correlation, scale_ratio) = correlation_and_scale(daz_axis, vam_axis)?;
        if correlation < MIN_AXIS_CORRELATION {
            return Err(VaMGeometryError::CoordinateSpace(format!(
                "axis {axis} DAZ/VaM correlation {correlation:.6} is below {MIN_AXIS_CORRELATION}"
            )));
        }
        if !(MIN_AXIS_SCALE_RATIO..=MAX_AXIS_SCALE_RATIO).contains(&scale_ratio) {
            return Err(VaMGeometryError::CoordinateSpace(format!(
                "axis {axis} DAZ/VaM spread ratio {scale_ratio:.6} is implausible"
            )));
        }
    }
    Ok(())
}

fn correlation_and_scale(
    left: impl Iterator<Item = f64>,
    right: impl Iterator<Item = f64>,
) -> GeometryResult<(f64, f64)> {
    let pairs = left.zip(right).collect::<Vec<_>>();
    let count = pairs.len() as f64;
    let left_mean = pairs.iter().map(|(value, _)| *value).sum::<f64>() / count;
    let right_mean = pairs.iter().map(|(_, value)| *value).sum::<f64>() / count;
    let mut covariance = 0.0;
    let mut left_variance = 0.0;
    let mut right_variance = 0.0;
    for &(left, right) in &pairs {
        let left_delta = left - left_mean;
        let right_delta = right - right_mean;
        covariance += left_delta * right_delta;
        left_variance += left_delta * left_delta;
        right_variance += right_delta * right_delta;
    }
    if left_variance <= f64::EPSILON || right_variance <= f64::EPSILON {
        return Err(VaMGeometryError::CoordinateSpace(
            "a shared coordinate axis has zero variance".to_owned(),
        ));
    }
    let correlation = covariance / (left_variance * right_variance).sqrt();
    let scale_ratio = (right_variance / left_variance).sqrt();
    Ok((correlation, scale_ratio))
}

#[derive(Clone, Copy)]
struct Bounds {
    minimum: [f64; 3],
    maximum: [f64; 3],
}

impl Bounds {
    fn from_points(points: &[[f64; 3]]) -> GeometryResult<Self> {
        let Some(first) = points.first().copied() else {
            return Err(VaMGeometryError::CoordinateSpace(
                "geometry has no vertices".to_owned(),
            ));
        };
        let mut bounds = Self {
            minimum: first,
            maximum: first,
        };
        for &point in points {
            if !point.iter().all(|value| value.is_finite()) {
                return Err(VaMGeometryError::CoordinateSpace(
                    "geometry contains a non-finite vertex".to_owned(),
                ));
            }
            for (axis, &component) in point.iter().enumerate() {
                bounds.minimum[axis] = bounds.minimum[axis].min(component);
                bounds.maximum[axis] = bounds.maximum[axis].max(component);
            }
        }
        Ok(bounds)
    }

    fn extents(self) -> [f64; 3] {
        [
            self.maximum[0] - self.minimum[0],
            self.maximum[1] - self.minimum[1],
            self.maximum[2] - self.minimum[2],
        ]
    }

    fn require_y_up(self, label: &str) -> GeometryResult<()> {
        let extents = self.extents();
        if extents[1] <= extents[0] * 1.05 || extents[1] <= extents[2] * 1.05 {
            return Err(VaMGeometryError::CoordinateSpace(format!(
                "{label} is not a plausibly Y-up full-body mesh; extents are {extents:?}"
            )));
        }
        Ok(())
    }
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn the_digest_domains_keep_their_original_spelling() {
        assert_eq!(TOPOLOGY_FINGERPRINT_PREFIX, b"vkit.vam.topology.v1\x00");
        assert_eq!(VERTEX_FINGERPRINT_PREFIX, b"vkit.vam.vertices.v1\x00");
        assert_eq!(LABEL_FINGERPRINT_PREFIX, b"vkit.vam.face-labels.v1\x00");
    }

    use super::*;

    const SHARED: usize = 8;
    const FEMALE_VERTICES: usize = 12;
    const FEMALE_FACES: usize = 10;
    const MALE_VERTICES: usize = 11;
    const MALE_FACES: usize = 11;

    fn anchor() -> DazGeometry {
        let vertices = vec![
            [-30.0, 0.0, -15.0],
            [30.0, 0.0, -14.0],
            [-25.0, 60.0, 10.0],
            [25.0, 62.0, 12.0],
            [-20.0, 120.0, -10.0],
            [20.0, 122.0, -8.0],
            [-10.0, 180.0, 1.0],
            [10.0, 179.0, 3.0],
        ];
        let faces = vec![
            vec![0, 1, 2],
            vec![1, 3, 2],
            vec![2, 3, 4],
            vec![3, 5, 4],
            vec![4, 5, 6],
            vec![5, 7, 6],
        ];
        let materials = REQUIRED_COMMON_MATERIALS
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        DazGeometry::new(
            "synthetic-anchor".to_owned(),
            vertices,
            faces,
            crate::formats::GroupTable {
                indices: vec![0; 6],
                names: vec!["body".to_owned()],
            },
            crate::formats::GroupTable {
                indices: (0..6)
                    .map(|index| (index % materials.len()) as u32)
                    .collect(),
                names: materials,
            },
            json!({}),
        )
        .unwrap()
    }

    fn vam_basis() -> OrderedObjMesh {
        let mut vertices = anchor()
            .vertices
            .iter()
            .map(|point| [point[0] / 100.0, point[1] / 100.0, point[2] / 100.0])
            .collect::<Vec<_>>();
        vertices.extend([
            [-0.12, 0.85, 0.08],
            [0.12, 0.86, 0.08],
            [-0.08, 0.95, 0.12],
            [0.08, 0.96, 0.12],
        ]);
        let indices = [
            [0, 1, 2],
            [1, 3, 2],
            [2, 3, 4],
            [3, 5, 4],
            [4, 8, 9],
            [5, 9, 10],
            [6, 10, 11],
            [7, 11, 8],
            [4, 5, 7],
            [4, 7, 6],
        ];
        let body_materials = ["Face", "Head", "Neck", "Lips"];
        let faces = indices
            .into_iter()
            .enumerate()
            .map(|(index, vertex_indices)| ObjFace {
                vertex_indices: vertex_indices.to_vec(),
                group: Some(if index < 4 { "body" } else { "graft" }.to_owned()),
                material: Some(if index < 4 {
                    body_materials[index].to_owned()
                } else if index == 9 {
                    "Hips".to_owned()
                } else {
                    "defaultMat-1".to_owned()
                }),
            })
            .collect();
        OrderedObjMesh { vertices, faces }
    }

    fn test_contract(anchor: &DazGeometry) -> ProviderContract {
        ProviderContract {
            shared_vertex_count: SHARED,
            daz_polygon_count: anchor.faces.len(),
            daz_topology_sha256: topology_digest(anchor.vertices.len(), &anchor.faces).unwrap(),
            female: ExpectedProfile {
                vertex_count: FEMALE_VERTICES,
                face_count: FEMALE_FACES,
                required_graft_materials: REQUIRED_FEMALE_GRAFT_MATERIALS,
            },
            male: ExpectedProfile {
                vertex_count: MALE_VERTICES,
                face_count: MALE_FACES,
                required_graft_materials: REQUIRED_MALE_GRAFT_MATERIALS,
            },
            required_common_materials: REQUIRED_COMMON_MATERIALS,
            minimum_overlap_numerator: 60,
            minimum_overlap_denominator: 100,
            maximum_protected_numerator: 75,
            maximum_protected_denominator: 100,
        }
    }

    fn provider() -> VaMGeometryProvider {
        let anchor = anchor();
        let contract = test_contract(&anchor);
        VaMGeometryProvider::build_with_contract(
            GeometrySex::Female,
            anchor,
            vam_basis(),
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap()
    }

    #[test]
    fn public_counts_are_shortlists_only() {
        assert_eq!(VAM_SHARED_VERTEX_COUNT, 21_556);
        assert_eq!(GeometrySex::Female.expected_counts(), (24_928, 44_474));
        assert_eq!(GeometrySex::Male.expected_counts(), (24_801, 44_500));
        assert_eq!(
            GeometrySex::shortlist_from_counts(24_928, 44_474),
            Some(GeometrySex::Female)
        );
        assert_eq!(GeometrySex::shortlist_from_counts(24_928, 1), None);
    }

    #[test]
    fn count_impostor_with_disconnected_topology_is_rejected() {
        let anchor = anchor();
        let contract = test_contract(&anchor);
        let mut impostor = vam_basis();
        impostor.faces[0].vertex_indices = vec![0, 2, 1];
        let error = VaMGeometryProvider::build_with_contract(
            GeometrySex::Female,
            anchor,
            impostor,
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            VaMGeometryError::DisconnectedGraftRegion | VaMGeometryError::TopologyMismatch(_)
        ));
    }

    #[test]
    fn no_op_roundtrip_preserves_both_bases_exactly() {
        let provider = provider();
        let output = provider.compose_vam_output(provider.daz_anchor()).unwrap();
        assert_eq!(output, *provider.vam_basis());
        let proxy = provider.canonical_proxy_from_vam(&output).unwrap();
        assert_eq!(proxy, *provider.daz_anchor());
        assert_eq!(provider.receipt().basis_sex, GeometrySex::Female);
        provider.validate_sidecar(provider.receipt()).unwrap();
    }

    #[test]
    fn editable_head_delta_transfers_in_both_directions() {
        let provider = provider();
        assert!(!provider.protected_shared_vertices()[1]);
        let mut target = provider.daz_anchor().clone();
        target.vertices[1][0] += 1.25;
        target.vertices[1][1] -= 0.5;
        target.vertices[1][2] += 0.75;

        let output = provider.compose_vam_output(&target).unwrap();
        assert!(
            (output.vertices[1][0] - (provider.vam_basis().vertices[1][0] + 0.0125)).abs()
                < 1.0e-12
        );
        assert_eq!(
            &output.vertices[SHARED..],
            &provider.vam_basis().vertices[SHARED..]
        );
        assert_eq!(output.faces, provider.vam_basis().faces);

        let recovered = provider.canonical_proxy_from_vam(&output).unwrap();
        assert_eq!(recovered.vertices, target.vertices);
        assert_eq!(recovered.faces, target.faces);
    }

    #[test]
    fn protected_seam_edit_is_discarded_during_graft_composition() {
        let provider = provider();
        assert!(provider.protected_shared_vertices()[4]);
        let mut target = provider.daz_anchor().clone();
        target.vertices[4][0] += 0.1;
        let output = provider.compose_vam_output(&target).unwrap();
        assert_eq!(output.vertices[4], provider.vam_basis().vertices[4]);
    }

    #[test]
    fn appended_vam_edit_is_rejected_by_proxy_conversion() {
        let provider = provider();
        let mut source = provider.vam_basis().clone();
        source.vertices[SHARED][1] += 0.01;
        let error = provider.canonical_proxy_from_vam(&source).unwrap_err();
        assert!(matches!(
            error,
            VaMGeometryError::ProtectedRegionChanged {
                region: ProtectedRegion::AppendedGeometry,
                vertex_index: SHARED,
                ..
            }
        ));
    }

    #[test]
    fn wrong_sex_count_is_rejected_before_topology_enrollment() {
        let anchor = anchor();
        let contract = test_contract(&anchor);
        let error = VaMGeometryProvider::build_with_contract(
            GeometrySex::Male,
            anchor,
            vam_basis(),
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            VaMGeometryError::WrongSexOrCounts {
                sex: GeometrySex::Male,
                actual_vertices: FEMALE_VERTICES,
                actual_faces: FEMALE_FACES,
                ..
            }
        ));
    }

    #[test]
    fn sidecar_detects_substitution_after_full_revalidation() {
        let provider = provider();
        let mut sidecar = provider.receipt().clone();
        sidecar.vam_basis_vertices_sha256[0] ^= 0xff;
        assert_eq!(
            provider.validate_sidecar(&sidecar),
            Err(VaMGeometryError::SidecarMismatch)
        );
    }

    #[test]
    fn common_vam_materials_accept_numeric_suffixes_but_graft_gates_stay_exact() {
        let anchor = anchor();
        let contract = test_contract(&anchor);
        let mut suffixed = vam_basis();
        for (face, material) in suffixed.faces[..4]
            .iter_mut()
            .zip(["fAcE-1", "HEAD-2", "neck-1", "Lips-9"])
        {
            face.material = Some(material.to_owned());
        }
        VaMGeometryProvider::build_with_contract(
            GeometrySex::Female,
            anchor.clone(),
            suffixed.clone(),
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap();

        for face in &mut suffixed.faces[4..] {
            if face.material.as_deref() == Some("defaultMat-1") {
                face.material = Some("defaultMat-1-1".to_owned());
            }
        }
        let error = VaMGeometryProvider::build_with_contract(
            GeometrySex::Female,
            anchor,
            suffixed,
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap_err();
        assert_eq!(
            error,
            VaMGeometryError::MissingRequiredMaterial {
                family: GeometryFamily::VirtAMate,
                material: "defaultMat-1".to_owned(),
            }
        );
    }

    #[test]
    fn runtime_anchor_canonicalizes_only_head_visual_material_suffixes() {
        assert_eq!(canonical_vam_anchor_label("Face-1"), "Face");
        assert_eq!(canonical_vam_anchor_label("sClErA-2"), "Sclera");
        assert_eq!(canonical_vam_anchor_label("Torso-1"), "Torso-1");
        assert_eq!(canonical_vam_anchor_label("defaultMat-1"), "defaultMat-1");
    }

    #[test]
    fn canonical_shared_triangles_may_use_vam_face_order() {
        let anchor = anchor();
        let contract = test_contract(&anchor);
        let mut reordered = vam_basis();
        reordered.faces.swap(0, 3);
        reordered.faces.swap(1, 2);

        let provider = VaMGeometryProvider::build_with_contract(
            GeometrySex::Female,
            anchor,
            reordered.clone(),
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap();
        assert_eq!(provider.vam_basis().faces, reordered.faces);
        assert_eq!(provider.receipt().shared_canonical_triangle_count, 4);
    }

    #[test]
    fn loose_shared_and_appended_positions_are_bound_and_protected() {
        let mut loose_anchor = anchor();
        loose_anchor.vertices.push([5.0, 90.0, 5.0]);
        loose_anchor.validate().unwrap();

        let mut loose_basis = vam_basis();
        loose_basis.vertices.insert(SHARED, [0.05, 0.9, 0.05]);
        for face in &mut loose_basis.faces {
            for index in &mut face.vertex_indices {
                if *index as usize >= SHARED {
                    *index += 1;
                }
            }
        }
        let appended_loose_index = loose_basis.vertices.len();
        loose_basis.vertices.push([0.33, 0.91, 0.14]);
        loose_basis.validate().unwrap();

        let mut contract = test_contract(&loose_anchor);
        contract.shared_vertex_count = SHARED + 1;
        contract.female.vertex_count = FEMALE_VERTICES + 2;
        let provider = VaMGeometryProvider::build_with_contract(
            GeometrySex::Female,
            loose_anchor,
            loose_basis.clone(),
            GeometryFamily::DazGenesis2,
            contract,
        )
        .unwrap();

        assert!(provider.protected_shared_vertices()[SHARED]);
        assert_eq!(
            provider.compose_vam_output(provider.daz_anchor()).unwrap(),
            loose_basis
        );

        let mut changed_shared = provider.daz_anchor().clone();
        changed_shared.vertices[SHARED][0] += 0.1;
        let composed = provider.compose_vam_output(&changed_shared).unwrap();
        assert_eq!(
            composed.vertices[SHARED],
            provider.vam_basis().vertices[SHARED]
        );

        let mut changed_appended = provider.vam_basis().clone();
        changed_appended.vertices[appended_loose_index][0] += 1.0e-4;
        assert!(matches!(
            provider.canonical_proxy_from_vam(&changed_appended),
            Err(VaMGeometryError::ProtectedRegionChanged {
                region: ProtectedRegion::AppendedGeometry,
                vertex_index,
                ..
            }) if vertex_index == appended_loose_index
        ));
    }

    #[test]
    #[ignore = "requires the user's licensed G2F DSF and user-owned VaM male base"]
    fn real_user_owned_male_provider_enrolls() {
        let dsf_path = std::env::var_os("VKIT_G2_DSF")
            .expect("set VKIT_G2_DSF to the licensed Genesis2Female.dsf");
        let male_base_path = std::env::var_os("VKIT_VAM_MALE_BASE")
            .expect("set VKIT_VAM_MALE_BASE to the user-owned VaM male OBJ");
        let anchor = crate::formats::load_dsf_path(dsf_path, 0)
            .expect("load the licensed canonical G2F DSF");
        let basis = crate::formats::load_ordered_obj(male_base_path)
            .expect("load the user-owned VaM male base");

        let provider = VaMGeometryProvider::from_user_owned_pair(GeometrySex::Male, anchor, basis)
            .expect("enroll the user-owned VaM male geometry provider");
        assert_eq!(provider.sex(), GeometrySex::Male);
        assert_eq!(
            (
                provider.receipt().vam_topology.vertex_count,
                provider.receipt().vam_topology.polygon_count,
            ),
            (VAM_MALE_VERTEX_COUNT, VAM_MALE_FACE_COUNT)
        );
        assert_eq!(provider.receipt().basis_sex, GeometrySex::Male);
        assert_eq!(provider.receipt().basis_family, GeometryFamily::VirtAMate);
        assert_eq!(
            provider.receipt().anchor_family,
            GeometryFamily::DazGenesis2
        );
    }

    #[test]
    fn neutrality_gate_accepts_export_noise_and_rejects_character_looks() {
        let mut neutral_vertices = vec![[0.0_f64, 0.0, 0.0]; VAM_SHARED_VERTEX_COUNT];
        for (index, vertex) in neutral_vertices.iter_mut().enumerate() {
            vertex[1] = (index % 97) as f64 * 0.01;
        }
        let neutral = OrderedObjMesh {
            vertices: neutral_vertices.clone(),
            faces: Vec::new(),
        };

        let mut noisy = neutral.clone();
        for vertex in &mut noisy.vertices {
            vertex[0] += 0.5e-5;
        }
        let receipt = validate_neutral_shared_prefix(&noisy, &neutral).unwrap();
        assert!(receipt.rms_cm < NEUTRAL_BASIS_RMS_LIMIT_CM);

        let mut character = neutral.clone();
        character.vertices[123][2] += 0.03;
        let error = validate_neutral_shared_prefix(&character, &neutral).unwrap_err();
        assert!(matches!(
            error,
            VaMGeometryError::NotNeutralBasis {
                max_vertex_index: 123,
                ..
            }
        ));

        let mut global = neutral.clone();
        for vertex in &mut global.vertices {
            vertex[0] += 0.005;
        }
        let error = validate_neutral_shared_prefix(&global, &neutral).unwrap_err();
        assert!(matches!(error, VaMGeometryError::NotNeutralBasis { .. }));

        let mut grafted = neutral.clone();
        grafted.vertices.push([9.0, 9.0, 9.0]);
        validate_neutral_shared_prefix(&grafted, &neutral).unwrap();
    }

    #[test]
    fn neutral_base_enrollment_rejects_wrong_counts_and_non_canonical_streams() {
        let error =
            VaMGeometryProvider::from_neutral_base(GeometrySex::Female, vam_basis()).unwrap_err();
        assert!(matches!(error, VaMGeometryError::InvalidVamBasis(_)));

        let mut base = OrderedObjMesh {
            vertices: vec![[0.0, 0.0, 0.0]; VAM_SHARED_VERTEX_COUNT],
            faces: Vec::new(),
        };
        for (index, vertex) in base.vertices.iter_mut().enumerate() {
            vertex[0] = ((index % 11) as f64 - 5.0) * 0.05;
            vertex[1] = (index % 173) as f64 * 0.01;
            vertex[2] = ((index % 7) as f64 - 3.0) * 0.04;
        }
        let materials = ["Face", "Head", "Neck", "Lips"];
        for face_index in 0..G2F_POLYGON_COUNT {
            let start = (face_index * 3 % (VAM_SHARED_VERTEX_COUNT - 4)) as u32;
            base.faces.push(ObjFace {
                vertex_indices: vec![start, start + 1, start + 2, start + 3],
                group: Some(materials[face_index % materials.len()].to_owned()),
                material: Some(materials[face_index % materials.len()].to_owned()),
            });
        }
        let error = VaMGeometryProvider::from_neutral_base(GeometrySex::Female, base).unwrap_err();
        assert!(
            matches!(error, VaMGeometryError::TopologyMismatch(_)),
            "a synthetic quad stream must fail the canonical digest gate: {error:?}"
        );
    }

    #[test]
    #[ignore = "requires the user's VaM installation"]
    fn real_vam_root_enrolls_neutral_and_pair_providers() {
        let root = crate::vam::VaMRoot::open(
            std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT"),
        )
        .expect("open the VaM root");
        for sex in [GeometrySex::Female, GeometrySex::Male] {
            let neutral = crate::vam::extract_neutral_base(&root, sex).unwrap();
            let provider =
                VaMGeometryProvider::from_neutral_base(sex, neutral.mesh.clone()).unwrap();
            assert!(provider.uses_vam_derived_anchor());
            assert_eq!(
                provider.daz_anchor().vertices.len(),
                VAM_SHARED_VERTEX_COUNT
            );
            assert_eq!(provider.daz_anchor().faces.len(), G2F_POLYGON_COUNT);
            assert_eq!(
                crate::formats::topology_digest(
                    provider.daz_anchor().vertices.len(),
                    &provider.daz_anchor().faces
                )
                .unwrap(),
                G2F_TOPOLOGY_SHA256
            );

            for candidate in root.geometry_base_candidates(sex) {
                let basis = crate::formats::load_ordered_obj(&candidate).unwrap();
                if GeometrySex::shortlist_from_counts(basis.vertices.len(), basis.faces.len())
                    != Some(sex)
                {
                    continue;
                }
                if validate_neutral_shared_prefix(&basis, &neutral.mesh).is_ok() {
                    VaMGeometryProvider::from_user_owned_pair(
                        sex,
                        provider.daz_anchor().clone(),
                        basis,
                    )
                    .unwrap_or_else(|error| {
                        panic!("{} failed pair enrollment: {error}", candidate.display())
                    });
                }
            }
        }
    }
}
