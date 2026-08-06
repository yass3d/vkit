mod assisted_alignment;
mod deformation_graph;
mod dense_registration;
mod geometry_assisted;
mod landmark_warp;
mod lsmr;
mod operator;
mod semantic_assisted;
mod semantic_surface;

pub use assisted_alignment::{
    AssistedAlignmentError, AssistedAlignmentRejection, AssistedAlignmentResult,
    AssistedFitOptions, AssistedFitReceipt, AssistedIcpIterationReceipt, AssistedIcpSkipReason,
    AssistedInitializationMode, GuardedWeightedFallbackRejection, MIN_ASSISTED_FIT_PAIRS,
    RECOMMENDED_ASSISTED_FIT_PAIRS, compose_similarity, estimate_assisted_similarity,
    estimate_assisted_similarity_with_prior, refine_assisted_similarity_icp,
};
pub use dense_registration::{
    DEFAULT_DENSE_REGISTRATION_STAGES, DenseIterationOutcome, DenseIterationReceipt,
    DenseRegistrationError, DenseRegistrationOptions, DenseRegistrationProgress,
    DenseRegistrationReport, DenseRegistrationResult, DenseRegistrationStage, DenseStageKind,
    G2F_HEAD_SKIN_TRIANGLE_COUNT, G2F_HEAD_SKIN_VERTEX_COUNT, G2F_NECK_SEAM_VERTEX_COUNT,
    PYTHON_PARITY_DENSE_REGISTRATION_STAGES, SCAN_FIDELITY_CLEAN_LIMIT, SCAN_FIDELITY_RANGE,
    SkinRegistrationInputs, dense_stages_at_fidelity, nonrigid_skin_registration,
};
pub use geometry_assisted::{
    AutomaticConstraintProvenance, AutomaticConstraintRejection, AutomaticSurfaceConstraintError,
    AutomaticSurfaceConstraintOptions, AutomaticSurfaceConstraintReceipt,
    AutomaticSurfaceConstraintResult, AutomaticSurfaceCorrespondence, GeometryExtentPriorResult,
    GeometryInitializationError, GeometryInitializationMode, GeometryInitializationReceipt,
    GeometryInitializationRejection, GeometryInitializationRequest, GeometryInitializationResult,
    RobustInitializationFallback, SparseInitializationFallback,
    build_automatic_surface_correspondences, initialize_geometry_assisted_similarity,
    refine_zero_pin_extent_prior, suggest_zero_pin_extent_similarity,
};
pub use landmark_warp::{
    AnchorOperator, BarycentricConstraint, BarycentricLandmarkOperator, LandmarkWarpError,
    LandmarkWarpOptions, LandmarkWarpReport, LandmarkWarpResult, UniformLaplacianOperator,
    WarpSolveReport, landmark_laplacian_warp, topology_safe_step,
};
pub use lsmr::{LsmrError, LsmrOptions, LsmrReport, LsmrStop, lsmr, sym_ortho};
pub use operator::{CsrMatrix, LinearOperator, OperatorError, StackedOperator};
pub use semantic_assisted::{
    SemanticConstraintRejection, SemanticSurfaceConstraintError, SemanticSurfaceConstraintReceipt,
    SemanticSurfaceConstraintResult, build_semantic_surface_correspondences,
};
pub use semantic_surface::{
    SEMANTIC_RENDER_SIZE, SemanticFaceRender, SemanticProjectionFrame, SemanticSurfaceError,
    SemanticSurfaceRejection, attach_semantic_points, attach_visible_semantic_points,
    render_semantic_face,
};
