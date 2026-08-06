use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

pub use vkit_core::sculpt::*;

use glam::DVec3;
use vkit_core::formats::OrderedObjMesh;

const MAX_SCULPT_HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SculptBrush {
    #[default]
    Move,

    Smooth,

    Restore,
}

impl SculptBrush {
    pub const ALL: [Self; 3] = [Self::Move, Self::Smooth, Self::Restore];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SculptDab {
    pub center_local: [f64; 3],
    pub radius_local: f64,
    pub strength: f64,
    pub operation: SculptOperation,
}

#[derive(Debug)]
enum SculptUndoStep {
    Stroke {
        before: Vec<(u32, [f64; 3])>,
        smooth_reference_before: Vec<(u32, Option<f64>)>,
    },

    Reset {
        working: OrderedObjMesh,
        smooth_reference_areas: BTreeMap<u32, f64>,
        last_hit_triangle: Option<u32>,
        last_hit_view_direction: Option<[f64; 3]>,
        last_hit_anchor: Option<StrokeAnchor>,
        applied: bool,
        changed: bool,
    },
}

#[derive(Debug)]
struct ActiveStroke {
    before: BTreeMap<u32, [f64; 3]>,

    smooth_reference_before: BTreeMap<u32, Option<f64>>,
    seed_triangle: Option<u32>,

    primary_fixed_influence: Option<Vec<BrushInfluence>>,
    mirrored_fixed_influence: Option<Vec<BrushInfluence>>,

    anchor: Option<StrokeAnchor>,

    brush_sample: Option<CapturedBrushSample>,

    view_direction: Option<DVec3>,
    brush_direction: Option<DVec3>,
    backface_masking: bool,
    falloff: SculptFalloff,
    targets: SculptTargets,
    x_symmetry: bool,

    connected_topology_only: bool,
}

impl ActiveStroke {
    fn incoming_direction(&self) -> Option<DVec3> {
        self.brush_direction.or(self.view_direction)
    }
}

#[derive(Clone, Copy, Debug)]
struct CapturedBrushSample {
    radius: f64,
    strength: f64,
    operation: SculptOperationKind,
}

#[derive(Debug)]
pub struct SculptSession {
    baseline: Option<Arc<OrderedObjMesh>>,
    working: Option<OrderedObjMesh>,

    presentation_vertices: Option<Vec<[f64; 3]>>,
    topology: Option<Arc<SculptTopology>>,

    restore_basis: Option<Vec<[f64; 3]>>,

    x_mirror_vertices: Option<Arc<Vec<usize>>>,

    targets: SculptTargets,

    visible_targets: SculptTargets,

    solo: Option<(SculptTarget, SculptTargets)>,

    history: VecDeque<(u64, SculptUndoStep)>,
    active_stroke: Option<ActiveStroke>,

    smooth_reference_areas: BTreeMap<u32, f64>,
    falloff: SculptFalloff,
    backface_masking: bool,
    x_symmetry: bool,

    connected_topology_only: bool,

    last_hit_triangle: Cell<Option<u32>>,

    last_hit_view_direction: Cell<Option<[f64; 3]>>,

    last_hit_anchor: Cell<Option<StrokeAnchor>>,

    influence_scratch: InfluenceScratch,
    applied: bool,
    changed: bool,
}

impl Default for SculptSession {
    fn default() -> Self {
        Self {
            baseline: None,
            working: None,
            presentation_vertices: None,
            topology: None,
            restore_basis: None,
            x_mirror_vertices: None,
            targets: SculptTargets::default(),
            visible_targets: SculptTargets::ALL,
            solo: None,
            history: VecDeque::new(),
            active_stroke: None,
            smooth_reference_areas: BTreeMap::new(),
            falloff: SculptFalloff::default(),
            backface_masking: false,
            x_symmetry: true,
            connected_topology_only: false,
            last_hit_triangle: Cell::new(None),
            last_hit_view_direction: Cell::new(None),
            last_hit_anchor: Cell::new(None),
            influence_scratch: InfluenceScratch::default(),
            applied: false,
            changed: false,
        }
    }
}

impl SculptSession {
    pub fn begin(&mut self, mesh: &OrderedObjMesh) -> Result<(), SculptError> {
        let topology = Arc::new(SculptTopology::build(mesh)?);
        self.baseline = Some(Arc::new(mesh.clone()));
        self.working = Some(mesh.clone());
        self.presentation_vertices = None;
        self.topology = Some(topology);
        self.restore_basis = None;
        self.x_mirror_vertices = None;
        self.targets = SculptTargets::default();
        self.visible_targets = SculptTargets::ALL;
        self.solo = None;
        self.history.clear();
        self.active_stroke = None;
        self.smooth_reference_areas.clear();
        self.last_hit_triangle.set(None);
        self.last_hit_view_direction.set(None);
        self.last_hit_anchor.set(None);
        self.applied = false;
        self.changed = false;
        Ok(())
    }

    pub fn load_applied(&mut self, mesh: &OrderedObjMesh) -> Result<(), SculptError> {
        self.begin(mesh)?;
        self.applied = true;
        Ok(())
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub const fn is_ready(&self) -> bool {
        self.working.is_some()
    }

    pub const fn is_applied(&self) -> bool {
        self.applied
    }

    #[cfg(test)]
    pub const fn has_changes(&self) -> bool {
        self.changed
    }

    pub const fn editable_targets(&self) -> SculptTargets {
        self.targets
    }

    pub const fn visible_targets(&self) -> SculptTargets {
        self.visible_targets
    }

    pub const fn falloff_preset(&self) -> SculptFalloff {
        self.falloff
    }

    pub fn set_falloff_preset(&mut self, falloff: SculptFalloff) {
        self.falloff = falloff;
    }

    pub const fn backface_masking(&self) -> bool {
        self.backface_masking
    }

    pub fn set_backface_masking(&mut self, enabled: bool) {
        self.backface_masking = enabled;
    }

    #[cfg(test)]
    pub const fn x_symmetry(&self) -> bool {
        self.x_symmetry
    }

    pub fn set_x_symmetry(&mut self, enabled: bool) {
        self.x_symmetry = enabled;
    }

    #[cfg(test)]
    pub const fn connected_topology_only(&self) -> bool {
        self.connected_topology_only
    }

    pub fn set_connected_topology_only(&mut self, enabled: bool) {
        self.connected_topology_only = enabled;
    }

    #[cfg(test)]
    pub fn set_visible_targets(&mut self, targets: SculptTargets) {
        self.visible_targets = targets;
    }

    pub fn set_restore_basis(&mut self, basis: Option<Vec<[f64; 3]>>) -> Result<(), SculptError> {
        let Some(basis) = basis else {
            self.restore_basis = None;
            self.x_mirror_vertices = None;
            return Ok(());
        };
        let working = self.working.as_ref().ok_or(SculptError::NotInitialized)?;
        if basis.len() != working.vertices.len()
            || basis
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
        {
            self.restore_basis = None;
            return Err(SculptError::InvalidMesh(
                "restore basis does not match the sculpt topology".to_owned(),
            ));
        }
        self.x_mirror_vertices = Some(Arc::new(build_x_mirror_vertex_map(&basis)));
        self.restore_basis = Some(basis);
        Ok(())
    }

    #[cfg(test)]
    pub fn has_restore_basis(&self) -> bool {
        self.restore_basis.is_some()
    }

    pub fn set_target_enabled(&mut self, target: SculptTarget, enabled: bool) {
        self.set_editable_target_enabled(target, enabled);
    }

    pub fn set_editable_target_enabled(&mut self, target: SculptTarget, enabled: bool) {
        self.targets = if enabled {
            self.targets.with(target)
        } else {
            self.targets.without(target)
        };
    }

    #[cfg(test)]
    pub fn toggle_editable_target(&mut self, target: SculptTarget) -> bool {
        let enabled = !self.targets.contains(target);
        self.set_editable_target_enabled(target, enabled);
        enabled
    }

    pub fn set_visible_target_enabled(&mut self, target: SculptTarget, enabled: bool) {
        self.visible_targets = if enabled {
            self.visible_targets.with(target)
        } else {
            self.visible_targets.without(target)
        };

        self.solo = None;
    }

    pub const fn soloed_target(&self) -> Option<SculptTarget> {
        match self.solo {
            Some((target, _)) => Some(target),
            None => None,
        }
    }

    pub fn toggle_solo_target(&mut self, target: SculptTarget) {
        match self.solo.take() {
            Some((current, restore)) if current == target => self.visible_targets = restore,
            Some((_, restore)) => {
                self.visible_targets = SculptTargets::NONE.with(target);
                self.solo = Some((target, restore));
            }
            None => {
                let restore = self.visible_targets;
                self.visible_targets = SculptTargets::NONE.with(target);
                self.solo = Some((target, restore));
            }
        }
    }

    pub fn working_mesh(&self) -> Option<&OrderedObjMesh> {
        self.working.as_ref()
    }

    pub fn displacement(&self) -> Option<Vec<[f64; 3]>> {
        let (baseline, working) = (self.baseline.as_ref()?, self.working.as_ref()?);
        if baseline.vertices.len() != working.vertices.len() {
            return None;
        }
        Some(
            baseline
                .vertices
                .iter()
                .zip(&working.vertices)
                .map(|(from, to)| [to[0] - from[0], to[1] - from[1], to[2] - from[2]])
                .collect(),
        )
    }

    pub fn graft_displacement(&mut self, delta: &[[f64; 3]]) -> Result<bool, SculptError> {
        let baseline = self.baseline.as_ref().ok_or(SculptError::NotInitialized)?;
        let working = self.working.as_mut().ok_or(SculptError::NotInitialized)?;
        if delta.len() != baseline.vertices.len() || delta.len() != working.vertices.len() {
            return Err(SculptError::NotInitialized);
        }
        let mut moved = false;
        for ((vertex, from), offset) in working
            .vertices
            .iter_mut()
            .zip(&baseline.vertices)
            .zip(delta)
        {
            for axis in 0..3 {
                let target = from[axis] + offset[axis];
                moved |= target.to_bits() != vertex[axis].to_bits();
                *vertex = {
                    let mut next = *vertex;
                    next[axis] = target;
                    next
                };
            }
        }
        self.presentation_vertices = None;
        self.history.clear();
        self.active_stroke = None;
        self.smooth_reference_areas.clear();
        self.changed |= moved;
        Ok(moved)
    }

    pub fn set_presentation_vertices(&mut self, vertices: &[[f64; 3]]) -> Result<(), SculptError> {
        let working = self.working.as_ref().ok_or(SculptError::NotInitialized)?;
        if vertices.len() != working.vertices.len()
            || vertices
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
        {
            return Err(SculptError::InvalidMesh(
                "morph presentation coordinates do not match the sculpt basis".to_owned(),
            ));
        }
        self.presentation_vertices =
            (vertices != working.vertices.as_slice()).then(|| vertices.to_vec());
        Ok(())
    }

    pub fn clear_presentation_vertices(&mut self) {
        self.presentation_vertices = None;
    }

    #[cfg(test)]
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn top_undo_seq(&self) -> Option<u64> {
        if self.active_stroke.is_some() {
            return Some(u64::MAX);
        }
        self.history.back().map(|(seq, _)| *seq)
    }

    pub fn begin_stroke(&mut self) -> Result<(), SculptError> {
        self.begin_stroke_with_directions(self.last_hit_view_direction.get(), None)
    }

    pub fn begin_stroke_with_directions(
        &mut self,
        view_direction_local: Option<[f64; 3]>,
        brush_direction_local: Option<[f64; 3]>,
    ) -> Result<(), SculptError> {
        if self.working.is_none() {
            return Err(SculptError::NotInitialized);
        }
        if self.active_stroke.is_some() {
            return Err(SculptError::StrokeAlreadyActive);
        }
        let view_direction = normalize_stroke_direction(
            view_direction_local.or_else(|| self.last_hit_view_direction.get()),
        )?;
        let brush_direction = normalize_stroke_direction(brush_direction_local)?;
        self.active_stroke = Some(ActiveStroke {
            before: BTreeMap::new(),
            smooth_reference_before: BTreeMap::new(),
            seed_triangle: self.last_hit_triangle.get(),
            primary_fixed_influence: None,
            mirrored_fixed_influence: None,
            anchor: self.last_hit_anchor.get(),
            brush_sample: None,
            view_direction,
            brush_direction,
            backface_masking: self.backface_masking,
            falloff: self.falloff,
            targets: self.targets,
            x_symmetry: self.x_symmetry,
            connected_topology_only: self.connected_topology_only,
        });
        Ok(())
    }

    pub fn dab(&mut self, dab: SculptDab) -> Result<usize, SculptError> {
        validate_dab(dab)?;
        let operation_kind = SculptOperationKind::of(dab.operation);
        let (
            sample,
            stroke_seed,
            stored_anchor,
            stored_primary_influence,
            stored_mirrored_influence,
            stroke_falloff,
            stroke_backface_masking,
            stroke_incoming_direction,
            stroke_targets,
            stroke_x_symmetry,
            stroke_connected_topology_only,
        ) = {
            let stroke = self
                .active_stroke
                .as_mut()
                .ok_or(SculptError::NoActiveStroke)?;
            let sample = match stroke.brush_sample {
                Some(sample) if sample.operation != operation_kind => {
                    return Err(SculptError::InvalidDab);
                }
                Some(sample) => sample,
                None => {
                    let sample = CapturedBrushSample {
                        radius: dab.radius_local,
                        strength: dab.strength,
                        operation: operation_kind,
                    };
                    stroke.brush_sample = Some(sample);
                    sample
                }
            };
            (
                sample,
                stroke.seed_triangle,
                stroke.anchor,
                stroke.primary_fixed_influence.clone(),
                stroke.mirrored_fixed_influence.clone(),
                stroke.falloff,
                stroke.backface_masking,
                stroke.incoming_direction(),
                stroke.targets,
                stroke.x_symmetry,
                stroke.connected_topology_only,
            )
        };
        if stroke_targets.is_empty() {
            return Ok(0);
        }

        if matches!(dab.operation, SculptOperation::Restore) && self.restore_basis.is_none() {
            return Ok(0);
        }

        let topology = Arc::clone(self.topology.as_ref().ok_or(SculptError::NotInitialized)?);
        let working = self.working.as_ref().ok_or(SculptError::NotInitialized)?;
        let interaction_vertices = self
            .presentation_vertices
            .as_deref()
            .unwrap_or(&working.vertices);
        let x_mirror_vertices = self.x_mirror_vertices.clone();
        let influence_scratch = &mut self.influence_scratch;
        let input_center = DVec3::from_array(dab.center_local);
        let current_hit_seed = self.last_hit_triangle.get();
        let is_smooth = operation_kind == SculptOperationKind::Smooth;

        let is_fixed = matches!(
            operation_kind,
            SculptOperationKind::Grab | SculptOperationKind::Inflate
        );

        let mut anchor = stored_anchor;
        if operation_kind == SculptOperationKind::Inflate && anchor.is_none() {
            anchor = self.last_hit_anchor.get().or_else(|| {
                let seed = current_hit_seed
                    .or(stroke_seed)
                    .filter(|&triangle| (triangle as usize) < topology.triangles.len())
                    .or_else(|| {
                        nearest_target_triangle(
                            interaction_vertices,
                            &topology,
                            input_center,
                            stroke_targets,
                        )
                    })?;
                stroke_anchor_from_seed(interaction_vertices, &topology, input_center, seed)
            });
        }

        let anchored_center = anchor.map_or(input_center, |anchor| anchor.point);
        let primary_seed = match operation_kind {
            SculptOperationKind::Inflate => anchor.map(|anchor| anchor.seed_triangle),
            SculptOperationKind::Grab => stroke_seed.or(current_hit_seed),
            SculptOperationKind::Smooth | SculptOperationKind::Restore => current_hit_seed,
        }
        .filter(|&triangle| (triangle as usize) < topology.triangles.len())
        .or_else(|| {
            nearest_target_triangle(
                interaction_vertices,
                &topology,
                if operation_kind == SculptOperationKind::Inflate {
                    anchored_center
                } else {
                    input_center
                },
                stroke_targets,
            )
        });
        let primary_center = match operation_kind {
            SculptOperationKind::Inflate => anchored_center,

            SculptOperationKind::Grab => primary_seed
                .and_then(|seed| {
                    nearest_target_vertex_position(
                        interaction_vertices,
                        &topology,
                        input_center,
                        seed,
                        stroke_targets,
                    )
                })
                .unwrap_or(input_center),
            SculptOperationKind::Smooth | SculptOperationKind::Restore => input_center,
        };

        let primary_settings = InfluenceSettings {
            radius: sample.radius,

            strength: sample.strength,
            targets: stroke_targets,
            falloff: stroke_falloff,
            backface_masking: stroke_backface_masking,
            incoming_direction: stroke_incoming_direction,
            include_nearby_components: operation_kind == SculptOperationKind::Grab
                && !stroke_connected_topology_only,
        };
        let mirrored_settings = InfluenceSettings {
            incoming_direction: stroke_incoming_direction.map(mirror_x_vector),
            ..primary_settings
        };

        let mut primary_influence = stored_primary_influence;
        let mut mirrored_influence = stored_mirrored_influence;
        if !is_fixed || primary_influence.is_none() {
            primary_influence = primary_seed.map(|seed| {
                weighted_sculpt_influence_with_scratch(
                    interaction_vertices,
                    &topology,
                    primary_center,
                    seed,
                    primary_settings,
                    influence_scratch,
                )
            });
        }
        if stroke_x_symmetry
            && (!is_fixed || mirrored_influence.is_none())
            && x_mirror_vertices.is_some()
        {
            mirrored_influence = primary_influence.as_ref().map(|influence| {
                mirror_influence_by_topology(
                    influence,
                    x_mirror_vertices.as_deref().expect("checked above"),
                )
            });
        } else if stroke_x_symmetry && (!is_fixed || mirrored_influence.is_none()) {
            let mirrored_center = mirror_x_point(primary_center);
            mirrored_influence = nearest_target_triangle(
                interaction_vertices,
                &topology,
                mirrored_center,
                stroke_targets,
            )
            .map(|seed| {
                weighted_sculpt_influence_with_scratch(
                    interaction_vertices,
                    &topology,
                    mirrored_center,
                    seed,
                    mirrored_settings,
                    influence_scratch,
                )
            });
        } else if !stroke_x_symmetry {
            mirrored_influence = None;
        }

        let primary_influence = primary_influence.unwrap_or_default();
        let mirrored_influence = mirrored_influence.unwrap_or_default();
        let mut reference_updates = BTreeMap::new();
        if is_smooth {
            for (triangle, area) in
                smooth_reference_area_updates(&working.vertices, &topology, &primary_influence)
                    .into_iter()
                    .chain(smooth_reference_area_updates(
                        &working.vertices,
                        &topology,
                        &mirrored_influence,
                    ))
            {
                reference_updates.entry(triangle).or_insert(area);
            }
        }

        if is_fixed {
            let stroke = self
                .active_stroke
                .as_mut()
                .ok_or(SculptError::NoActiveStroke)?;
            stroke.seed_triangle = primary_seed;
            stroke.anchor = anchor;
            if stroke.primary_fixed_influence.is_none() {
                stroke.primary_fixed_influence = Some(primary_influence.clone());
            }
            if stroke.x_symmetry && stroke.mirrored_fixed_influence.is_none() {
                stroke.mirrored_fixed_influence = Some(mirrored_influence.clone());
            }
        }
        for (triangle, area) in reference_updates {
            if !self.smooth_reference_areas.contains_key(&triangle) {
                self.active_stroke
                    .as_mut()
                    .ok_or(SculptError::NoActiveStroke)?
                    .smooth_reference_before
                    .entry(triangle)
                    .or_insert(None);
                self.smooth_reference_areas.insert(triangle, area);
            }
        }

        let working = self.working.as_ref().ok_or(SculptError::NotInitialized)?;

        let restore_basis = match dab.operation {
            SculptOperation::RestoreFit => self
                .baseline
                .as_ref()
                .map(|baseline| baseline.vertices.as_slice()),
            _ => self.restore_basis.as_deref(),
        };
        let primary_normal = anchor.map(|anchor| anchor.normal);
        let mut primary_proposals = sculpt_branch_proposals(
            SculptBranch {
                operation: dab.operation,
                anchored_normal: primary_normal,
                restore_basis,
            },
            &working.vertices,
            &topology,
            stroke_targets,
            &primary_influence,
            &self.smooth_reference_areas,
        );
        let mut mirrored_proposals = if stroke_x_symmetry {
            sculpt_branch_proposals(
                SculptBranch {
                    operation: mirror_x_operation(dab.operation),
                    anchored_normal: primary_normal.map(mirror_x_vector),
                    restore_basis,
                },
                &working.vertices,
                &topology,
                stroke_targets,
                &mirrored_influence,
                &self.smooth_reference_areas,
            )
        } else {
            Vec::new()
        };
        let had_mirrored_branch = !mirrored_proposals.is_empty();
        let mut proposals = if stroke_x_symmetry {
            merge_symmetric_proposals(
                &working.vertices,
                std::mem::take(&mut primary_proposals),
                std::mem::take(&mut mirrored_proposals),
            )
        } else {
            primary_proposals
        };
        if is_smooth && had_mirrored_branch {
            proposals = backtrack_smooth_proposals(
                &working.vertices,
                &topology,
                proposals,
                &self.smooth_reference_areas,
            );
        }

        let rebase_triangles = if is_smooth {
            BTreeSet::new()
        } else {
            proposals
                .iter()
                .flat_map(|(vertex, _)| topology.incident_triangles[*vertex].iter().copied())
                .filter(|triangle| self.smooth_reference_areas.contains_key(triangle))
                .collect::<BTreeSet<_>>()
        };
        let working = self.working.as_mut().ok_or(SculptError::NotInitialized)?;
        let stroke = self
            .active_stroke
            .as_mut()
            .ok_or(SculptError::NoActiveStroke)?;
        let mut presentation = self.presentation_vertices.as_mut();
        for &(index, next) in &proposals {
            let previous = working.vertices[index];
            stroke.before.entry(index as u32).or_insert(previous);
            working.vertices[index] = next;
            if let Some(presentation) = presentation.as_deref_mut() {
                let delta = DVec3::from_array(next) - DVec3::from_array(previous);
                presentation[index] = (DVec3::from_array(presentation[index]) + delta).to_array();
            }
        }
        for triangle in rebase_triangles {
            let previous = self
                .smooth_reference_areas
                .get(&triangle)
                .copied()
                .expect("rebase set contains only referenced triangles");
            stroke
                .smooth_reference_before
                .entry(triangle)
                .or_insert(Some(previous));
            let area =
                triangle_double_area(topology.triangles[triangle as usize], &working.vertices);
            self.smooth_reference_areas.insert(triangle, area);
        }
        if !proposals.is_empty() {
            self.applied = false;
            self.changed = true;
        }
        Ok(proposals.len())
    }

    pub fn end_stroke(&mut self) -> Result<bool, SculptError> {
        let Some(stroke) = self.active_stroke.take() else {
            return Ok(false);
        };
        if stroke.before.is_empty() {
            return Ok(false);
        }
        self.push_history(SculptUndoStep::Stroke {
            before: stroke.before.into_iter().collect(),
            smooth_reference_before: stroke.smooth_reference_before.into_iter().collect(),
        });
        Ok(true)
    }

    pub fn undo(&mut self) -> Result<bool, SculptError> {
        if let Some(stroke) = self.active_stroke.take() {
            self.restore_stroke(
                stroke.before.into_iter().collect(),
                stroke.smooth_reference_before.into_iter().collect(),
            )?;
            return Ok(true);
        }
        let Some((_, step)) = self.history.pop_back() else {
            return Ok(false);
        };
        match step {
            SculptUndoStep::Stroke {
                before,
                smooth_reference_before,
            } => self.restore_stroke(before, smooth_reference_before)?,
            SculptUndoStep::Reset {
                working,
                smooth_reference_areas,
                last_hit_triangle,
                last_hit_view_direction,
                last_hit_anchor,
                applied,
                changed,
            } => {
                if let (Some(current), Some(presentation)) =
                    (self.working.as_ref(), self.presentation_vertices.as_mut())
                {
                    for ((presented, current), restored) in presentation
                        .iter_mut()
                        .zip(&current.vertices)
                        .zip(&working.vertices)
                    {
                        let delta = DVec3::from_array(*restored) - DVec3::from_array(*current);
                        *presented = (DVec3::from_array(*presented) + delta).to_array();
                    }
                }
                self.working = Some(working);
                self.smooth_reference_areas = smooth_reference_areas;
                self.last_hit_triangle.set(last_hit_triangle);
                self.last_hit_view_direction.set(last_hit_view_direction);
                self.last_hit_anchor.set(last_hit_anchor);
                self.applied = applied;
                self.changed = changed;
            }
        }
        Ok(true)
    }

    fn restore_stroke(
        &mut self,
        before: Vec<(u32, [f64; 3])>,
        smooth_reference_before: Vec<(u32, Option<f64>)>,
    ) -> Result<(), SculptError> {
        let working = self.working.as_mut().ok_or(SculptError::NotInitialized)?;
        let mut presentation = self.presentation_vertices.as_mut();
        for (index, point) in before {
            let index = index as usize;
            let current = working.vertices[index];
            working.vertices[index] = point;
            if let Some(presentation) = presentation.as_deref_mut() {
                let delta = DVec3::from_array(point) - DVec3::from_array(current);
                presentation[index] = (DVec3::from_array(presentation[index]) + delta).to_array();
            }
        }
        for (triangle, reference) in smooth_reference_before {
            if let Some(area) = reference {
                self.smooth_reference_areas.insert(triangle, area);
            } else {
                self.smooth_reference_areas.remove(&triangle);
            }
        }
        self.applied = false;
        self.changed = self
            .baseline
            .as_ref()
            .is_some_and(|baseline| working != baseline.as_ref());
        Ok(())
    }

    fn push_history(&mut self, step: SculptUndoStep) {
        if self.history.len() == MAX_SCULPT_HISTORY {
            self.history.pop_front();
        }
        self.history
            .push_back((crate::edit_clock::next_seq(), step));
    }

    pub fn reset(&mut self) -> Result<bool, SculptError> {
        self.end_stroke()?;
        let baseline = self.baseline.as_ref().ok_or(SculptError::NotInitialized)?;
        let working = self.working.as_ref().ok_or(SculptError::NotInitialized)?;
        let changes_reset_state = working != baseline.as_ref()
            || !self.smooth_reference_areas.is_empty()
            || self.last_hit_triangle.get().is_some()
            || self.last_hit_view_direction.get().is_some()
            || self.last_hit_anchor.get().is_some()
            || self.applied
            || self.changed;
        if !changes_reset_state {
            return Ok(false);
        }
        let undo = SculptUndoStep::Reset {
            working: working.clone(),
            smooth_reference_areas: self.smooth_reference_areas.clone(),
            last_hit_triangle: self.last_hit_triangle.get(),
            last_hit_view_direction: self.last_hit_view_direction.get(),
            last_hit_anchor: self.last_hit_anchor.get(),
            applied: self.applied,
            changed: self.changed,
        };
        if let Some(presentation) = self.presentation_vertices.as_mut() {
            for ((presented, current), reset) in presentation
                .iter_mut()
                .zip(&working.vertices)
                .zip(&baseline.vertices)
            {
                let delta = DVec3::from_array(*reset) - DVec3::from_array(*current);
                *presented = (DVec3::from_array(*presented) + delta).to_array();
            }
        }
        self.working = Some((**baseline).clone());
        self.smooth_reference_areas.clear();
        self.last_hit_triangle.set(None);
        self.last_hit_view_direction.set(None);
        self.last_hit_anchor.set(None);
        self.applied = false;
        self.changed = false;
        self.push_history(undo);
        Ok(true)
    }

    #[cfg(test)]
    pub fn prepare_apply(&self) -> Result<&OrderedObjMesh, SculptError> {
        self.working.as_ref().ok_or(SculptError::NotInitialized)
    }

    pub fn mark_applied(&mut self) {
        if self.active_stroke.is_some() {
            let _ = self.end_stroke();
        }
        self.applied = true;
    }

    #[cfg(test)]
    pub fn raycast(&self, origin_local: [f64; 3], direction_local: [f64; 3]) -> Option<SculptHit> {
        self.raycast_visible(origin_local, direction_local, SculptTargets::ALL)
    }

    pub fn raycast_visible(
        &self,
        origin_local: [f64; 3],
        direction_local: [f64; 3],
        visible_targets: SculptTargets,
    ) -> Option<SculptHit> {
        self.raycast_visible_inner(origin_local, direction_local, visible_targets, None)
    }

    pub fn raycast_visible_with_brush_radius(
        &self,
        origin_local: [f64; 3],
        direction_local: [f64; 3],
        visible_targets: SculptTargets,
        pick_radius_local: f64,
    ) -> Option<SculptHit> {
        self.raycast_visible_inner(
            origin_local,
            direction_local,
            visible_targets,
            Some(pick_radius_local),
        )
    }

    fn raycast_visible_inner(
        &self,
        origin_local: [f64; 3],
        direction_local: [f64; 3],
        visible_targets: SculptTargets,
        pick_radius_local: Option<f64>,
    ) -> Option<SculptHit> {
        self.last_hit_triangle.set(None);
        self.last_hit_view_direction.set(None);
        self.last_hit_anchor.set(None);
        let working = self.working.as_ref()?;
        let interaction_vertices = self
            .presentation_vertices
            .as_deref()
            .unwrap_or(&working.vertices);
        let topology = self.topology.as_ref()?;
        let origin = DVec3::from_array(origin_local);
        let direction = DVec3::from_array(direction_local).try_normalize()?;

        let visible_targets =
            SculptTargets::from_bits(visible_targets.bits() & self.visible_targets.bits());
        if !origin.is_finite() || !direction.is_finite() || visible_targets.is_empty() {
            return None;
        }

        let backface_masking = self
            .active_stroke
            .as_ref()
            .map(|stroke| stroke.backface_masking)
            .unwrap_or(self.backface_masking);

        let hit = if let Some(exact_hit) = nearest_visible_ray_hit(
            interaction_vertices,
            topology,
            origin,
            direction,
            visible_targets,
            backface_masking,
        ) {
            self.targets
                .intersects_bits(topology.triangle_targets[exact_hit.triangle_index as usize])
                .then_some(exact_hit)?
        } else {
            let pick_radius = pick_radius_local
                .filter(|radius| radius.is_finite() && *radius >= MIN_BRUSH_RADIUS)?;
            nearest_brush_surface_hit(
                interaction_vertices,
                topology,
                &BrushHaloQuery {
                    origin,
                    direction,
                    visible_targets,
                    editable_targets: self.targets,
                    pick_radius,
                    backface_masking,
                },
            )?
        };

        self.last_hit_triangle.set(Some(hit.triangle_index));
        self.last_hit_view_direction.set(Some(direction.to_array()));
        self.last_hit_anchor.set(Some(StrokeAnchor {
            point: DVec3::from_array(hit.point_local),
            normal: DVec3::from_array(hit.normal_local),
            seed_triangle: hit.triangle_index,
        }));
        Some(hit)
    }
}

fn build_x_mirror_vertex_map(basis: &[[f64; 3]]) -> Vec<usize> {
    const CELL: f64 = 0.01;
    const SEARCH_RADIUS: i64 = 2;
    let cell = |point: [f64; 3]| {
        (
            (point[0] / CELL).round() as i64,
            (point[1] / CELL).round() as i64,
            (point[2] / CELL).round() as i64,
        )
    };
    let mut buckets = BTreeMap::<(i64, i64, i64), Vec<usize>>::new();
    for (index, &point) in basis.iter().enumerate() {
        buckets.entry(cell(point)).or_default().push(index);
    }
    basis
        .iter()
        .enumerate()
        .map(|(source, point)| {
            let target = [-point[0], point[1], point[2]];
            let (cx, cy, cz) = cell(target);
            let mut best = None::<(f64, usize)>;
            for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
                for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
                    for dz in -SEARCH_RADIUS..=SEARCH_RADIUS {
                        let Some(candidates) = buckets.get(&(cx + dx, cy + dy, cz + dz)) else {
                            continue;
                        };
                        for &candidate in candidates {
                            let point = basis[candidate];
                            let distance = (point[0] - target[0]).powi(2)
                                + (point[1] - target[1]).powi(2)
                                + (point[2] - target[2]).powi(2);
                            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                                best = Some((distance, candidate));
                            }
                        }
                    }
                }
            }
            best.map_or(source, |(_, candidate)| candidate)
        })
        .collect()
}

fn mirror_influence_by_topology(
    influence: &[BrushInfluence],
    mirror_vertices: &[usize],
) -> Vec<BrushInfluence> {
    let mut mirrored = BTreeMap::<usize, BrushInfluence>::new();
    for sample in influence {
        let Some(&vertex) = mirror_vertices.get(sample.vertex) else {
            continue;
        };
        let mut sample = *sample;
        sample.vertex = vertex;
        sample.sheet_normal = sample.sheet_normal.map(mirror_x_vector);
        sample.incoming_direction = sample.incoming_direction.map(mirror_x_vector);
        mirrored
            .entry(vertex)
            .and_modify(|current| {
                if sample.falloff > current.falloff {
                    *current = sample;
                }
            })
            .or_insert(sample);
    }
    mirrored.into_values().collect()
}

fn validate_dab(dab: SculptDab) -> Result<(), SculptError> {
    let operation_is_finite = match dab.operation {
        SculptOperation::Grab { translation_local } => {
            translation_local.into_iter().all(f64::is_finite)
        }
        SculptOperation::Smooth | SculptOperation::Restore | SculptOperation::RestoreFit => true,
        SculptOperation::Inflate { distance } => distance.is_finite(),
    };
    if !dab.center_local.into_iter().all(f64::is_finite)
        || !dab.radius_local.is_finite()
        || dab.radius_local < MIN_BRUSH_RADIUS
        || !dab.strength.is_finite()
        || !(0.0..=1.0).contains(&dab.strength)
        || !operation_is_finite
    {
        return Err(SculptError::InvalidDab);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
