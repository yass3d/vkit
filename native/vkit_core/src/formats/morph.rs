use std::{collections::BTreeMap, sync::Arc};

use super::{DazGeometry, FormatError, OrderedObjMesh, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct MorphCompatibility {
    pub vertex_count: usize,
    pub face_count: usize,
    pub topology_validated: bool,
    pub face_order_matches: bool,
    pub source_unit_scale: f64,
    pub zero_tolerance: f64,
    pub active_vertex_count: usize,
    pub maximum_delta: f64,
    pub rms_delta: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MorphTarget {
    pub key: String,
    pub deltas: Vec<[f64; 3]>,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
    pub compatibility: MorphCompatibility,

    reference_faces: Arc<Vec<Vec<u32>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphAuthoring {
    pub source_unit_scale: f64,

    pub zero_tolerance: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
}

impl MorphAuthoring {
    pub const UNIT: Self = Self {
        source_unit_scale: 1.0,
        zero_tolerance: 0.0,
        minimum: 0.0,
        maximum: 1.0,
        default: 0.0,
    };

    pub const fn with_range(minimum: f64, maximum: f64) -> Self {
        Self {
            minimum,
            maximum,
            ..Self::UNIT
        }
    }

    pub const fn vam(minimum: f64, maximum: f64, default: f64) -> Self {
        Self {
            source_unit_scale: 100.0,
            zero_tolerance: 1.0e-12,
            minimum,
            maximum,
            default,
        }
    }
}

impl MorphTarget {
    pub const MINIMUM_EXTRAPOLATION_MAGNITUDE: f64 = 2.0;

    pub fn from_sparse_deltas(
        key: impl Into<String>,
        vertex_count: usize,
        sparse_deltas: &[(u32, [f64; 3])],
        reference_faces: Vec<Vec<u32>>,
        authoring: MorphAuthoring,
    ) -> Result<Self> {
        Self::from_sparse_deltas_shared(
            key,
            vertex_count,
            sparse_deltas,
            Arc::new(reference_faces),
            authoring,
        )
    }

    pub fn from_sparse_deltas_shared(
        key: impl Into<String>,
        vertex_count: usize,
        sparse_deltas: &[(u32, [f64; 3])],
        reference_faces: Arc<Vec<Vec<u32>>>,
        authoring: MorphAuthoring,
    ) -> Result<Self> {
        let zero_tolerance = authoring.zero_tolerance;
        if vertex_count == 0 {
            return Err(morph_error("cached morph vertex count must be positive"));
        }
        let mut dense = vec![[0.0; 3]; vertex_count];
        let mut previous = None;
        for (sparse_index, &(vertex_id, delta)) in sparse_deltas.iter().enumerate() {
            let vertex_id = usize::try_from(vertex_id)
                .map_err(|_| morph_error("cached morph vertex ID does not fit usize"))?;
            if vertex_id >= vertex_count {
                return Err(morph_error(format!(
                    "cached morph delta {sparse_index} references vertex {vertex_id}, but vertex count is {vertex_count}"
                )));
            }
            if previous.is_some_and(|prior| prior >= vertex_id) {
                return Err(morph_error(
                    "cached morph vertex IDs must be strictly increasing",
                ));
            }
            if !delta.iter().all(|value| value.is_finite()) {
                return Err(morph_error(format!(
                    "cached morph delta {sparse_index} is non-finite"
                )));
            }
            let norm = delta.iter().map(|value| value * value).sum::<f64>().sqrt();
            if norm <= zero_tolerance {
                return Err(morph_error(format!(
                    "cached morph delta {sparse_index} does not exceed zero tolerance"
                )));
            }
            dense[vertex_id] = delta;
            previous = Some(vertex_id);
        }
        if sparse_deltas.is_empty() {
            return Err(morph_error("cached morph contains no active deltas"));
        }
        Self::from_dense_deltas_shared(key, dense, reference_faces, authoring)
    }

    pub(crate) fn from_dense_deltas(
        key: impl Into<String>,
        deltas: Vec<[f64; 3]>,
        reference_faces: Vec<Vec<u32>>,
        authoring: MorphAuthoring,
    ) -> Result<Self> {
        Self::from_dense_deltas_shared(key, deltas, Arc::new(reference_faces), authoring)
    }

    pub(crate) fn from_dense_deltas_shared(
        key: impl Into<String>,
        deltas: Vec<[f64; 3]>,
        reference_faces: Arc<Vec<Vec<u32>>>,
        authoring: MorphAuthoring,
    ) -> Result<Self> {
        let MorphAuthoring {
            source_unit_scale,
            zero_tolerance,
            minimum,
            maximum,
            default,
        } = authoring;
        let key = key.into();
        if key.trim().is_empty() {
            return Err(morph_error("morph key must not be empty"));
        }
        validate_slider_range(minimum, maximum, default)?;
        if !source_unit_scale.is_finite() || source_unit_scale <= 0.0 {
            return Err(morph_error(
                "morph source unit scale must be positive and finite",
            ));
        }
        if !zero_tolerance.is_finite() || zero_tolerance < 0.0 {
            return Err(morph_error(
                "morph zero tolerance must be finite and non-negative",
            ));
        }
        validate_base_faces(deltas.len(), reference_faces.as_slice())?;
        let statistics = delta_statistics(&deltas)?;
        let result = Self {
            key,
            compatibility: MorphCompatibility {
                vertex_count: deltas.len(),
                face_count: reference_faces.len(),
                topology_validated: true,
                face_order_matches: true,
                source_unit_scale,
                zero_tolerance,
                active_vertex_count: statistics.active_vertex_count,
                maximum_delta: statistics.maximum_delta,
                rms_delta: statistics.rms_delta,
            },
            deltas,
            minimum,
            maximum,
            default,
            reference_faces,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<()> {
        if self.key.trim().is_empty() {
            return Err(morph_error("morph key must not be empty"));
        }
        validate_slider_range(self.minimum, self.maximum, self.default)?;
        if self.deltas.is_empty() {
            return Err(morph_error("morph target has no vertex deltas"));
        }
        if self.compatibility.vertex_count != self.deltas.len() {
            return Err(morph_error(format!(
                "morph {:?} compatibility records {} vertices, but it contains {} deltas",
                self.key,
                self.compatibility.vertex_count,
                self.deltas.len()
            )));
        }
        if !self.compatibility.topology_validated {
            return Err(morph_error(format!(
                "morph {:?} does not have validated topology",
                self.key
            )));
        }
        if self.compatibility.face_count != self.reference_faces.len() {
            return Err(morph_error(format!(
                "morph {:?} compatibility records {} faces, but its topology receipt contains {}",
                self.key,
                self.compatibility.face_count,
                self.reference_faces.len()
            )));
        }
        validate_base_faces(self.deltas.len(), &self.reference_faces)?;
        if !self.compatibility.source_unit_scale.is_finite()
            || self.compatibility.source_unit_scale <= 0.0
        {
            return Err(morph_error(
                "morph source unit scale must be positive and finite",
            ));
        }
        if !self.compatibility.zero_tolerance.is_finite() || self.compatibility.zero_tolerance < 0.0
        {
            return Err(morph_error(
                "morph zero tolerance must be finite and non-negative",
            ));
        }

        let statistics = delta_statistics(&self.deltas)?;
        if statistics.active_vertex_count != self.compatibility.active_vertex_count
            || statistics.maximum_delta != self.compatibility.maximum_delta
            || statistics.rms_delta != self.compatibility.rms_delta
        {
            return Err(morph_error(format!(
                "morph {:?} delta statistics do not match its compatibility receipt",
                self.key
            )));
        }
        Ok(())
    }

    pub fn reference_faces(&self) -> &[Vec<u32>] {
        self.reference_faces.as_slice()
    }

    pub fn checked_value(&self, value: f64) -> Result<f64> {
        validate_slider_range(self.minimum, self.maximum, self.default)?;
        if !value.is_finite() {
            return Err(morph_error(format!(
                "morph {:?} slider value must be finite",
                self.key
            )));
        }
        if !(self.minimum..=self.maximum).contains(&value) {
            return Err(morph_error(format!(
                "morph {:?} slider value {value} is outside [{}, {}]",
                self.key, self.minimum, self.maximum
            )));
        }
        Ok(value)
    }

    pub fn checked_extrapolated_value(&self, value: f64) -> Result<f64> {
        validate_slider_range(self.minimum, self.maximum, self.default)?;
        if !value.is_finite() {
            return Err(morph_error(format!(
                "morph {:?} extrapolated value must be finite",
                self.key
            )));
        }
        let (minimum, maximum) = self.extrapolated_bounds();
        if !(minimum..=maximum).contains(&value) {
            return Err(morph_error(format!(
                "morph {:?} extrapolated value {value} is outside [{minimum}, {maximum}]",
                self.key
            )));
        }
        Ok(value)
    }

    pub fn extrapolated_bounds(&self) -> (f64, f64) {
        (
            self.minimum.min(-Self::MINIMUM_EXTRAPOLATION_MAGNITUDE),
            self.maximum.max(Self::MINIMUM_EXTRAPOLATION_MAGNITUDE),
        )
    }

    pub fn compose(
        &self,
        result_vertices: &[[f64; 3]],
        value: f64,
        reference_value: f64,
    ) -> Result<Vec<[f64; 3]>> {
        self.validate()?;
        self.compose_validated(result_vertices, value, reference_value)
    }

    pub fn apply_to_daz_geometry(&self, geometry: &DazGeometry, value: f64) -> Result<DazGeometry> {
        geometry.validate()?;
        self.validate_application_topology(
            geometry.vertices.len(),
            geometry.faces.iter().map(Vec::as_slice),
        )?;

        let mut result = geometry.clone();
        result.vertices = self.compose_validated(&geometry.vertices, value, self.default)?;
        result.validate()?;
        Ok(result)
    }

    pub fn bake_onto_ordered_obj(
        &self,
        result_mesh: &OrderedObjMesh,
        value: f64,
        reference_value: f64,
    ) -> Result<OrderedObjMesh> {
        result_mesh.validate()?;
        self.validate_application_topology(
            result_mesh.vertices.len(),
            result_mesh
                .faces
                .iter()
                .map(|face| face.vertex_indices.as_slice()),
        )?;

        let mut result = result_mesh.clone();
        result.vertices = self.compose_validated(&result_mesh.vertices, value, reference_value)?;
        result.validate()?;
        Ok(result)
    }

    pub fn apply_to_bound_vertices(
        &self,
        vertices: &mut [[f64; 3]],
        value: f64,
        reference_value: f64,
    ) -> Result<()> {
        if !self.compatibility.topology_validated
            || !self.compatibility.face_order_matches
            || self.compatibility.vertex_count != vertices.len()
            || self.deltas.len() != vertices.len()
        {
            return Err(morph_error(format!(
                "morph {:?} is not bound to the supplied G2 vertices",
                self.key
            )));
        }
        let amount = self.checked_value(value)? - self.checked_value(reference_value)?;
        for (position, delta) in vertices.iter_mut().zip(&self.deltas) {
            for axis in 0..3 {
                position[axis] += amount * delta[axis];
            }
        }
        Ok(())
    }

    fn compose_validated(
        &self,
        result_vertices: &[[f64; 3]],
        value: f64,
        reference_value: f64,
    ) -> Result<Vec<[f64; 3]>> {
        if result_vertices.len() != self.deltas.len() {
            return Err(morph_error(format!(
                "morph {:?} has {} vertices; result has {}",
                self.key,
                self.deltas.len(),
                result_vertices.len()
            )));
        }
        let amount = self.checked_value(value)? - self.checked_value(reference_value)?;
        if !amount.is_finite() {
            return Err(morph_error(format!(
                "morph {:?} slider difference is not finite",
                self.key
            )));
        }
        let mut result = Vec::with_capacity(result_vertices.len());
        for (base, delta) in result_vertices.iter().zip(&self.deltas) {
            if !base.iter().all(|coordinate| coordinate.is_finite()) {
                return Err(morph_error(
                    "morph base vertices contain a non-finite coordinate",
                ));
            }
            let value = [
                base[0] + amount * delta[0],
                base[1] + amount * delta[1],
                base[2] + amount * delta[2],
            ];
            if !value.iter().all(|coordinate| coordinate.is_finite()) {
                return Err(morph_error(
                    "composed morph contains non-finite coordinates",
                ));
            }
            result.push(value);
        }
        Ok(result)
    }

    fn validate_application_topology<'a>(
        &self,
        vertex_count: usize,
        faces: impl ExactSizeIterator<Item = &'a [u32]>,
    ) -> Result<()> {
        self.validate()?;
        if vertex_count != self.deltas.len() {
            return Err(morph_error(format!(
                "morph {:?} has {} vertices; geometry has {vertex_count}",
                self.key,
                self.deltas.len()
            )));
        }
        validate_exact_application_topology(vertex_count, faces, &self.reference_faces)?;
        Ok(())
    }
}

pub fn validate_morph_topology(
    target: &OrderedObjMesh,
    base_vertex_count: usize,
    base_faces: &[Vec<u32>],
    require_faces: bool,
) -> Result<bool> {
    target.validate()?;
    if target.vertices.len() != base_vertex_count {
        return Err(morph_error(format!(
            "morph OBJ has {} vertices; base has {base_vertex_count}",
            target.vertices.len()
        )));
    }
    validate_base_faces(base_vertex_count, base_faces)?;
    if target.faces.is_empty() {
        return if require_faces {
            Err(morph_error(
                "morph OBJ has no faces, so topology cannot be verified",
            ))
        } else {
            Ok(false)
        };
    }
    let target_faces: Vec<&[u32]> = target
        .faces
        .iter()
        .map(|face| face.vertex_indices.as_slice())
        .collect();
    validate_face_topology(base_vertex_count, &target_faces, base_faces)?;
    Ok(target_faces
        .iter()
        .zip(base_faces)
        .all(|(target, base)| *target == base.as_slice()))
}

pub fn infer_target_unit_scale(
    base_vertices: &[[f64; 3]],
    target_vertices: &[[f64; 3]],
) -> Result<f64> {
    if base_vertices.len() != target_vertices.len() {
        return Err(morph_error(format!(
            "cannot infer scale for {} target and {} base vertices",
            target_vertices.len(),
            base_vertices.len()
        )));
    }
    if base_vertices.is_empty() {
        return Err(morph_error("cannot infer scale from empty vertices"));
    }
    let base_extent = extents(base_vertices)?;
    let target_extent = extents(target_vertices)?;
    let mut ratios = Vec::with_capacity(3);
    for axis in 0..3 {
        if base_extent[axis] > 1.0e-12 && target_extent[axis] > 1.0e-12 {
            ratios.push(base_extent[axis] / target_extent[axis]);
        }
    }
    if ratios.is_empty() {
        return Err(morph_error("cannot infer scale from zero-size bounds"));
    }
    ratios.sort_by(f64::total_cmp);
    let raw = if ratios.len() % 2 == 1 {
        ratios[ratios.len() / 2]
    } else {
        (ratios[ratios.len() / 2 - 1] + ratios[ratios.len() / 2]) * 0.5
    };
    if !raw.is_finite() || raw <= 0.0 {
        return Err(morph_error("could not infer a positive unit scale"));
    }
    let decimal_scale = 10.0_f64.powf(raw.log10().round());
    let evidence = raw / decimal_scale;
    if !(0.8..=1.25).contains(&evidence) {
        return Err(morph_error(format!(
            "ambiguous morph OBJ unit ratio {raw}; provide unit scale explicitly"
        )));
    }
    Ok(decimal_scale)
}

pub struct ObjMorphSource<'a> {
    pub target: &'a OrderedObjMesh,
    pub base_vertices: &'a [[f64; 3]],
    pub base_faces: &'a [Vec<u32>],
    pub unit_scale: Option<f64>,
}

pub fn load_obj_morph_target(
    key: impl Into<String>,
    source: ObjMorphSource<'_>,
    zero_tolerance: f64,
    minimum: f64,
    maximum: f64,
    default: f64,
) -> Result<MorphTarget> {
    let ObjMorphSource {
        target,
        base_vertices,
        base_faces,
        unit_scale,
    } = source;
    let key = key.into();
    if key.trim().is_empty() {
        return Err(morph_error("morph key must not be empty"));
    }
    if !zero_tolerance.is_finite() || zero_tolerance < 0.0 {
        return Err(morph_error(
            "morph zero tolerance must be finite and non-negative",
        ));
    }
    validate_slider_range(minimum, maximum, default)?;
    let face_order_matches =
        validate_morph_topology(target, base_vertices.len(), base_faces, true)?;
    let auto_scale = unit_scale.is_none();
    let scale = match unit_scale {
        Some(value) if value.is_finite() && value > 0.0 => value,
        Some(_) => return Err(morph_error("unit scale must be positive and finite")),
        None => infer_target_unit_scale(base_vertices, &target.vertices)?,
    };

    let mut deltas = Vec::with_capacity(base_vertices.len());
    for (base, source) in base_vertices.iter().zip(&target.vertices) {
        if !base.iter().all(|value| value.is_finite()) {
            return Err(morph_error("base vertices contain a non-finite coordinate"));
        }
        let delta = [
            source[0] * scale - base[0],
            source[1] * scale - base[1],
            source[2] * scale - base[2],
        ];
        if !delta.iter().all(|coordinate| coordinate.is_finite()) {
            return Err(morph_error(
                "morph deltas contain a non-finite coordinate after unit conversion",
            ));
        }
        deltas.push(delta);
    }
    if auto_scale {
        let extent = extents(base_vertices)?;
        let diagonal = extent.iter().map(|value| value * value).sum::<f64>().sqrt();
        let raw_rms = delta_statistics(&deltas)?.rms_delta;
        if diagonal <= 1.0e-12 || raw_rms > diagonal * 0.15 {
            return Err(morph_error(format!(
                "morph has matching topology but incompatible axes/origin; RMS {raw_rms} for diagonal {diagonal}"
            )));
        }
    }
    for delta in &mut deltas {
        if delta_norm(*delta) <= zero_tolerance {
            *delta = [0.0; 3];
        }
    }
    let statistics = delta_statistics(&deltas)?;
    let compatibility = MorphCompatibility {
        vertex_count: base_vertices.len(),
        face_count: base_faces.len(),
        topology_validated: true,
        face_order_matches,
        source_unit_scale: scale,
        zero_tolerance,
        active_vertex_count: statistics.active_vertex_count,
        maximum_delta: statistics.maximum_delta,
        rms_delta: statistics.rms_delta,
    };
    let result = MorphTarget {
        key,
        deltas,
        minimum,
        maximum,
        default,
        compatibility,
        reference_faces: Arc::new(base_faces.to_vec()),
    };
    result.validate()?;
    Ok(result)
}

fn validate_slider_range(minimum: f64, maximum: f64, default: f64) -> Result<()> {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || !default.is_finite()
        || minimum >= maximum
        || !(minimum..=maximum).contains(&default)
    {
        return Err(morph_error("invalid morph slider range"));
    }
    Ok(())
}

fn validate_face_topology<T: AsRef<[u32]>>(
    vertex_count: usize,
    candidate_faces: &[T],
    reference_faces: &[Vec<u32>],
) -> Result<()> {
    validate_base_faces(vertex_count, reference_faces)?;
    for (face_id, face) in candidate_faces.iter().enumerate() {
        let face = face.as_ref();
        if face.len() < 3 {
            return Err(morph_error(format!(
                "geometry face {face_id} contains fewer than three corners"
            )));
        }
        if let Some(&bad) = face.iter().find(|&&index| index as usize >= vertex_count) {
            return Err(morph_error(format!(
                "geometry face {face_id} references vertex {bad}, but the geometry has {vertex_count} vertices"
            )));
        }
    }
    if candidate_faces.len() != reference_faces.len() {
        return Err(morph_error(format!(
            "geometry has {} faces; morph reference has {}",
            candidate_faces.len(),
            reference_faces.len()
        )));
    }

    let candidate_undirected = face_counter(candidate_faces.iter().map(|face| {
        let mut sorted = face.as_ref().to_vec();
        sorted.sort_unstable();
        sorted
    }));
    let reference_undirected = face_counter(reference_faces.iter().map(|face| {
        let mut sorted = face.clone();
        sorted.sort_unstable();
        sorted
    }));
    if candidate_undirected != reference_undirected {
        return Err(morph_error(
            "geometry polygon membership or duplicate counts differ from morph reference",
        ));
    }

    let candidate_oriented = face_counter(
        candidate_faces
            .iter()
            .map(|face| oriented_cycle(face.as_ref())),
    );
    let reference_oriented = face_counter(
        reference_faces
            .iter()
            .map(|face| oriented_cycle(face.as_slice())),
    );
    if candidate_oriented != reference_oriented {
        return Err(morph_error(
            "geometry contains reversed polygon winding relative to morph reference",
        ));
    }
    Ok(())
}

fn validate_exact_application_topology<'a>(
    vertex_count: usize,
    candidate_faces: impl ExactSizeIterator<Item = &'a [u32]>,
    reference_faces: &[Vec<u32>],
) -> Result<()> {
    if candidate_faces.len() != reference_faces.len() {
        return Err(morph_error(format!(
            "geometry has {} faces; morph reference has {}",
            candidate_faces.len(),
            reference_faces.len()
        )));
    }
    for (face_id, (candidate, reference)) in candidate_faces.zip(reference_faces).enumerate() {
        if candidate.len() < 3 {
            return Err(morph_error(format!(
                "geometry face {face_id} contains fewer than three corners"
            )));
        }
        if let Some(&bad) = candidate
            .iter()
            .find(|&&index| index as usize >= vertex_count)
        {
            return Err(morph_error(format!(
                "geometry face {face_id} references vertex {bad}, but the geometry has {vertex_count} vertices"
            )));
        }
        if candidate != reference.as_slice() {
            return Err(morph_error(format!(
                "geometry face {face_id} differs from the ordered morph reference topology"
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DeltaStatistics {
    active_vertex_count: usize,
    maximum_delta: f64,
    rms_delta: f64,
}

fn delta_statistics(deltas: &[[f64; 3]]) -> Result<DeltaStatistics> {
    if deltas.is_empty() {
        return Err(morph_error("morph target has no vertex deltas"));
    }
    let mut active_vertex_count = 0;
    let mut maximum_delta = 0.0_f64;
    let mut scale = 0.0_f64;
    let mut scaled_sum_squares = 1.0_f64;
    for delta in deltas {
        if !delta.iter().all(|coordinate| coordinate.is_finite()) {
            return Err(morph_error("morph deltas contain a non-finite coordinate"));
        }
        let norm = delta_norm(*delta);
        if norm > 0.0 {
            active_vertex_count += 1;
        }
        maximum_delta = maximum_delta.max(norm);
        for coordinate in delta {
            let absolute = coordinate.abs();
            if absolute == 0.0 {
                continue;
            }
            if scale < absolute {
                scaled_sum_squares = 1.0 + scaled_sum_squares * (scale / absolute).powi(2);
                scale = absolute;
            } else {
                scaled_sum_squares += (absolute / scale).powi(2);
            }
        }
    }
    let rms_delta = if scale == 0.0 {
        0.0
    } else {
        scale * (scaled_sum_squares / deltas.len() as f64).sqrt()
    };
    if !maximum_delta.is_finite() || !rms_delta.is_finite() {
        return Err(morph_error(
            "morph delta magnitude exceeds the finite range",
        ));
    }
    Ok(DeltaStatistics {
        active_vertex_count,
        maximum_delta,
        rms_delta,
    })
}

fn delta_norm(delta: [f64; 3]) -> f64 {
    delta[0].hypot(delta[1]).hypot(delta[2])
}

fn validate_base_faces(vertex_count: usize, faces: &[Vec<u32>]) -> Result<()> {
    for (face_id, face) in faces.iter().enumerate() {
        if face.len() < 3 {
            return Err(morph_error(format!(
                "base face {face_id} contains fewer than three corners"
            )));
        }
        if let Some(&bad) = face.iter().find(|&&index| index as usize >= vertex_count) {
            return Err(morph_error(format!(
                "base face {face_id} references vertex {bad}, but the base has {vertex_count} vertices"
            )));
        }
    }
    Ok(())
}

fn face_counter(faces: impl IntoIterator<Item = Vec<u32>>) -> BTreeMap<Vec<u32>, usize> {
    let mut counts = BTreeMap::new();
    for face in faces {
        *counts.entry(face).or_insert(0) += 1;
    }
    counts
}

fn oriented_cycle(face: &[u32]) -> Vec<u32> {
    let mut best = face.to_vec();
    for offset in 1..face.len() {
        let candidate: Vec<u32> = face[offset..]
            .iter()
            .chain(&face[..offset])
            .copied()
            .collect();
        if candidate < best {
            best = candidate;
        }
    }
    best
}

fn extents(vertices: &[[f64; 3]]) -> Result<[f64; 3]> {
    let Some(first) = vertices.first().copied() else {
        return Err(morph_error("vertex array is empty"));
    };
    if !first.iter().all(|value| value.is_finite()) {
        return Err(morph_error("vertices contain a non-finite coordinate"));
    }
    let mut minimum = first;
    let mut maximum = first;
    for vertex in &vertices[1..] {
        if !vertex.iter().all(|value| value.is_finite()) {
            return Err(morph_error("vertices contain a non-finite coordinate"));
        }
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex[axis]);
            maximum[axis] = maximum[axis].max(vertex[axis]);
        }
    }
    Ok([
        maximum[0] - minimum[0],
        maximum[1] - minimum[1],
        maximum[2] - minimum[2],
    ])
}

fn morph_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidMorph(message.into())
}
