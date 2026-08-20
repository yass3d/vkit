use std::{cmp::Reverse, collections::BTreeMap, sync::Arc};

use vkit_core::{
    formats::{FormatError, MorphTarget, OrderedObjMesh},
    vam::{
        GeometrySex, SparseDelta, VAM_SHARED_BODY_VERTEX_COUNT, VaMGeometryProvider,
        VaMGeometrySidecarReceipt,
    },
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MorphCategory {
    Eyes,
    Brows,
    Nose,
    Mouth,
    Jaw,
    Cheeks,
    Ears,
    Head,
    Expression,
    Cheekbones,

    Body,
}

impl MorphCategory {
    pub const ALL: [Self; 10] = [
        Self::Eyes,
        Self::Brows,
        Self::Nose,
        Self::Mouth,
        Self::Jaw,
        Self::Cheeks,
        Self::Ears,
        Self::Head,
        Self::Body,
        Self::Expression,
    ];
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MorphCategoryFilter {
    #[default]
    All,
    Category(MorphCategory),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MorphSource {
    VaMBuiltin,
    Curated,

    PackagedVaM,

    LocalVaM,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HorizontalSymmetry {
    #[default]
    Mirrored,
    Left,
    Right,
}

impl HorizontalSymmetry {
    pub const fn is_one_sided(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MorphAvailability {
    Resolved,
    Unresolved,
    Loading,
    Failed,
}

impl MorphSource {
    pub const fn is_imported(self) -> bool {
        matches!(self, Self::PackagedVaM | Self::LocalVaM)
    }

    const fn priority(self) -> u8 {
        match self {
            Self::VaMBuiltin => 1,
            Self::Curated => 2,
            Self::PackagedVaM | Self::LocalVaM => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MorphControl {
    pub id: String,
    pub label: String,
    pub category: MorphCategory,
    pub source: MorphSource,
    pub symmetry: HorizontalSymmetry,
    pub value: f32,

    pub target: MorphTargetBinding,

    pub genital_target: Option<Arc<GenitalMorphTarget>>,
    pub availability: MorphAvailability,

    pub unsupported_formula_count: usize,
}

#[derive(Clone, Debug)]
pub struct GenitalMorphTarget {
    pub sex: GeometrySex,
    pub provider_receipt: VaMGeometrySidecarReceipt,
    pub full_vertex_count: usize,
    pub deltas: Arc<[SparseDelta]>,
}

#[derive(Clone, Debug)]
pub struct GenitalMorphApplication {
    pub target: Arc<GenitalMorphTarget>,
    pub amount: f64,
}

impl GenitalMorphTarget {
    pub fn new(
        sex: GeometrySex,
        provider_receipt: VaMGeometrySidecarReceipt,
        full_vertex_count: usize,
        deltas: Vec<SparseDelta>,
    ) -> Result<Self, FormatError> {
        let shared = VAM_SHARED_BODY_VERTEX_COUNT as usize;
        if provider_receipt.basis_sex != sex
            || provider_receipt.shared_vertex_count != shared
            || provider_receipt.vam_topology.vertex_count != full_vertex_count
            || full_vertex_count <= shared
        {
            return Err(FormatError::InvalidMorph(
                "genital morph provider receipt has incompatible sex or topology".to_owned(),
            ));
        }
        if deltas.is_empty() {
            return Err(FormatError::InvalidMorph(
                "genital morph contains no nonzero appended deltas".to_owned(),
            ));
        }
        let mut previous = None;
        for delta in &deltas {
            let index = delta.vertex_index as usize;
            if index < shared || index >= full_vertex_count {
                return Err(FormatError::InvalidMorph(format!(
                    "genital morph vertex {} is outside appended topology [{shared}, {full_vertex_count})",
                    delta.vertex_index
                )));
            }
            if !delta.delta_cm.iter().all(|value| value.is_finite()) {
                return Err(FormatError::InvalidMorph(format!(
                    "genital morph vertex {} has a non-finite delta",
                    delta.vertex_index
                )));
            }
            if previous.is_some_and(|value| value >= delta.vertex_index) {
                return Err(FormatError::InvalidMorph(
                    "genital morph indices must be unique and sorted".to_owned(),
                ));
            }
            previous = Some(delta.vertex_index);
        }
        Ok(Self {
            sex,
            provider_receipt,
            full_vertex_count,
            deltas: deltas.into(),
        })
    }

    fn validate_provider(&self, provider: &VaMGeometryProvider) -> Result<(), FormatError> {
        if provider.sex() != self.sex
            || provider.vam_basis().vertices.len() != self.full_vertex_count
            || provider.receipt().shared_vertex_count != VAM_SHARED_BODY_VERTEX_COUNT as usize
        {
            return Err(FormatError::InvalidMorph(
                "genital morph does not match the active VaM provider sex/topology".to_owned(),
            ));
        }
        provider
            .validate_sidecar(&self.provider_receipt)
            .map_err(|error| FormatError::InvalidMorph(error.to_string()))
    }
}

#[derive(Clone, Debug)]
pub struct MorphTargetBinding {
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
    resolved: Option<Arc<MorphTarget>>,
}

impl MorphTargetBinding {
    fn from_resolved(target: Arc<MorphTarget>) -> Self {
        Self {
            minimum: target.minimum,
            maximum: target.maximum,
            default: target.default,
            resolved: Some(target),
        }
    }

    fn unresolved(
        _key: impl Into<String>,
        minimum: f64,
        maximum: f64,
        default: f64,
    ) -> Result<Self, FormatError> {
        if !minimum.is_finite()
            || !maximum.is_finite()
            || !default.is_finite()
            || minimum > maximum
            || default < minimum
            || default > maximum
        {
            return Err(FormatError::InvalidMorph(
                "unresolved morph slider range is invalid".to_owned(),
            ));
        }
        Ok(Self {
            minimum,
            maximum,
            default,
            resolved: None,
        })
    }

    fn resolved(&self) -> Option<&MorphTarget> {
        self.resolved.as_deref()
    }

    #[cfg(test)]
    pub const fn authored_bounds(&self) -> (f64, f64) {
        (self.minimum, self.maximum)
    }

    pub fn entry_bounds(&self) -> (f64, f64) {
        self.resolved().map_or(
            (
                self.minimum
                    .min(-MorphTarget::MINIMUM_EXTRAPOLATION_MAGNITUDE),
                self.maximum
                    .max(MorphTarget::MINIMUM_EXTRAPOLATION_MAGNITUDE),
            ),
            MorphTarget::extrapolated_bounds,
        )
    }

    pub fn slider_bounds(&self, value: f64) -> (f64, f64) {
        let (entry_minimum, entry_maximum) = self.entry_bounds();
        let value = value.clamp(entry_minimum, entry_maximum);
        (self.minimum.min(value), self.maximum.max(value))
    }

    fn extrapolated_bounds(&self) -> (f64, f64) {
        self.entry_bounds()
    }

    #[cfg(test)]
    fn is_resolved(&self) -> bool {
        self.resolved.is_some()
    }
}

impl MorphControl {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        category: MorphCategory,
        source: MorphSource,
        target: Arc<MorphTarget>,
    ) -> Self {
        let (id, label) = (id.into(), label.into());
        Self {
            symmetry: horizontal_symmetry(&label).or(horizontal_symmetry(&id)),
            id,
            label,
            category,
            source,
            value: target.default as f32,
            target: MorphTargetBinding::from_resolved(target),
            genital_target: None,
            availability: MorphAvailability::Resolved,
            unsupported_formula_count: 0,
        }
    }

    pub fn new_unresolved(
        id: impl Into<String>,
        label: impl Into<String>,
        category: MorphCategory,
        source: MorphSource,
        slider_contract: MorphTargetBinding,
    ) -> Self {
        let (id, label) = (id.into(), label.into());
        Self {
            symmetry: horizontal_symmetry(&label).or(horizontal_symmetry(&id)),
            id,
            label,
            category,
            source,
            value: slider_contract.default as f32,
            target: slider_contract,
            genital_target: None,
            availability: MorphAvailability::Unresolved,
            unsupported_formula_count: 0,
        }
    }

    pub fn new_genital_vam(
        id: impl Into<String>,
        label: impl Into<String>,
        category: MorphCategory,
        target: Arc<GenitalMorphTarget>,
        minimum: f64,
        maximum: f64,
        default: f64,
    ) -> Result<Self, FormatError> {
        let (id, label) = (id.into(), label.into());
        let slider = MorphTargetBinding::unresolved(&id, minimum, maximum, default)?;
        Ok(Self {
            symmetry: horizontal_symmetry(&label).or(horizontal_symmetry(&id)),
            id,
            label,
            category,
            source: MorphSource::PackagedVaM,
            value: default as f32,
            target: slider,
            genital_target: Some(target),
            availability: MorphAvailability::Resolved,
            unsupported_formula_count: 0,
        })
    }

    pub fn is_resolved(&self) -> bool {
        self.availability == MorphAvailability::Resolved
    }

    pub fn is_modified(&self) -> bool {
        let value = f64::from(self.value);
        value.is_finite() && (value - self.target.default).abs() > 1.0e-6
    }

    pub fn active(&self) -> bool {
        self.is_resolved() && self.is_modified()
    }

    pub(crate) fn canonical_active(&self) -> bool {
        self.active() && self.genital_target.is_none()
    }
}

pub fn unresolved_slider_contract(
    _topology_template: &MorphTarget,
    key: impl Into<String>,
    minimum: f64,
    maximum: f64,
    default: f64,
) -> Result<MorphTargetBinding, FormatError> {
    MorphTargetBinding::unresolved(key, minimum, maximum, default)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MorphListFilter {
    #[default]
    All,

    Active,
}

#[derive(Debug, Default)]
pub struct MorphLibrary {
    controls: Vec<MorphControl>,
    pub query: String,
    pub category_filter: MorphCategoryFilter,
    list_filter: MorphListFilter,

    pub show_one_sided: bool,
}

fn horizontal_symmetry(name: &str) -> HorizontalSymmetry {
    fn side_of(word: &str, was_capital: bool) -> Option<HorizontalSymmetry> {
        match word {
            "left" | "lft" => Some(HorizontalSymmetry::Left),
            "right" | "rgt" => Some(HorizontalSymmetry::Right),
            "l" if was_capital => Some(HorizontalSymmetry::Left),
            "r" if was_capital => Some(HorizontalSymmetry::Right),
            _ => None,
        }
    }

    let mut found = None;
    for token in name.split(|character: char| !character.is_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        let mut word = String::new();
        let mut word_was_capital = false;
        let mut previous_was_lower = false;

        for (index, character) in token.char_indices() {
            let starts_camel_word = character.is_uppercase() && previous_was_lower;
            let is_trailing_side = matches!(character, 'L' | 'R')
                && index + character.len_utf8() == token.len()
                && token.len() > 2;
            if starts_camel_word || is_trailing_side {
                found = found.or(side_of(&word, word_was_capital));
                word.clear();
            }
            if word.is_empty() {
                word_was_capital = character.is_uppercase();
            }
            previous_was_lower = character.is_lowercase();
            word.extend(character.to_lowercase());
        }
        found = found.or(side_of(&word, word_was_capital));
    }
    found.unwrap_or(HorizontalSymmetry::Mirrored)
}

impl HorizontalSymmetry {
    const fn or(self, other: Self) -> Self {
        match self {
            Self::Mirrored => other,
            sided => sided,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MorphLibraryValueSnapshot {
    values: BTreeMap<String, f32>,
}

impl MorphLibraryValueSnapshot {
    pub const fn from_values(values: BTreeMap<String, f32>) -> Self {
        Self { values }
    }
}

impl MorphLibrary {
    pub fn controls(&self) -> &[MorphControl] {
        &self.controls
    }

    pub const fn list_filter(&self) -> MorphListFilter {
        self.list_filter
    }

    pub const fn modified_only(&self) -> bool {
        matches!(self.list_filter, MorphListFilter::Active)
    }

    pub fn set_list_filter(&mut self, filter: MorphListFilter) -> bool {
        if self.list_filter == filter {
            return false;
        }
        self.list_filter = filter;
        true
    }

    pub fn has_modified_controls(&self) -> bool {
        self.controls.iter().any(MorphControl::is_modified)
    }

    pub fn replace_source(&mut self, source: MorphSource, incoming: Vec<MorphControl>) {
        let previous_values = self
            .controls
            .iter()
            .map(|control| (canonical_identity(&control.id), control.value))
            .collect::<BTreeMap<_, _>>();
        self.controls.retain(|control| control.source != source);
        self.controls
            .extend(incoming.into_iter().map(|mut control| {
                control.source = source;
                if let Some(value) = previous_values.get(&canonical_identity(&control.id)) {
                    let (minimum, maximum) = control.target.entry_bounds();
                    control.value = f64::from(*value).clamp(minimum, maximum) as f32;
                }
                control
            }));

        self.controls.sort_by_key(|control| {
            (
                Reverse(control.source.priority()),
                control.label.to_ascii_lowercase(),
                control.id.to_ascii_lowercase(),
            )
        });
        let mut accepted = BTreeMap::<String, u8>::new();
        self.controls.retain(|control| {
            let key = canonical_identity(&control.id);
            match accepted.get(&key) {
                Some(&priority) if priority >= control.source.priority() => false,
                _ => {
                    accepted.insert(key, control.source.priority());
                    true
                }
            }
        });
    }

    pub fn upsert_imported_vam(&mut self, incoming: MorphControl) {
        debug_assert!(
            incoming.source.is_imported(),
            "upsert_imported_vam received a shipped morph"
        );
        let mut incoming = incoming;
        let identity = canonical_identity(&incoming.id);
        let previous_value = self
            .controls
            .iter()
            .find(|control| {
                control.source.is_imported() && canonical_identity(&control.id) == identity
            })
            .map(|control| control.value);
        self.controls.retain(|control| {
            !control.source.is_imported() || canonical_identity(&control.id) != identity
        });
        if let Some(value) = previous_value {
            let (minimum, maximum) = incoming.target.extrapolated_bounds();
            incoming.value = f64::from(value).clamp(minimum, maximum) as f32;
        }
        self.controls.push(incoming);
        self.sort_and_deduplicate();
    }

    fn sort_and_deduplicate(&mut self) {
        self.controls.sort_by_key(|control| {
            (
                Reverse(control.source.priority()),
                control.label.to_ascii_lowercase(),
                control.id.to_ascii_lowercase(),
            )
        });
        let mut accepted = BTreeMap::<String, u8>::new();
        self.controls.retain(|control| {
            let key = canonical_identity(&control.id);
            match accepted.get(&key) {
                Some(&priority) if priority >= control.source.priority() => false,
                _ => {
                    accepted.insert(key, control.source.priority());
                    true
                }
            }
        });
    }

    pub fn set_value(&mut self, id: &str, value: f32) -> bool {
        let identity = canonical_identity(id);
        let Some(control) = self
            .controls
            .iter_mut()
            .find(|control| canonical_identity(&control.id) == identity)
        else {
            return false;
        };
        if control.availability != MorphAvailability::Resolved {
            return false;
        }
        let value = f64::from(value);
        if !value.is_finite() {
            return false;
        }
        let (minimum, maximum) = control.target.extrapolated_bounds();
        let value = value.clamp(minimum, maximum) as f32;
        if (control.value - value).abs() <= f32::EPSILON {
            return false;
        }
        control.value = value;
        true
    }

    pub fn begin_resolution(&mut self, id: &str, value: f32) -> bool {
        let identity = canonical_identity(id);
        let Some(control) = self.controls.iter_mut().find(|control| {
            control.source == MorphSource::VaMBuiltin && canonical_identity(&control.id) == identity
        }) else {
            return false;
        };
        if control.availability == MorphAvailability::Resolved {
            return false;
        }
        let value = f64::from(value);
        if !value.is_finite() {
            return false;
        }
        let (minimum, maximum) = control.target.extrapolated_bounds();
        control.value = value.clamp(minimum, maximum) as f32;
        if control.availability == MorphAvailability::Loading {
            return false;
        }
        control.availability = MorphAvailability::Loading;
        true
    }

    pub fn needs_resolution(&self, id: &str) -> bool {
        let identity = canonical_identity(id);
        self.controls.iter().any(|control| {
            control.source == MorphSource::VaMBuiltin
                && control.availability != MorphAvailability::Resolved
                && canonical_identity(&control.id) == identity
        })
    }

    pub fn has_loading_controls(&self) -> bool {
        self.controls
            .iter()
            .any(|control| control.availability == MorphAvailability::Loading)
    }

    pub fn install_resolved_target(
        &mut self,
        id: &str,
        target: Arc<MorphTarget>,
    ) -> Result<bool, FormatError> {
        target.validate()?;
        let identity = canonical_identity(id);
        let Some(control) = self.controls.iter_mut().find(|control| {
            control.source == MorphSource::VaMBuiltin && canonical_identity(&control.id) == identity
        }) else {
            return Ok(false);
        };
        let (minimum, maximum) = target.extrapolated_bounds();
        control.value = f64::from(control.value).clamp(minimum, maximum) as f32;
        control.target = MorphTargetBinding::from_resolved(target);
        control.availability = MorphAvailability::Resolved;

        Ok(true)
    }

    pub fn set_unsupported_formula_count(&mut self, id: &str, count: usize) -> bool {
        let identity = canonical_identity(id);
        let Some(control) = self.controls.iter_mut().find(|control| {
            control.source == MorphSource::VaMBuiltin && canonical_identity(&control.id) == identity
        }) else {
            return false;
        };
        control.unsupported_formula_count = count;
        true
    }

    pub fn mark_resolution_failed(&mut self, id: &str) -> bool {
        let identity = canonical_identity(id);
        let Some(control) = self.controls.iter_mut().find(|control| {
            control.source == MorphSource::VaMBuiltin && canonical_identity(&control.id) == identity
        }) else {
            return false;
        };
        control.value = control.target.default as f32;
        control.availability = MorphAvailability::Failed;
        true
    }

    pub fn mark_resolution_deferred(&mut self, id: &str) -> bool {
        let identity = canonical_identity(id);
        let Some(control) = self.controls.iter_mut().find(|control| {
            control.source == MorphSource::VaMBuiltin && canonical_identity(&control.id) == identity
        }) else {
            return false;
        };
        control.value = control.target.default as f32;
        control.availability = MorphAvailability::Unresolved;
        true
    }

    pub fn snapshot_values(&self) -> MorphLibraryValueSnapshot {
        MorphLibraryValueSnapshot {
            values: self
                .controls
                .iter()
                .map(|control| (control.id.clone(), control.value))
                .collect(),
        }
    }

    pub fn restore_values(&mut self, snapshot: &MorphLibraryValueSnapshot) -> bool {
        let mut changed = false;
        for control in &mut self.controls {
            let Some(&value) = snapshot.values.get(&control.id) else {
                continue;
            };
            changed |= control.value.to_bits() != value.to_bits();
            control.value = value;
        }
        changed
    }

    pub fn reset(&mut self) -> bool {
        let mut changed = false;
        for control in &mut self.controls {
            let default = control.target.default as f32;
            changed |= control.value.to_bits() != default.to_bits();
            control.value = default;
        }
        changed
    }

    #[must_use]
    pub fn control_passes_non_text_filters(&self, control: &MorphControl) -> bool {
        let category_matches = match self.category_filter {
            MorphCategoryFilter::All => true,
            MorphCategoryFilter::Category(MorphCategory::Cheeks) => matches!(
                control.category,
                MorphCategory::Cheeks | MorphCategory::Cheekbones
            ),
            MorphCategoryFilter::Category(category) => control.category == category,
        };
        let pile_matches = match self.list_filter {
            MorphListFilter::All => true,
            MorphListFilter::Active => control.is_modified(),
        };
        let side_matches = self.show_one_sided || !control.symmetry.is_one_sided();
        category_matches && pile_matches && side_matches
    }

    #[allow(
        dead_code,
        reason = "retained as the locale-neutral library filter used by focused tests"
    )]
    fn control_matches_normalized_filters(&self, control: &MorphControl, query: &str) -> bool {
        let search_matches = query.is_empty()
            || normalize_search(&format!("{} {}", control.label, control.id)).contains(query);
        self.control_passes_non_text_filters(control) && search_matches
    }

    #[allow(
        dead_code,
        reason = "retained as the fully checked boundary/reference composition path"
    )]
    pub fn visible_indices(&self) -> Vec<usize> {
        let query = normalize_search(&self.query);
        self.controls
            .iter()
            .enumerate()
            .filter_map(|(index, control)| {
                self.control_matches_normalized_filters(control, &query)
                    .then_some(index)
            })
            .collect()
    }

    pub fn compose_onto(&self, base: &OrderedObjMesh) -> Result<OrderedObjMesh, FormatError> {
        base.validate()?;
        let mut result = base.clone();
        for control in self
            .controls
            .iter()
            .filter(|control| control.canonical_active())
        {
            let target = control.target.resolved().ok_or_else(|| {
                FormatError::InvalidMorph(format!(
                    "resolved morph {:?} has no decoded target",
                    control.id
                ))
            })?;
            target.validate()?;
            if target.compatibility.vertex_count != result.vertices.len()
                || target.reference_faces().len() != result.faces.len()
                || target
                    .reference_faces()
                    .iter()
                    .zip(&result.faces)
                    .any(|(expected, actual)| expected.as_slice() != actual.vertex_indices)
            {
                return Err(FormatError::InvalidMorph(format!(
                    "morph {:?} is not bound to the fitted G2 topology",
                    control.id
                )));
            }
            let amount =
                target.checked_extrapolated_value(f64::from(control.value))? - target.default;
            for (position, delta) in result.vertices.iter_mut().zip(&target.deltas) {
                for axis in 0..3 {
                    position[axis] += amount * delta[axis];
                }
            }
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(
        dead_code,
        reason = "retained as the allocating reference path and regression oracle"
    )]
    pub fn compose_vertices_onto_bound(
        &self,
        base_vertices: &[[f64; 3]],
        bound_face_count: usize,
    ) -> Result<Vec<[f64; 3]>, FormatError> {
        let mut vertices = base_vertices.to_vec();
        self.apply_to_bound_vertices(&mut vertices, bound_face_count)?;
        Ok(vertices)
    }

    pub fn apply_to_bound_vertices(
        &self,
        vertices: &mut [[f64; 3]],
        bound_face_count: usize,
    ) -> Result<(), FormatError> {
        let mut candidate = vertices.to_vec();
        for control in self
            .controls
            .iter()
            .filter(|control| control.canonical_active())
        {
            validate_bound_control(control, vertices.len(), bound_face_count)?;
        }

        for control in self
            .controls
            .iter()
            .filter(|control| control.canonical_active())
        {
            let target = control
                .target
                .resolved()
                .expect("active control preflighted");
            let amount =
                target.checked_extrapolated_value(f64::from(control.value))? - target.default;
            for (position, delta) in candidate.iter_mut().zip(&target.deltas) {
                for axis in 0..3 {
                    let value = position[axis] + amount * delta[axis];
                    if !value.is_finite() {
                        return Err(FormatError::InvalidMorph(
                            "composed morph contains non-finite coordinates".to_owned(),
                        ));
                    }
                    position[axis] = value;
                }
            }
        }
        vertices.copy_from_slice(&candidate);
        Ok(())
    }

    pub fn genital_applications(&self) -> Result<Vec<GenitalMorphApplication>, FormatError> {
        self.controls
            .iter()
            .filter(|control| control.active())
            .filter_map(|control| {
                control.genital_target.as_ref().map(|target| {
                    let amount = f64::from(control.value) - control.target.default;
                    if !amount.is_finite() {
                        return Err(FormatError::InvalidMorph(format!(
                            "genital morph {:?} has a non-finite value",
                            control.id
                        )));
                    }
                    Ok(GenitalMorphApplication {
                        target: Arc::clone(target),
                        amount,
                    })
                })
            })
            .collect()
    }
}

pub fn apply_genital_applications_to_vam_mesh(
    provider: &VaMGeometryProvider,
    mesh: &mut OrderedObjMesh,
    applications: &[GenitalMorphApplication],
) -> Result<(), FormatError> {
    mesh.validate()?;
    if mesh.vertices.len() != provider.vam_basis().vertices.len()
        || mesh.faces != provider.vam_basis().faces
    {
        return Err(FormatError::InvalidMorph(
            "full VaM preview does not match the active provider topology".to_owned(),
        ));
    }
    for application in applications {
        application.target.validate_provider(provider)?;
        if application.target.full_vertex_count != mesh.vertices.len()
            || !application.amount.is_finite()
        {
            return Err(FormatError::InvalidMorph(
                "genital morph application has incompatible topology or value".to_owned(),
            ));
        }
    }

    let mut candidate = mesh.vertices.clone();
    for application in applications {
        for delta in application.target.deltas.iter() {
            let position = &mut candidate[delta.vertex_index as usize];
            let converted = [
                delta.delta_cm[0] / 100.0,
                delta.delta_cm[1] / 100.0,
                delta.delta_cm[2] / 100.0,
            ];
            for axis in 0..3 {
                let value = position[axis] + application.amount * converted[axis];
                if !value.is_finite() {
                    return Err(FormatError::InvalidMorph(
                        "composed genital morph contains non-finite coordinates".to_owned(),
                    ));
                }
                position[axis] = value;
            }
        }
    }
    mesh.vertices = candidate;
    mesh.validate()?;
    Ok(())
}

fn validate_bound_control(
    control: &MorphControl,
    vertex_count: usize,
    face_count: usize,
) -> Result<(), FormatError> {
    let target = control.target.resolved().ok_or_else(|| {
        FormatError::InvalidMorph(format!(
            "resolved morph {:?} has no decoded target",
            control.id
        ))
    })?;
    if !target.compatibility.topology_validated
        || !target.compatibility.face_order_matches
        || target.compatibility.vertex_count != vertex_count
        || target.compatibility.face_count != face_count
        || target.deltas.len() != vertex_count
    {
        return Err(FormatError::InvalidMorph(format!(
            "morph {:?} is not bound to the fitted G2 topology",
            control.id
        )));
    }
    target.checked_extrapolated_value(f64::from(control.value))?;
    Ok(())
}

pub fn canonical_identity(value: &str) -> String {
    let normalized = normalize_search(value);
    for prefix in ["ctrl", "phm", "fbm", "pbm"] {
        if let Some(stripped) = normalized.strip_prefix(prefix)
            && stripped.len() >= 3
        {
            return stripped.to_owned();
        }
    }
    normalized
}

fn normalize_search(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use vkit_core::{
        formats::{ObjMorphSource, load_obj_morph_target, parse_ordered_obj},
        vam::{
            GeometryFamily, TopologyFingerprint, VmbVertexRouting, decode_vmb_daz_cm_for_topology,
            encode_vmb_daz_cm,
        },
    };

    use super::*;

    fn target(key: &str, dx: f64) -> Arc<MorphTarget> {
        let base =
            load_ordered_obj_from_text("v 0 0 0\nv 1 0 0\nv 0 1 0\ng Face\nusemtl Face\nf 1 2 3\n");
        let target = format!("v {dx} 0 0\nv 1 0 0\nv 0 1 0\ng Face\nusemtl Face\nf 1 2 3\n");
        let loaded = parse_ordered_obj(Cursor::new(target)).unwrap();
        let base_faces = base
            .faces
            .iter()
            .map(|face| face.vertex_indices.clone())
            .collect::<Vec<_>>();
        Arc::new(
            load_obj_morph_target(
                key,
                ObjMorphSource {
                    target: &loaded,
                    base_vertices: &base.vertices,
                    base_faces: &base_faces,
                    unit_scale: Some(1.0),
                },
                1.0e-9,
                0.0,
                1.0,
                0.0,
            )
            .unwrap(),
        )
    }

    fn load_ordered_obj_from_text(text: &str) -> OrderedObjMesh {
        parse_ordered_obj(Cursor::new(text)).unwrap()
    }

    fn synthetic_receipt(sex: GeometrySex, full_vertex_count: usize) -> VaMGeometrySidecarReceipt {
        let shared = VAM_SHARED_BODY_VERTEX_COUNT as usize;
        VaMGeometrySidecarReceipt {
            schema_version: 1,
            anchor_family: GeometryFamily::DazGenesis2,
            basis_family: GeometryFamily::VirtAMate,
            basis_sex: sex,
            shared_vertex_count: shared,
            daz_topology: TopologyFingerprint {
                vertex_count: shared,
                polygon_count: 1,
                triangle_count: 1,
                sha256: [1; 32],
            },
            vam_topology: TopologyFingerprint {
                vertex_count: full_vertex_count,
                polygon_count: 2,
                triangle_count: 2,
                sha256: [2; 32],
            },
            daz_vertices_sha256: [3; 32],
            vam_basis_vertices_sha256: [4; 32],
            vam_face_labels_sha256: [5; 32],
            canonical_triangle_count: 1,
            shared_canonical_triangle_count: 1,
            replaced_canonical_triangle_count: 0,
            replacement_shared_triangle_count: 0,
            appended_face_count: 1,
            protected_shared_vertex_count: 0,
            metres_to_centimetres: 100.0,
            protected_epsilon_cm: 1.0e-6,
        }
    }

    #[test]
    fn packaged_and_local_morphs_filter_apart() {
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::VaMBuiltin,
            vec![MorphControl::new(
                "PHMBuiltin",
                "Builtin",
                MorphCategory::Head,
                MorphSource::VaMBuiltin,
                target("builtin", 1.0),
            )],
        );
        library.upsert_imported_vam(MorphControl::new(
            "vam-var:SomeCreator.Pack.1:/morph.vmi",
            "Somebody else's morph",
            MorphCategory::Head,
            MorphSource::PackagedVaM,
            target("packaged", 2.0),
        ));
        library.upsert_imported_vam(MorphControl::new(
            "vam-pair:C:/VaM/Custom/Atom/Person/Morphs/mine.vmi",
            "My own morph",
            MorphCategory::Head,
            MorphSource::LocalVaM,
            target("local", 3.0),
        ));

        let mut ids = |filter| {
            library.set_list_filter(filter);
            library
                .visible_indices()
                .into_iter()
                .map(|index| library.controls()[index].id.clone())
                .collect::<Vec<_>>()
        };

        assert_eq!(ids(MorphListFilter::All).len(), 3);
        assert!(ids(MorphListFilter::Active).is_empty());
    }

    #[test]
    fn curated_alias_replaces_same_vam_builtin() {
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::VaMBuiltin,
            vec![MorphControl::new(
                "PHMMouthOpenWide",
                "Mouth Open Wide",
                MorphCategory::Mouth,
                MorphSource::VaMBuiltin,
                target("builtin", 1.0),
            )],
        );
        library.replace_source(
            MorphSource::Curated,
            vec![MorphControl::new(
                "Mouth Open Wide",
                "Mouth Open Wide",
                MorphCategory::Mouth,
                MorphSource::Curated,
                target("curated", 2.0),
            )],
        );
        assert_eq!(library.controls().len(), 1);
        assert_eq!(library.controls()[0].source, MorphSource::Curated);
    }

    #[test]
    fn imported_vam_upsert_keeps_multiple_pairs_and_refreshes_only_matching_id() {
        let mut library = MorphLibrary::default();
        for (id, dx) in [("pair-a", 0.25), ("pair-b", 0.5)] {
            library.upsert_imported_vam(MorphControl::new(
                id,
                id,
                MorphCategory::Head,
                MorphSource::PackagedVaM,
                target(id, dx),
            ));
        }
        assert_eq!(library.controls().len(), 2);
        assert!(library.set_value("pair-a", 0.75));
        library.upsert_imported_vam(MorphControl::new(
            "pair-a",
            "pair-a refreshed",
            MorphCategory::Head,
            MorphSource::PackagedVaM,
            target("pair-a-refreshed", 0.75),
        ));
        assert_eq!(library.controls().len(), 2);
        assert_eq!(
            library
                .controls()
                .iter()
                .find(|control| control.id == "pair-a")
                .unwrap()
                .value,
            0.75
        );
        assert!(
            library
                .controls()
                .iter()
                .all(|control| control.source.is_imported())
        );
    }

    #[test]
    fn body_and_genital_pairs_coexist_without_canonical_index_contamination() {
        let shared = VAM_SHARED_BODY_VERTEX_COUNT as usize;
        let full_count = shared + 2;
        let local = [SparseDelta {
            vertex_index: 0,
            delta_cm: [0.5, -0.25, 0.125],
        }];
        let encoded = encode_vmb_daz_cm(&local, full_count - shared).unwrap();
        let routed =
            decode_vmb_daz_cm_for_topology(&encoded, full_count, Some(VmbVertexRouting::Genitalia))
                .unwrap();
        assert_eq!(routed[0].vertex_index as usize, shared);

        let genital_target = Arc::new(
            GenitalMorphTarget::new(
                GeometrySex::Female,
                synthetic_receipt(GeometrySex::Female, full_count),
                full_count,
                routed,
            )
            .unwrap(),
        );
        let mut library = MorphLibrary::default();
        library.upsert_imported_vam(MorphControl::new(
            "body-pair",
            "Body pair",
            MorphCategory::Head,
            MorphSource::PackagedVaM,
            target("body-pair", 2.0),
        ));
        library.upsert_imported_vam(
            MorphControl::new_genital_vam(
                "genital-pair",
                "Genital pair",
                MorphCategory::Head,
                genital_target,
                0.0,
                1.0,
                0.0,
            )
            .unwrap(),
        );
        assert!(library.set_value("body-pair", 0.5));
        assert!(library.set_value("genital-pair", 1.0));

        let canonical =
            load_ordered_obj_from_text("v 0 0 0\nv 1 0 0\nv 0 1 0\ng Face\nusemtl Face\nf 1 2 3\n");
        let composed = library.compose_onto(&canonical).unwrap();
        assert_eq!(composed.vertices[0], [1.0, 0.0, 0.0]);
        let applications = library.genital_applications().unwrap();
        assert_eq!(applications.len(), 1);
        assert_eq!(
            applications[0].target.deltas[0].vertex_index as usize,
            shared
        );
        assert_eq!(applications[0].amount, 1.0);

        assert!(
            GenitalMorphTarget::new(
                GeometrySex::Male,
                synthetic_receipt(GeometrySex::Female, full_count),
                full_count,
                vec![SparseDelta {
                    vertex_index: shared as u32,
                    delta_cm: [0.1, 0.0, 0.0],
                }],
            )
            .is_err()
        );
        assert!(
            GenitalMorphTarget::new(
                GeometrySex::Female,
                synthetic_receipt(GeometrySex::Female, full_count),
                full_count,
                vec![SparseDelta {
                    vertex_index: (shared - 1) as u32,
                    delta_cm: [0.1, 0.0, 0.0],
                }],
            )
            .is_err()
        );
    }

    #[test]
    fn composition_is_rebuilt_from_the_supplied_base() {
        let base =
            load_ordered_obj_from_text("v 0 0 0\nv 1 0 0\nv 0 1 0\ng Face\nusemtl Face\nf 1 2 3\n");
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::Curated,
            vec![MorphControl::new(
                "smile",
                "Smile",
                MorphCategory::Expression,
                MorphSource::Curated,
                target("smile", 2.0),
            )],
        );
        assert!(library.set_value("smile", 0.5));
        let first = library.compose_onto(&base).unwrap();
        let second = library.compose_onto(&base).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.vertices[0][0], 1.0);
        assert_eq!(
            library
                .compose_vertices_onto_bound(&base.vertices, base.faces.len())
                .unwrap(),
            first.vertices
        );
    }

    #[test]
    fn in_place_composition_is_bit_identical_and_preflight_is_transactional() {
        let base =
            load_ordered_obj_from_text("v 0 0 0\nv 1 0 0\nv 0 1 0\ng Face\nusemtl Face\nf 1 2 3\n");
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::Curated,
            vec![MorphControl::new(
                "smile",
                "Smile",
                MorphCategory::Expression,
                MorphSource::Curated,
                target("smile", 2.0),
            )],
        );
        assert!(library.set_value("smile", 0.375));

        let allocating = library
            .compose_vertices_onto_bound(&base.vertices, base.faces.len())
            .unwrap();
        let mut in_place = base.vertices.clone();
        library
            .apply_to_bound_vertices(&mut in_place, base.faces.len())
            .unwrap();
        let allocating_bits = allocating
            .iter()
            .map(|point| point.map(f64::to_bits))
            .collect::<Vec<_>>();
        let in_place_bits = in_place
            .iter()
            .map(|point| point.map(f64::to_bits))
            .collect::<Vec<_>>();
        assert_eq!(in_place_bits, allocating_bits);

        let mut rejected = base.vertices.clone();
        let before = rejected.clone();
        assert!(
            library
                .apply_to_bound_vertices(&mut rejected, base.faces.len() + 1)
                .is_err()
        );
        assert_eq!(rejected, before);
    }

    #[test]
    fn category_and_search_filters_compose() {
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::Curated,
            vec![
                MorphControl::new(
                    "eye_height",
                    "Eye Height",
                    MorphCategory::Eyes,
                    MorphSource::Curated,
                    target("eye_height", 1.0),
                ),
                MorphControl::new(
                    "mouth_smile",
                    "Mouth Smile",
                    MorphCategory::Mouth,
                    MorphSource::Curated,
                    target("mouth_smile", 1.0),
                ),
            ],
        );
        library.category_filter = MorphCategoryFilter::Category(MorphCategory::Mouth);
        library.query = "smile".into();
        assert_eq!(library.visible_indices(), vec![1]);
    }

    #[test]
    fn numeric_over_morph_widens_live_slider_without_changing_authored_bounds() {
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::Curated,
            vec![MorphControl::new(
                "smile",
                "Smile",
                MorphCategory::Expression,
                MorphSource::Curated,
                target("smile", 1.0),
            )],
        );

        let control = &library.controls()[0];
        assert_eq!(control.target.authored_bounds(), (0.0, 1.0));
        assert_eq!(control.target.entry_bounds(), (-2.0, 2.0));
        assert_eq!(
            control.target.slider_bounds(f64::from(control.value)),
            (0.0, 1.0)
        );

        assert!(library.set_value("smile", 1.5));
        let control = &library.controls()[0];
        assert_eq!(control.target.authored_bounds(), (0.0, 1.0));
        assert_eq!(
            control.target.slider_bounds(f64::from(control.value)),
            (0.0, 1.5)
        );

        assert!(library.set_value("smile", 8.0));
        let control = &library.controls()[0];
        assert_eq!(control.value, 2.0);
        assert_eq!(
            control.target.slider_bounds(f64::from(control.value)),
            (0.0, 2.0)
        );
    }

    #[test]
    fn modified_only_composes_with_category_query_and_genital_sidecars() {
        let shared = VAM_SHARED_BODY_VERTEX_COUNT as usize;
        let full_count = shared + 2;
        let genital = Arc::new(
            GenitalMorphTarget::new(
                GeometrySex::Female,
                synthetic_receipt(GeometrySex::Female, full_count),
                full_count,
                vec![SparseDelta {
                    vertex_index: shared as u32,
                    delta_cm: [0.1, 0.0, 0.0],
                }],
            )
            .unwrap(),
        );
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::Curated,
            vec![
                MorphControl::new(
                    "eye_height",
                    "Eye Height",
                    MorphCategory::Eyes,
                    MorphSource::Curated,
                    target("eye_height", 1.0),
                ),
                MorphControl::new(
                    "mouth_smile",
                    "Mouth Smile",
                    MorphCategory::Mouth,
                    MorphSource::Curated,
                    target("mouth_smile", 1.0),
                ),
            ],
        );
        library.upsert_imported_vam(
            MorphControl::new_genital_vam(
                "vam:genital-sidecar",
                "Genital Sidecar",
                MorphCategory::Mouth,
                genital,
                0.0,
                1.0,
                0.0,
            )
            .unwrap(),
        );
        assert!(library.set_value("eye_height", 0.5));
        assert!(library.set_value("vam:genital-sidecar", 1.0));
        let sidecar = library
            .controls()
            .iter()
            .find(|control| control.id == "vam:genital-sidecar")
            .unwrap();
        assert!(sidecar.is_resolved());
        assert!(sidecar.is_modified());
        assert!(sidecar.active());

        assert!(library.set_list_filter(MorphListFilter::Active));
        assert!(library.modified_only());
        assert!(library.has_modified_controls());
        let visible = library
            .visible_indices()
            .into_iter()
            .map(|index| library.controls()[index].id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(visible, vec!["vam:genital-sidecar", "eye_height"]);

        library.category_filter = MorphCategoryFilter::Category(MorphCategory::Mouth);
        library.query = "sidecar".into();
        assert_eq!(
            library
                .visible_indices()
                .into_iter()
                .map(|index| library.controls()[index].id.as_str())
                .collect::<Vec<_>>(),
            vec!["vam:genital-sidecar"]
        );
        library.query = "smile".into();
        assert!(library.visible_indices().is_empty());

        assert!(library.reset());
        assert!(library.modified_only());
        assert!(!library.has_modified_controls());
        assert!(library.visible_indices().is_empty());
    }

    #[test]
    fn value_snapshot_restores_every_control_bit_exactly_after_reset() {
        let mut library = MorphLibrary::default();
        library.replace_source(
            MorphSource::Curated,
            vec![
                MorphControl::new(
                    "smile",
                    "Smile",
                    MorphCategory::Expression,
                    MorphSource::Curated,
                    target("smile", 1.0),
                ),
                MorphControl::new(
                    "jaw_width",
                    "Jaw Width",
                    MorphCategory::Jaw,
                    MorphSource::Curated,
                    target("jaw_width", 1.0),
                ),
            ],
        );
        assert!(library.set_value("smile", 0.375));
        assert!(library.set_value("jaw_width", 0.625));
        let before = library
            .controls()
            .iter()
            .map(|control| (control.id.clone(), control.value.to_bits()))
            .collect::<BTreeMap<_, _>>();
        let snapshot = library.snapshot_values();

        assert!(library.reset());
        assert!(library.restore_values(&snapshot));
        let restored = library
            .controls()
            .iter()
            .map(|control| (control.id.clone(), control.value.to_bits()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(restored, before);
    }

    #[test]
    fn canonical_identity_normalizes_common_daz_prefixes() {
        assert_eq!(canonical_identity("PHM Mouth_Open-Wide"), "mouthopenwide");
        assert_eq!(canonical_identity("Mouth Open Wide"), "mouthopenwide");
    }

    #[test]
    fn unresolved_vam_control_loads_once_and_never_composes_its_slider_contract() {
        let base =
            load_ordered_obj_from_text("v 0 0 0\nv 1 0 0\nv 0 1 0\ng Face\nusemtl Face\nf 1 2 3\n");
        let mut library = MorphLibrary::default();
        let topology_template = target("slider_contract", 9.0);
        library.replace_source(
            MorphSource::VaMBuiltin,
            vec![MorphControl::new_unresolved(
                "PHMTest",
                "Test",
                MorphCategory::Mouth,
                MorphSource::VaMBuiltin,
                unresolved_slider_contract(&topology_template, "slider_contract", 0.0, 1.0, 0.0)
                    .unwrap(),
            )],
        );
        assert!(library.needs_resolution("PHMTest"));
        assert!(library.begin_resolution("PHMTest", 0.5));
        assert!(!library.begin_resolution("PHMTest", 0.75));
        let pending = &library.controls()[0];
        assert!(pending.is_modified());
        assert!(!pending.is_resolved());
        assert!(!pending.active());
        assert_eq!(library.compose_onto(&base).unwrap(), base);
        assert!(
            library
                .install_resolved_target("PHMTest", target("resolved", 2.0))
                .unwrap()
        );
        assert!(!library.needs_resolution("PHMTest"));
        assert_eq!(library.compose_onto(&base).unwrap().vertices[0][0], 1.5);
    }

    #[test]
    fn unresolved_slider_contract_preserves_signed_source_range() {
        let template = target("template", 1.0);
        let contract =
            unresolved_slider_contract(&template, "vam:signed", -2.0, 3.0, -0.25).unwrap();
        assert_eq!(contract.minimum, -2.0);
        assert_eq!(contract.maximum, 3.0);
        assert_eq!(contract.default, -0.25);
        assert!(!contract.is_resolved());
    }

    #[test]
    fn unresolved_controls_do_not_retain_dense_topology_or_delta_arrays() {
        let template = target("template", 1.0);
        let controls = (0..571)
            .map(|index| {
                MorphControl::new_unresolved(
                    format!("vam:{index}"),
                    format!("VaM {index}"),
                    MorphCategory::Eyes,
                    MorphSource::VaMBuiltin,
                    unresolved_slider_contract(&template, format!("vam:{index}"), 0.0, 1.0, 0.0)
                        .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(Arc::strong_count(&template), 1);
        assert!(controls.iter().all(|control| !control.target.is_resolved()));
    }

    #[test]
    fn a_side_word_inside_a_longer_word_is_not_a_side() {
        for centred in [
            "ChinCleft",
            "PHMChinCleft",
            "TM_Upright1-db5c6ba7",
            "TM_UprightSmoother-2a97977c",
            "Brow Height",
            "Lips Pucker (REN)",
            "Eyes Closed (REN)",
            "LIps Top Width",
            "PHMNoseSide-Side",
            "Mouth Side Crease B",
        ] {
            assert_eq!(
                horizontal_symmetry(centred),
                HorizontalSymmetry::Mirrored,
                "{centred} moves both sides; hiding it would take a centre-line morph off the list"
            );
        }

        for sided in [
            "Cheek Squish Left",
            "Cheek Squish Right",
            "Iris Placement L",
            "BrowD_L",
            "EyeSquint_R",
            "BK Breast Diameter L",
            "LT Areola S2S R",
            "PHMSnarlLeft",
            "PHMSnarlRight",
            "JawLeft",
            "PBMBicepsLeftSWM",
            "TM_RollRight1-fc5737e5",
            "PHMEyesSizeL",
            "PHMEyesSizeR",
            "PHMMouthSmileSimpleL",
            "PHMMouthSmileSimpleR",
            "PHMBrowDefineL",
            "PHMCheeksDimpleCreaseR",
            "PHMEyelidsUpperHeightL",
            "PHMNostrilsWidthR",
            "PHMFlirtingFeminineL",
            "PHMTemplesDefineR",
        ] {
            assert!(
                horizontal_symmetry(sided).is_one_sided(),
                "{sided} moves one side alone and should hide with the toggle off"
            );
        }
    }

    #[test]
    fn the_rule_agrees_with_the_real_built_in_list() {
        let manifests = [
            include_str!("../resources/g2f-builtin-morph-names.txt"),
            include_str!("../resources/g2m-builtin-morph-names.txt"),
        ];
        let names: std::collections::BTreeSet<&str> = manifests
            .iter()
            .flat_map(|file| file.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let sided: Vec<&str> = names
            .iter()
            .copied()
            .filter(|name| horizontal_symmetry(name).is_one_sided())
            .collect();

        let twin_of = |name: &str| -> Option<String> {
            if let Some(stem) = name.strip_suffix('L') {
                return Some(format!("{stem}R"));
            }
            if let Some(stem) = name.strip_suffix('R') {
                return Some(format!("{stem}L"));
            }
            if name.contains("Left") {
                return Some(name.replace("Left", "Right"));
            }
            if name.contains("Right") {
                return Some(name.replace("Right", "Left"));
            }
            None
        };
        let lone: Vec<&str> = sided
            .iter()
            .copied()
            .filter(|name| twin_of(name).is_none_or(|twin| !names.contains(twin.as_str())))
            .collect();
        assert_eq!(
            lone,
            ["Iris Placement L"],
            "a one-sided morph with no opposite number is either a naming quirk or a false \
             positive; only the known quirk should be here"
        );

        assert_eq!(
            sided.len(),
            81,
            "the built-in set holds 81 one-sided morphs of {}; if this number moved, the rule \
             changed what it catches and the new spelling wants a case in the tests above",
            names.len()
        );
    }

    #[test]
    fn the_list_the_interface_draws_honours_the_side_toggle() {
        let mut library = MorphLibrary {
            controls: vec![
                control_named("Brow Inner Up", MorphCategory::Brows),
                control_named("PHMBrowsInnerUpL", MorphCategory::Brows),
                control_named("PHMBrowsInnerUpR", MorphCategory::Brows),
            ],
            ..MorphLibrary::default()
        };
        let listed = |library: &MorphLibrary| {
            library
                .controls()
                .iter()
                .filter(|control| library.control_passes_non_text_filters(control))
                .count()
        };

        library.show_one_sided = true;
        assert_eq!(listed(&library), 3, "the toggle on lists every morph");

        library.show_one_sided = false;
        assert_eq!(
            listed(&library),
            1,
            "the toggle off has to drop the left and right pair from the list"
        );

        assert_eq!(library.visible_indices().len(), listed(&library));
    }

    #[test]
    fn the_side_filter_outlives_a_category_change() {
        let mut library = MorphLibrary {
            controls: vec![
                control_named("Cheek Squish Left", MorphCategory::Cheeks),
                control_named("ChinCleft", MorphCategory::Cheeks),
                control_named("BrowD_L", MorphCategory::Brows),
            ],
            ..MorphLibrary::default()
        };

        assert!(
            !library.show_one_sided,
            "a fresh library leaves the one-sided morphs out"
        );
        assert_eq!(
            library.visible_indices(),
            vec![1],
            "only the centre-line morph is listed until they are asked for"
        );

        library.show_one_sided = true;
        assert_eq!(library.visible_indices().len(), 3);

        library.show_one_sided = false;

        library.category_filter = MorphCategoryFilter::Category(MorphCategory::Brows);
        assert!(
            library.visible_indices().is_empty(),
            "the one brow morph is one-sided, so the brow category is empty while the toggle is off"
        );
    }

    fn control_named(label: &str, category: MorphCategory) -> MorphControl {
        MorphControl {
            id: label.to_owned(),
            label: label.to_owned(),
            category,
            source: MorphSource::VaMBuiltin,
            symmetry: horizontal_symmetry(label),
            value: 0.0,
            target: MorphTargetBinding::unresolved(label, 0.0, 1.0, 0.0)
                .expect("a 0..1 slider range is valid"),
            genital_target: None,
            availability: MorphAvailability::Resolved,
            unsupported_formula_count: 0,
        }
    }
}
