mod base_discovery;
mod builtin_morph_patch;
mod catalog;
mod geometry;
mod hair;
pub mod hair_joints;
pub mod hair_writer;
mod morph_route;
mod package_index;
mod pose;
mod rig_transfer;
mod simple_json;

mod rig_bake;
mod skeleton;
mod skin;
mod skin_binding;
mod unity_base;
mod unity_morph_bank;
mod unity_textures;
mod uv;
mod var_package;
mod vmb;
mod vmi;
mod zip_archive;

pub use base_discovery::{
    DiscoveredGeometryBase, GeometryBaseRequest, GeometryBaseTier, discover_geometry_base,
};
pub use builtin_morph_patch::{
    BuiltinMorphDigest, BuiltinMorphPatchEntry, BuiltinMorphPatchOutput, BuiltinMorphReplacement,
    patch_builtin_morph_bank, read_builtin_morph_digests,
};
pub use catalog::{
    BuiltinMorphDescriptor, BuiltinMorphSource, EYELID_LOOK_CONTROLS, MorphCatalog, MorphCategory,
    VaMRoot, geometry_base_candidate_names, is_eyelid_look_control,
};
pub use geometry::{
    DEFAULT_PROTECTED_EPSILON_CM, GeometryFamily, GeometryResult, GeometrySex,
    NEUTRAL_BASIS_MAX_LIMIT_CM, NEUTRAL_BASIS_RMS_LIMIT_CM, NeutralityReceipt, ProtectedRegion,
    TopologyFingerprint, VAM_FEMALE_FACE_COUNT, VAM_FEMALE_VERTEX_COUNT, VAM_MALE_FACE_COUNT,
    VAM_MALE_VERTEX_COUNT, VAM_METRES_TO_DAZ_CM, VAM_SHARED_VERTEX_COUNT, VaMGeometryError,
    VaMGeometryProvider, VaMGeometrySidecarReceipt, validate_neutral_shared_prefix,
};
pub use hair::{
    BuiltinHairScalp, HairGuide, HairGuideGeometry, HairLookPatch, HairNearbyJoint,
    HairOpticalSettings, HairPartAsset, HairPartReference, HairPhysicsPatch, HairPhysicsSettings,
    HairPreset, HairPresetScanDiagnostics, HairPresetScanReport, HairScalpGeometry,
    HairScalpMaterialSettings, HairScalpTextureBytes, HairShaderType, HairSpreadSettings,
    HairWavinessSettings, hair_look_from_storable, hair_physics_from_storable,
    hair_scalp_material_from_storables, hair_scalp_material_storable, hair_sim_storable,
    load_builtin_hair_scalps, load_hair_part_asset, load_hair_part_geometry, load_hair_part_look,
    load_hair_part_scalp, load_hair_part_settings, load_hair_scalp_textures, parse_hair_scalp_vab,
    parse_hair_vab, read_hair_storables, scan_hair_presets, scan_hair_presets_with_report,
};
pub use morph_route::{VaMMorphRoute, VaMMorphRouteError, classify_vam_morph_route};
pub use var_package::{
    ExistingPackage, VAR_LICENSES, VarContent, VarMetadata, VarPackage, next_version,
    safe_identity, var_package_path, write_var_package,
};

pub use package_index::{
    PackageAsset, PackageAssetKind, PackageIndex, PackageIndexProgress, PackageRef,
};
pub use pose::{
    MorphReference, PackageRequest, PoseBone, PoseDocument, PoseMorph, PoseMorphSource,
    PoseResolution, PoseRoot, resolve_pose, resolve_pose_morph,
};
pub use rig_bake::{merge_rig_delta, rig_delta, strip_rig_delta};
pub use rig_transfer::{RIG_TRANSFER_NEIGHBORS, RIG_TRANSFER_POWER, transfer_rig_point_idw};
pub use skeleton::{
    RestBone, RestSkeleton, RotationOrder, extract_rest_skeleton, extract_rest_skeleton_from_bundle,
};
pub use skin::{
    AssetLocator, SkinAuxMaterial, SkinAuxiliary, SkinDiffuseSource, SkinPreset, SkinRegion,
    SkinScanDiagnostics, SkinScanReport, SkinSex, SkinSurfaceLocators, SkinTextureChannel,
    SkinTextureSet, filter_skin_presets_by_sex, list_var_entries, read_var_entry_bytes,
    scan_skin_library, scan_skin_library_with_report,
};
pub use skin_binding::{
    BoneBinding, BoundSkin, SkinBinding, TriaxWeight, extract_skin_binding,
    extract_skin_binding_from_bundle, extract_skin_bindings, extract_skin_bindings_from_bundle,
};
pub use unity_textures::{
    BuiltinScalpTextureSet, BuiltinTextureRef, DecodedBuiltinTexture, load_builtin_texture_rgba,
    scan_builtin_scalp_textures, scan_builtin_skins,
};

pub use unity_base::{
    NeutralBaseCacheReceipt, NeutralBaseHandle, NeutralBaseMesh, extract_merged_mesh_from_bundle,
    extract_neutral_base, extract_neutral_base_from_bundle, load_or_extract_neutral_base,
    read_neutral_base_receipt,
};
pub use unity_morph_bank::{
    BuiltinMorphSession, Formula, FormulaTarget, MorphResolutionReceipt, ResolvedMorph,
    load_resolved_builtin,
};
pub use uv::{
    G2UvFace, G2UvMapping, G2UvTriangle, UvCorner, UvMaterialRegion, canonical_head_face_mask,
    canonical_head_vertex_mask, load_g2_uv_mapping, load_g2_uv_mapping_for_sex,
};
pub use vmb::{
    SparseDelta, VAM_SHARED_BODY_VERTEX_COUNT, VmbVertexRouting, build_sparse_deltas_daz_cm,
    decode_vmb_daz_cm, decode_vmb_daz_cm_for_topology, encode_vmb_daz_cm,
    encode_vmb_daz_cm_for_topology, vmb_raw_entry_count,
};
pub use vmi::{
    ShapeVmiOptions, VmiBooleanScalar, VmiDocument, VmiFormula, VmiNumericScalar, build_shape_vmi,
    build_shape_vmi_with_options, encode_vmi_pretty, parse_vmi, read_vmi, write_vmi_pretty,
};

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaMError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid VaM installation at {path}: {message}")]
    InvalidRoot { path: PathBuf, message: String },

    #[error("invalid VaM morph bank: {0}")]
    InvalidMorphBank(String),

    #[error("invalid VaM base bundle: {0}")]
    InvalidBaseBundle(String),

    #[error("invalid VaM VMI metadata: {0}")]
    InvalidVmi(String),

    #[error("VMI I/O error while {operation}: {source}")]
    VmiIo {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("unsupported VaM asset encoding: {0}")]
    UnsupportedEncoding(String),

    #[error("invalid VaM skin preset at {locator}: {message}")]
    InvalidSkinPreset { locator: String, message: String },

    #[error("invalid VaM hair preset at {locator}: {message}")]
    InvalidHairPreset { locator: String, message: String },

    #[error("invalid VaM hair geometry at {locator}: {message}")]
    InvalidHairGeometry { locator: String, message: String },

    #[error("unsupported VaM hair geometry at {locator}: {message}")]
    UnsupportedHairGeometry { locator: String, message: String },

    #[error("invalid VaM G2 UV source: {0}")]
    InvalidUv(String),

    #[error(transparent)]
    Format(#[from] crate::formats::FormatError),
}

pub type Result<T> = std::result::Result<T, VaMError>;

pub(crate) fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> VaMError {
    VaMError::Io {
        path: path.into(),
        source,
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};

    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut accumulator, byte| {
            let _ = write!(accumulator, "{byte:02x}");
            accumulator
        })
}
