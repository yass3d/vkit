mod canonical_anatomy;
mod draco;
mod dsf;
mod dsf_morph;
mod embedded_morph;
mod g2_obj;
mod mesh;
mod meshopt;
mod morph;
mod mtl;
mod obj;
mod obj_appearance;
mod session;
mod sparse_morph_pack;
mod template_pack;

pub use canonical_anatomy::{
    CANONICAL_HEAD_GROUP_NAMES, CANONICAL_HEAD_GROUPS_SCHEMA_VERSION, CanonicalHeadGroupFaces,
    CanonicalHeadGroupsDocument, EYE_GLOBE_MATERIALS, apply_canonical_head_groups,
    canonical_head_groups_cache_path, collect_canonical_head_groups,
    derive_canonical_head_group_faces, ensure_canonical_head_groups, has_canonical_head_groups,
    read_canonical_head_groups, write_canonical_head_groups,
};
pub use draco::{
    AttributeKind as DracoAttributeKind, DracoAttribute, DracoError, DracoMesh,
    MAX_BITSTREAM_VERSION as DRACO_MAX_BITSTREAM_VERSION,
    MAX_DECODED_BYTES as DRACO_MAX_DECODED_BYTES, decode_mesh as decode_draco_mesh,
};
pub use dsf::{
    DazBone, DazGeometry, HEAD_SKIN_MATERIALS, HEAD_VISUAL_EXCLUDED_MATERIALS,
    HEAD_VISUAL_MATERIALS, load_dsf, load_dsf_path,
};
pub use dsf_morph::{
    DsfMorphAsset, DsfMorphDelta, DsfMorphFigure, encode_dsf_morph, load_dsf_morph_path,
    parse_dsf_morph,
};
pub use embedded_morph::{
    EMBEDDED_G2F_EYE_CLOSED_BYTES, EmbeddedMorphDelta, EmbeddedSparseMorph,
    decode_embedded_sparse_morph, embedded_g2f_eye_closed_morph,
    embedded_g2f_eye_closed_morph_for_vam_anchor, encode_embedded_sparse_morph,
};
pub use g2_obj::{
    CANONICAL_G2_HEIGHT_CM, CANONICAL_G2F_HEIGHT_CM, G2F_TOPOLOGY_SHA256, G2ObjImport,
    G2ObjImportReceipt, G2TemplateImport, G2TemplateImportReceipt, G2TemplateNormalizationReceipt,
    canonical_g2f_topology_digest, convert_g2_template, convert_g2f_obj,
    convert_structural_g2_template, load_g2_template_obj_path, load_g2f_obj_path,
    matches_canonical_g2_topology, matches_canonical_g2f_topology, normalize_g2_template_geometry,
    normalize_g2m_template_geometry, topology_digest,
};
pub use mesh::{Mesh, canonical_mesh_hash, canonical_mesh_hash_hex};
pub use meshopt::{
    Filter as MeshoptFilter, MAX_DECODED_BYTES as MESHOPT_MAX_DECODED_BYTES, MeshoptError,
    Mode as MeshoptMode, decode_buffer_view as decode_meshopt_buffer_view,
};
pub use morph::{
    MorphAuthoring, MorphCompatibility, MorphTarget, ObjMorphSource, infer_target_unit_scale,
    load_obj_morph_target, validate_morph_topology,
};
pub use mtl::{
    DiffuseMapBinding, DiffuseMapReport, MtlDocument, MtlMaterial, MtlOpacitySource, load_mtl,
    parse_mtl,
};
pub use obj::{
    ObjFace, OrderedObjMesh, WeldReceipt, load_ordered_obj, parse_ordered_obj,
    weld_ordered_obj_vertices, write_ordered_obj,
};
pub use obj_appearance::{
    ObjAppearance, ObjDocument, TriangulatedObjAppearance, load_obj_document, parse_obj_document,
    write_obj_document, write_obj_document_path,
};
pub use session::{
    ConstraintSettings, LandmarkEndpoint, LandmarkPair, LegacyLandmarkSessionV1, MeshBinding,
    SessionMeshes, SurfaceAttachment,
};
pub use sparse_morph_pack::{
    EMBEDDED_G2F_CURATED_MORPH_PACK_BYTES, G2F_CURATED_HEAD_MATERIALS, MorphPackDelta,
    MorphPackEntry, SPARSE_MORPH_PACK_MAGIC, SPARSE_MORPH_PACK_VERSION, SparseMorphPack,
    decode_sparse_morph_pack, embedded_g2f_curated_morph_pack, embedded_g2f_curated_morph_targets,
    embedded_g2f_curated_morph_targets_for_vam_anchor, encode_sparse_morph_pack,
    vertex_mask_for_materials,
};
pub use template_pack::{
    GroupTable, SparseMorphDelta, TEMPLATE_PACK_MAGIC, TEMPLATE_PACK_VERSION, TemplatePack,
    TemplatePolygon, decode_template_pack, encode_template_pack, load_template_pack_path,
    read_template_pack, write_template_pack, write_template_pack_path,
};

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid mesh: {0}")]
    InvalidMesh(String),

    #[error("invalid OBJ at line {line}: {message}")]
    InvalidObj { line: usize, message: String },

    #[error("invalid MTL at line {line}: {message}")]
    InvalidMtl { line: usize, message: String },

    #[error("invalid DSF: {0}")]
    InvalidDsf(String),

    #[error("invalid landmark session: {0}")]
    InvalidSession(String),

    #[error("invalid morph target: {0}")]
    InvalidMorph(String),

    #[error("invalid template pack: {0}")]
    InvalidTemplatePack(String),

    #[error("invalid Genesis 2 template: {0}")]
    InvalidG2Template(String),

    #[error("invalid embedded morph resource: {0}")]
    InvalidEmbeddedMorph(String),

    #[error("invalid sparse morph pack: {0}")]
    InvalidMorphPack(String),
}

pub type Result<T> = std::result::Result<T, FormatError>;

pub(crate) fn finite_point_with<L: FnOnce() -> String>(
    value: [f64; 3],
    label: L,
) -> Result<[f64; 3]> {
    if value.iter().all(|coordinate| coordinate.is_finite()) {
        Ok(value)
    } else {
        Err(FormatError::InvalidMesh(format!(
            "{} must contain three finite coordinates",
            label()
        )))
    }
}

pub(crate) fn finite_point(value: [f64; 3], label: &str) -> Result<[f64; 3]> {
    if value.iter().all(|coordinate| coordinate.is_finite()) {
        Ok(value)
    } else {
        Err(FormatError::InvalidMesh(format!(
            "{label} must contain three finite coordinates"
        )))
    }
}
