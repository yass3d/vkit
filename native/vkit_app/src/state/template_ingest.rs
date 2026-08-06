use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde_json::Value;
use vkit_core::{
    formats::{
        DazGeometry, G2TemplateNormalizationReceipt, convert_g2_template,
        convert_structural_g2_template, load_dsf_path, matches_canonical_g2f_topology,
        normalize_g2_template_geometry, normalize_g2m_template_geometry,
    },
    vam::{GeometrySex, VaMGeometryProvider},
};

use crate::importers::{OrderedTemplateMesh, load_ordered_template_mesh};

use super::{DetectedTemplateTopology, FigureSex, TemplateTopologyKind, VaMGeometryBaseProvenance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TemplateSexEvidence {
    Structural,
    FileNameFallback,
    CurrentSelectionFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TemplateSexReceipt {
    pub(super) detected: Option<FigureSex>,
    pub(super) effective: FigureSex,
    pub(super) evidence: TemplateSexEvidence,
    pub(super) conflicting_file_name: bool,
    pub(super) structural_ambiguity: bool,
}

impl TemplateSexReceipt {
    pub(super) fn detail(self) -> String {
        let effective = match self.effective {
            FigureSex::Female => "female",
            FigureSex::Male => "male",
        };
        match self.evidence {
            TemplateSexEvidence::Structural if self.conflicting_file_name => format!(
                "structural evidence selected {effective}; conflicting filename hint ignored"
            ),
            TemplateSexEvidence::Structural => {
                format!("structural evidence selected {effective}")
            }
            TemplateSexEvidence::FileNameFallback => {
                format!("canonical G2 structure accepted; filename fallback selected {effective}")
            }
            TemplateSexEvidence::CurrentSelectionFallback if self.structural_ambiguity => {
                format!("sex evidence is ambiguous; preserved explicit {effective} selection")
            }
            TemplateSexEvidence::CurrentSelectionFallback => {
                format!("sex evidence is absent; preserved explicit {effective} selection")
            }
        }
    }
}

#[derive(Debug)]
pub struct PreparedTemplateLoad {
    pub(super) geometry: DazGeometry,
    pub(super) detected: DetectedTemplateTopology,
    pub(super) detected_sex: Option<FigureSex>,
    pub(super) sex_receipt: TemplateSexReceipt,
    pub(super) normalization: Option<G2TemplateNormalizationReceipt>,
    pub(super) provider: Option<Arc<VaMGeometryProvider>>,
    pub(super) provider_path: Option<PathBuf>,
    pub(super) provider_provenance: Option<VaMGeometryBaseProvenance>,
}

pub(super) fn prepared_neutral_template_load(
    provider: &Arc<VaMGeometryProvider>,
    bundle_path: PathBuf,
    figure_sex: FigureSex,
    provenance: VaMGeometryBaseProvenance,
) -> PreparedTemplateLoad {
    PreparedTemplateLoad {
        geometry: provider.daz_anchor().clone(),
        detected: DetectedTemplateTopology {
            vertex_count: provider.vam_basis().vertices.len(),
            face_count: provider.vam_basis().faces.len(),
            classification: match figure_sex {
                FigureSex::Female => TemplateTopologyKind::VaMFemale,
                FigureSex::Male => TemplateTopologyKind::VaMMale,
            },
        },
        detected_sex: Some(figure_sex),
        sex_receipt: TemplateSexReceipt {
            detected: Some(figure_sex),
            effective: figure_sex,
            evidence: TemplateSexEvidence::Structural,
            conflicting_file_name: false,
            structural_ambiguity: false,
        },
        normalization: None,
        provider: Some(Arc::clone(provider)),
        provider_path: Some(bundle_path),
        provider_provenance: Some(provenance),
    }
}

#[derive(Clone, Copy)]
pub(super) struct TemplateLoadContext<'a> {
    pub(super) anchor: Option<&'a DazGeometry>,
    pub(super) cached_provider: Option<&'a Arc<VaMGeometryProvider>>,
    pub(super) cached_provider_path: Option<&'a Path>,
    pub(super) cached_provenance: Option<VaMGeometryBaseProvenance>,
    pub(super) current_sex: FigureSex,
}

pub(super) fn load_template_source(
    path: &Path,
    context: TemplateLoadContext<'_>,
    resolve_provider: impl FnOnce(
        &DazGeometry,
        GeometrySex,
    ) -> Result<
        (Arc<VaMGeometryProvider>, PathBuf, VaMGeometryBaseProvenance),
        String,
    >,
) -> Result<PreparedTemplateLoad, String> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("dsf") => {
            let mut geometry = load_dsf_path(path, 0).map_err(|error| error.to_string())?;
            let strong_male = strongly_identified_dsf_male(&geometry);
            let sex_receipt = if strong_male {
                strong_male_receipt(path)
            } else {
                classify_g2_sex(&geometry, path, context.current_sex)
            };
            let normalization = if strong_male {
                normalize_g2m_template_geometry(&mut geometry).map_err(|error| error.to_string())?
            } else {
                normalize_g2_template_geometry(&mut geometry).map_err(|error| error.to_string())?
            };
            Ok(PreparedTemplateLoad {
                detected: detected_g2_topology(&geometry, sex_receipt.detected),
                detected_sex: sex_receipt.detected,
                sex_receipt,
                normalization: Some(normalization),
                geometry,
                provider: None,
                provider_path: None,
                provider_provenance: None,
            })
        }
        Some("obj" | "glb" | "fbx") => load_ordered_template_mesh(path)
            .map_err(|error| error.to_string())
            .and_then(|ordered| {
                prepare_ordered_template_load(ordered, path, context, resolve_provider)
            }),
        _ => Err(
            "G2 input must be DSF, canonical G2 OBJ/GLB/FBX, or a VMI/VMB morph pair".to_owned(),
        ),
    }
}

fn prepare_ordered_template_load(
    template: OrderedTemplateMesh,
    path: &Path,
    context: TemplateLoadContext<'_>,
    resolve_provider: impl FnOnce(
        &DazGeometry,
        GeometrySex,
    ) -> Result<
        (Arc<VaMGeometryProvider>, PathBuf, VaMGeometryBaseProvenance),
        String,
    >,
) -> Result<PreparedTemplateLoad, String> {
    let OrderedTemplateMesh {
        ordered,
        structural_labels,
    } = template;
    let detected =
        DetectedTemplateTopology::from_counts(ordered.vertices.len(), ordered.faces.len());
    let Some(sex) = vam_geometry_sex(detected.classification) else {
        let exact_g2f = matches_canonical_g2f_topology(
            ordered.vertices.len(),
            &ordered
                .faces
                .iter()
                .map(|face| face.vertex_indices.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| error.to_string())?;
        let mut import = if exact_g2f {
            convert_g2_template(&ordered)
        } else {
            convert_structural_g2_template(&ordered)
        }
        .map_err(|error| error.to_string())?;
        tag_ordered_container_metadata(&mut import.geometry, path);
        let sex_receipt = if exact_g2f {
            exact_g2f_receipt(path)
        } else {
            classify_g2_sex_with_labels(
                &import.geometry,
                path,
                context.current_sex,
                &structural_labels,
            )
        };
        return Ok(PreparedTemplateLoad {
            detected: detected_g2_topology(&import.geometry, sex_receipt.detected),
            detected_sex: sex_receipt.detected,
            sex_receipt,
            normalization: Some(import.receipt.normalization),
            geometry: import.geometry,
            provider: None,
            provider_path: None,
            provider_provenance: None,
        });
    };
    let figure_sex = geometry_sex_to_figure_sex(sex);
    let sex_receipt = TemplateSexReceipt {
        detected: Some(figure_sex),
        effective: figure_sex,
        evidence: TemplateSexEvidence::Structural,
        conflicting_file_name: false,
        structural_ambiguity: false,
    };

    if let Some(provider) = context
        .cached_provider
        .filter(|provider| provider.sex() == sex)
        && let Ok(geometry) = provider.canonical_proxy_from_vam(&ordered)
    {
        return Ok(PreparedTemplateLoad {
            geometry,
            detected,
            detected_sex: Some(figure_sex),
            sex_receipt,
            normalization: None,
            provider: Some(Arc::clone(provider)),
            provider_path: context.cached_provider_path.map(Path::to_path_buf),
            provider_provenance: context.cached_provenance,
        });
    }

    let anchor = context.anchor.ok_or_else(|| {
        format!(
            "Recognized a VaM {} full mesh ({} vertices, {} polygons); select your VaM folder first so the canonical G2 anchor is ready",
            match sex {
                GeometrySex::Female => "female",
                GeometrySex::Male => "male",
            },
            ordered.vertices.len(),
            ordered.faces.len()
        )
    })?;
    let (provider, provider_path, provider_provenance) = resolve_provider(anchor, sex)?;
    let geometry = provider
        .canonical_proxy_from_vam(&ordered)
        .map_err(|error| format!("The VaM template does not match the validated base: {error}"))?;
    Ok(PreparedTemplateLoad {
        geometry,
        detected,
        detected_sex: Some(figure_sex),
        sex_receipt,
        normalization: None,
        provider: Some(provider),
        provider_path: Some(provider_path),
        provider_provenance: Some(provider_provenance),
    })
}

fn vam_geometry_sex(kind: TemplateTopologyKind) -> Option<GeometrySex> {
    match kind {
        TemplateTopologyKind::VaMFemale => Some(GeometrySex::Female),
        TemplateTopologyKind::VaMMale => Some(GeometrySex::Male),
        TemplateTopologyKind::DazG2Female | TemplateTopologyKind::Unknown => None,
    }
}

fn geometry_sex_to_figure_sex(sex: GeometrySex) -> FigureSex {
    match sex {
        GeometrySex::Female => FigureSex::Female,
        GeometrySex::Male => FigureSex::Male,
    }
}

fn detected_g2_topology(
    geometry: &DazGeometry,
    sex: Option<FigureSex>,
) -> DetectedTemplateTopology {
    DetectedTemplateTopology {
        vertex_count: geometry.vertices.len(),
        face_count: geometry.faces.len(),
        classification: match sex {
            Some(FigureSex::Female) => TemplateTopologyKind::DazG2Female,
            Some(FigureSex::Male) | None => TemplateTopologyKind::Unknown,
        },
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SexVotes {
    female: bool,
    male: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VoteResolution {
    None,
    Female,
    Male,
    Ambiguous,
}

impl SexVotes {
    fn add(&mut self, value: &str) {
        let tokens = value
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        self.female |= tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "female" | "g2f" | "genesisfemale" | "genesis2female"
            )
        }) || tokens
            .windows(3)
            .any(|window| window[0] == "genesis" && window[1] == "2" && window[2] == "female");
        self.male |= tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "male" | "g2m" | "genesismale" | "genesis2male"
            )
        }) || tokens
            .windows(3)
            .any(|window| window[0] == "genesis" && window[1] == "2" && window[2] == "male");
    }

    const fn resolution(self) -> VoteResolution {
        match (self.female, self.male) {
            (false, false) => VoteResolution::None,
            (true, false) => VoteResolution::Female,
            (false, true) => VoteResolution::Male,
            (true, true) => VoteResolution::Ambiguous,
        }
    }
}

fn classify_g2_sex(
    geometry: &DazGeometry,
    path: &Path,
    current_sex: FigureSex,
) -> TemplateSexReceipt {
    classify_g2_sex_with_labels(geometry, path, current_sex, &[])
}

fn classify_g2_sex_with_labels(
    geometry: &DazGeometry,
    path: &Path,
    current_sex: FigureSex,
    extra_structural_labels: &[String],
) -> TemplateSexReceipt {
    let mut structural = SexVotes::default();
    structural.add(&geometry.geometry_id);
    for label in geometry
        .polygon_groups
        .iter()
        .chain(&geometry.material_groups)
    {
        structural.add(label);
    }
    add_json_sex_votes(&geometry.root_region, &mut structural);
    for label in extra_structural_labels {
        structural.add(label);
    }

    let mut file_name = SexVotes::default();
    if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
        file_name.add(stem);
    }
    let structural_resolution = structural.resolution();
    let file_resolution = file_name.resolution();
    let structural_sex = resolution_sex(structural_resolution);
    if let Some(detected) = structural_sex {
        return TemplateSexReceipt {
            detected: Some(detected),
            effective: detected,
            evidence: TemplateSexEvidence::Structural,
            conflicting_file_name: resolution_sex(file_resolution)
                .is_some_and(|hint| hint != detected),
            structural_ambiguity: false,
        };
    }
    if structural_resolution == VoteResolution::None
        && let Some(detected) = resolution_sex(file_resolution)
    {
        return TemplateSexReceipt {
            detected: Some(detected),
            effective: detected,
            evidence: TemplateSexEvidence::FileNameFallback,
            conflicting_file_name: false,
            structural_ambiguity: false,
        };
    }
    TemplateSexReceipt {
        detected: None,
        effective: current_sex,
        evidence: TemplateSexEvidence::CurrentSelectionFallback,
        conflicting_file_name: false,
        structural_ambiguity: structural_resolution == VoteResolution::Ambiguous,
    }
}

fn strongly_identified_dsf_male(geometry: &DazGeometry) -> bool {
    let mut votes = SexVotes::default();
    votes.add(&geometry.geometry_id);
    add_json_sex_votes(&geometry.root_region, &mut votes);
    votes.resolution() == VoteResolution::Male
}

fn strong_male_receipt(path: &Path) -> TemplateSexReceipt {
    let mut file_name = SexVotes::default();
    if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
        file_name.add(stem);
    }
    TemplateSexReceipt {
        detected: Some(FigureSex::Male),
        effective: FigureSex::Male,
        evidence: TemplateSexEvidence::Structural,
        conflicting_file_name: resolution_sex(file_name.resolution()) == Some(FigureSex::Female),
        structural_ambiguity: false,
    }
}

fn exact_g2f_receipt(path: &Path) -> TemplateSexReceipt {
    let mut file_name = SexVotes::default();
    if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
        file_name.add(stem);
    }
    TemplateSexReceipt {
        detected: Some(FigureSex::Female),
        effective: FigureSex::Female,
        evidence: TemplateSexEvidence::Structural,
        conflicting_file_name: resolution_sex(file_name.resolution()) == Some(FigureSex::Male),
        structural_ambiguity: false,
    }
}

fn tag_ordered_container_metadata(geometry: &mut DazGeometry, path: &Path) {
    let Some(container) = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_uppercase)
    else {
        return;
    };
    geometry.geometry_id = format!("Genesis2-{container}");
    if let Some(import) = geometry
        .root_region
        .get_mut("vkit_import")
        .and_then(Value::as_object_mut)
    {
        import.insert("source".to_owned(), Value::String(container));
    }
}

const fn resolution_sex(resolution: VoteResolution) -> Option<FigureSex> {
    match resolution {
        VoteResolution::Female => Some(FigureSex::Female),
        VoteResolution::Male => Some(FigureSex::Male),
        VoteResolution::None | VoteResolution::Ambiguous => None,
    }
}

fn add_json_sex_votes(value: &Value, votes: &mut SexVotes) {
    match value {
        Value::String(value) => votes.add(value),
        Value::Array(values) => {
            for value in values {
                add_json_sex_votes(value, votes);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                votes.add(key);
                add_json_sex_votes(value, votes);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vkit_core::formats::{ObjFace, OrderedObjMesh};

    fn evidence_geometry(id: &str, root_region: Value, labels: &[&str]) -> DazGeometry {
        DazGeometry::new(
            id.into(),
            vec![[0.0, 0.0, 0.0], [0.0, 2.0, 0.0], [1.0, 0.0, 0.0]],
            vec![vec![0, 1, 2]],
            vkit_core::formats::GroupTable {
                indices: vec![0],
                names: vec![labels.first().copied().unwrap_or("body").into()],
            },
            vkit_core::formats::GroupTable {
                indices: vec![0],
                names: vec![labels.get(1).copied().unwrap_or("skin").into()],
            },
            root_region,
        )
        .unwrap()
    }

    #[test]
    fn structural_sex_evidence_outranks_a_conflicting_file_name() {
        let geometry = evidence_geometry("Genesis2Male", json!({}), &["body", "skin"]);
        let receipt = classify_g2_sex(
            &geometry,
            Path::new("Genesis2Female.obj"),
            FigureSex::Female,
        );
        assert_eq!(receipt.detected, Some(FigureSex::Male));
        assert_eq!(receipt.effective, FigureSex::Male);
        assert_eq!(receipt.evidence, TemplateSexEvidence::Structural);
        assert!(receipt.conflicting_file_name);
    }

    #[test]
    fn ambiguous_structural_evidence_preserves_the_explicit_selection() {
        let geometry = evidence_geometry(
            "Genesis2Female",
            json!({"node": "Genesis2Male"}),
            &["body", "skin"],
        );
        let receipt = classify_g2_sex(&geometry, Path::new("G2F.obj"), FigureSex::Male);
        assert_eq!(receipt.detected, None);
        assert_eq!(receipt.effective, FigureSex::Male);
        assert_eq!(
            receipt.evidence,
            TemplateSexEvidence::CurrentSelectionFallback
        );
        assert!(receipt.structural_ambiguity);
    }

    #[test]
    fn file_name_is_only_a_fallback_after_structure_has_no_sex_marker() {
        let geometry = evidence_geometry("geometry", json!({}), &["body", "skin"]);
        let receipt = classify_g2_sex(&geometry, Path::new("neutral_G2M.fbx"), FigureSex::Female);
        assert_eq!(receipt.detected, Some(FigureSex::Male));
        assert_eq!(receipt.evidence, TemplateSexEvidence::FileNameFallback);

        let ambiguous = classify_g2_sex(&geometry, Path::new("G2F_G2M.obj"), FigureSex::Female);
        assert_eq!(ambiguous.detected, None);
        assert_eq!(ambiguous.effective, FigureSex::Female);
    }

    #[test]
    fn female_marker_does_not_accidentally_vote_for_male() {
        let mut votes = SexVotes::default();
        votes.add("Genesis2Female");
        assert_eq!(votes.resolution(), VoteResolution::Female);
    }

    #[test]
    fn exact_g2f_digest_is_structural_female_evidence() {
        let receipt = exact_g2f_receipt(Path::new("misnamed_G2M.fbx"));
        assert_eq!(receipt.detected, Some(FigureSex::Female));
        assert_eq!(receipt.effective, FigureSex::Female);
        assert_eq!(receipt.evidence, TemplateSexEvidence::Structural);
        assert!(receipt.conflicting_file_name);
    }

    #[test]
    fn filename_and_current_sex_do_not_weaken_structural_validation() {
        let template = OrderedTemplateMesh {
            ordered: OrderedObjMesh {
                vertices: vec![[-0.5, 0.0, 0.0], [0.5, 0.0, 0.0], [0.0, 2.0, 0.0]],
                faces: vec![ObjFace {
                    vertex_indices: vec![0, 1, 2],
                    group: Some("lEye".to_owned()),
                    material: Some("Sclera".to_owned()),
                }],
            },
            structural_labels: vec!["Genesis2Male".to_owned()],
        };
        let context = TemplateLoadContext {
            anchor: None,
            cached_provider: None,
            cached_provider_path: None,
            cached_provenance: None,
            current_sex: FigureSex::Male,
        };
        let error = prepare_ordered_template_load(
            template,
            Path::new("definitely_G2M.obj"),
            context,
            |_, _| panic!("invalid G2 counts must fail before provider lookup"),
        )
        .expect_err("invalid structural template must be rejected");
        assert!(error.contains("expected 21556 vertices"), "{error}");
    }

    #[test]
    fn ordered_container_metadata_reports_fbx_instead_of_obj() {
        let mut geometry = evidence_geometry(
            "Genesis2-OBJ",
            json!({"vkit_import": {"source": "OBJ"}}),
            &["head", "Face"],
        );
        tag_ordered_container_metadata(&mut geometry, Path::new("template.fbx"));
        assert_eq!(geometry.geometry_id, "Genesis2-FBX");
        assert_eq!(geometry.root_region["vkit_import"]["source"], "FBX");
    }

    #[test]
    fn fbx_template_loading_uses_the_ordered_path_not_custom_head_simplification() {
        let workspace = tempfile::tempdir().expect("fixture workspace");
        let path = workspace.path().join("ordered-template.fbx");
        std::fs::write(
            &path,
            include_str!("../importers/fixtures/ordered_template_quad_ascii.fbx"),
        )
        .expect("fixture write");
        let context = TemplateLoadContext {
            anchor: None,
            cached_provider: None,
            cached_provider_path: None,
            cached_provenance: None,
            current_sex: FigureSex::Female,
        };

        let result = load_template_source(&path, context, |_, _| {
            panic!("the tiny G2M-labelled FBX must fail before provider lookup")
        });
        let Err(error) = result else {
            panic!("tiny fixture is not a G2 base");
        };
        assert!(
            error.contains("21556"),
            "tiny fixture must be rejected by the G2 base-count gate: {error}"
        );
    }
}
