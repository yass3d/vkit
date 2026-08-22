use std::collections::BTreeMap;
use std::sync::Arc;

use vkit_core::formats::Mesh;
use vkit_core::vam::BuiltinHairScalp;

use crate::hair_settings::HairSettings;
use crate::scene::SurfaceMesh;

pub const HAIR_SCALP_PROVIDERS: [&str; 3] = ["UdaneScalp", "LeytonScalp", "SoleilScalp"];

pub const DEFAULT_HAIR_SEGMENTS: usize = 16;
pub const MAX_HAIR_SEGMENTS: usize = 50;
pub const DEFAULT_SEGMENT_LENGTH_CM: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HairExportSexes {
    Female,
    Male,
    #[default]
    Both,
}

impl HairExportSexes {
    pub fn folders(self) -> &'static [&'static str] {
        match self {
            Self::Female => &["Female"],
            Self::Male => &["Male"],
            Self::Both => &["Female", "Male"],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HairTool {
    #[default]
    Plant,
    Pick,
    Grow,
    Erase,
    Comb,

    Pinch,

    Cut,

    Puff,

    /// Take hold of one point joint and move it.
    Vertex,
}

#[derive(Clone, Debug)]
pub struct HairStrand {
    pub points_cm: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HairScalpTexture {
    pub diffuse: Option<std::path::PathBuf>,
    pub alpha: Option<std::path::PathBuf>,
}

impl HairScalpTexture {
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        self.diffuse.is_none() && self.alpha.is_none()
    }

    pub fn sheets(&self) -> impl Iterator<Item = (ScalpSlot, &std::path::PathBuf)> {
        [
            (ScalpSlot::Diffuse, self.diffuse.as_ref()),
            (ScalpSlot::Alpha, self.alpha.as_ref()),
        ]
        .into_iter()
        .filter_map(|(slot, path)| path.map(|path| (slot, path)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalpSlot {
    Diffuse,
    Alpha,
}

impl ScalpSlot {
    #[must_use]
    pub fn vam_key(self) -> &'static str {
        match self {
            Self::Diffuse => "customTexture_MainTex",
            Self::Alpha => "customTexture_AlphaTex",
        }
    }

    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Diffuse => "scalp",
            Self::Alpha => "scalp_alpha",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HairPartKind {
    #[default]
    Hair,
    Scalp,
}

impl HairPartKind {
    #[must_use]
    pub fn is_scalp(self) -> bool {
        matches!(self, Self::Scalp)
    }
}

pub const SCALP_PART_NAME: &str = "scalp";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HairPanelPage {
    #[default]
    Parts,
    Settings,
    Scalp,
}

#[derive(Clone, Debug)]
pub struct HairPart {
    pub kind: HairPartKind,
    pub scalp_texture: HairScalpTexture,
    pub revision: u64,
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub provider_name: String,
    pub segments: usize,
    pub segment_length_cm: f32,
    pub settings: HairSettings,
    pub strands: BTreeMap<u32, HairStrand>,
    pub style_joints: bool,
}

#[derive(Debug)]
pub struct ScalpAuthoring {
    pub vertices_cm: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub surface: Arc<SurfaceMesh>,
    pub normals: Vec<[f32; 3]>,
    pub export_negate_x: bool,
    pub mirror_pair: Vec<u32>,
}

impl ScalpAuthoring {
    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }

    /// The same cap, standing on the head it is wrapped to.
    ///
    /// The stock cap is the shape the provider ships. What the person sees is
    /// that shape pulled onto the head, so what they click has to be too.
    #[must_use]
    pub fn posed(&self, vertices_cm: Vec<[f32; 3]>) -> Option<Self> {
        if vertices_cm.len() != self.vertices_cm.len() {
            return None;
        }
        let mut normals = vec![[0.0_f32; 3]; vertices_cm.len()];
        for triangle in &self.triangles {
            let corner = |index: u32| vertices_cm.get(index as usize).copied();
            let (Some(a), Some(b), Some(c)) = (
                corner(triangle[0]),
                corner(triangle[1]),
                corner(triangle[2]),
            ) else {
                continue;
            };
            let (a, b, c) = (
                glam::Vec3::from_array(a),
                glam::Vec3::from_array(b),
                glam::Vec3::from_array(c),
            );
            let face = (b - a).cross(c - a);
            for index in triangle {
                if let Some(slot) = normals.get_mut(*index as usize) {
                    slot[0] += face.x;
                    slot[1] += face.y;
                    slot[2] += face.z;
                }
            }
        }
        for normal in &mut normals {
            let vector = glam::Vec3::from_array(*normal);
            *normal = vector.try_normalize().unwrap_or(glam::Vec3::Z).to_array();
        }
        let vertices_f64: Vec<[f64; 3]> = vertices_cm
            .iter()
            .map(|point| {
                [
                    f64::from(point[0]),
                    f64::from(point[1]),
                    f64::from(point[2]),
                ]
            })
            .collect();
        let picking_triangles: Vec<[u32; 3]> = if self.export_negate_x {
            self.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect()
        } else {
            self.triangles.clone()
        };
        let mesh = Mesh::new(vertices_f64, picking_triangles).ok()?;
        let surface = Arc::new(SurfaceMesh::new(mesh).ok()?);
        Some(Self {
            vertices_cm,
            triangles: self.triangles.clone(),
            uvs: self.uvs.clone(),
            surface,
            normals,
            export_negate_x: self.export_negate_x,
            mirror_pair: self.mirror_pair.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct HairCheckpoint {
    parts: Vec<HairPart>,
    selected_part_id: Option<u64>,
    active_part_ids: std::collections::BTreeSet<u64>,
}

pub const HAIR_UNDO_DEPTH: usize = 64;

type HairControl = (&'static str, u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HairEdit {
    Stroke,
    PresetLoaded,
    PartAdded,
    PartRemoved,
    PartDuplicated,
    PartMirrored,
    PartRenamed,
    ScalpAdded,
    ScalpMesh,
    ScalpTexture,
    StyleJoints,
    ParamsReset,
    SettingsPasted,
    Segments,
    Param(&'static str),
    ColorChannel(&'static str, u8),
}

impl HairEdit {
    const fn control(self) -> Option<HairControl> {
        match self {
            Self::Segments => Some(("vkit.hair.segments", 0)),
            Self::Param(key) => Some((key, 0)),
            Self::ColorChannel(key, channel) => Some((key, channel + 1)),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct HairProject {
    pub parts: Vec<HairPart>,
    pub selected_part_id: Option<u64>,
    pub active_part_ids: std::collections::BTreeSet<u64>,
    next_part_id: u64,
    pub edit_revision: u64,
    pub active_tool: HairTool,
    pub export_name: String,
    pub export_creator: String,
    pub export_sexes: HairExportSexes,
    pub active_provider: String,
    history: crate::history::History<HairCheckpoint>,
    stroke_open: bool,
    open_control: Option<HairControl>,
}

impl HairProject {
    fn snapshot(&self) -> HairCheckpoint {
        HairCheckpoint {
            parts: self.parts.clone(),
            selected_part_id: self.selected_part_id,
            active_part_ids: self.active_part_ids.clone(),
        }
    }

    fn restore(&mut self, checkpoint: HairCheckpoint) {
        self.open_control = None;
        let visibility: std::collections::BTreeMap<u64, bool> = self
            .parts
            .iter()
            .map(|part| (part.id, part.visible))
            .collect();
        self.parts = checkpoint.parts;
        for part in &mut self.parts {
            if let Some(visible) = visibility.get(&part.id) {
                part.visible = *visible;
            }
        }
        self.selected_part_id = checkpoint.selected_part_id;
        self.active_part_ids = checkpoint.active_part_ids;
        self.touch_every_part();
    }

    pub fn activate_part(&mut self, id: u64, additive: bool) {
        if !self.parts.iter().any(|part| part.id == id) {
            return;
        }
        if !additive {
            let alone = self.active_part_ids.len() == 1 && self.active_part_ids.contains(&id);
            self.active_part_ids.clear();
            if alone {
                self.selected_part_id = None;
                return;
            }
            self.active_part_ids.insert(id);
            self.selected_part_id = Some(id);
            return;
        }
        if self.active_part_ids.contains(&id) {
            if self.active_part_ids.len() <= 1 {
                return;
            }
            self.active_part_ids.remove(&id);
            if self.selected_part_id == Some(id) {
                self.selected_part_id = self.active_part_ids.iter().next().copied();
            }
        } else {
            self.active_part_ids.insert(id);
            self.selected_part_id = Some(id);
        }
    }

    pub fn editable_parts(&self) -> Vec<u64> {
        self.parts
            .iter()
            .filter(|part| part.visible && self.active_part_ids.contains(&part.id))
            .map(|part| part.id)
            .collect()
    }

    pub fn is_part_editable(&self, id: u64) -> bool {
        self.part(id)
            .is_some_and(|part| part.visible && self.active_part_ids.contains(&id))
    }

    pub fn is_part_active(&self, id: u64) -> bool {
        self.active_part_ids.contains(&id)
    }

    pub fn clear_forward_history(&mut self) {
        self.history.clear_forward();
    }

    pub fn record(&mut self, edit: HairEdit) {
        if self.stroke_open {
            return;
        }
        if let Some(control) = edit.control() {
            if self.open_control == Some(control) {
                return;
            }
            self.open_control = Some(control);
        } else {
            self.open_control = None;
        }
        self.history.record(self.snapshot());
        self.history.trim(HAIR_UNDO_DEPTH, usize::MAX, |_| 0);
    }

    pub fn end_control(&mut self) {
        self.open_control = None;
    }

    pub fn begin_stroke(&mut self) {
        if self.stroke_open {
            return;
        }
        self.record(HairEdit::Stroke);
        self.stroke_open = true;
    }

    pub fn end_stroke(&mut self) {
        self.stroke_open = false;
    }

    pub const fn stroke_open(&self) -> bool {
        self.stroke_open
    }

    pub fn history_position(&self) -> (usize, usize) {
        self.history.position()
    }

    pub fn undo(&mut self) -> bool {
        self.stroke_open = false;
        if !self.history.can_undo() {
            return false;
        }
        let here = self.snapshot();
        let Some(previous) = self.history.undo(here) else {
            return false;
        };
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.stroke_open = false;
        if !self.history.can_redo() {
            return false;
        }
        let here = self.snapshot();
        let Some(next) = self.history.redo(here) else {
            return false;
        };
        self.restore(next);
        true
    }

    pub fn effective_provider(&self) -> &str {
        if self.active_provider.is_empty() {
            HAIR_SCALP_PROVIDERS[0]
        } else {
            &self.active_provider
        }
    }

    pub fn duplicate_part(&mut self, id: u64) -> Option<u64> {
        let source = self.parts.iter().find(|part| part.id == id)?.clone();
        self.next_part_id += 1;
        let new_id = self.next_part_id;
        let mut copy = source;
        copy.id = new_id;
        copy.name = format!("Hair {new_id}");
        copy.settings.hide_scalp_cap();
        self.parts.push(copy);
        self.activate_part(new_id, false);
        self.touch(new_id);
        Some(new_id)
    }

    pub fn mirror_part(&mut self, id: u64, scalp: &ScalpAuthoring) -> bool {
        let Some(part) = self.parts.iter_mut().find(|part| part.id == id) else {
            return false;
        };
        let mut mirrored: BTreeMap<u32, HairStrand> = BTreeMap::new();
        for (scalp_index, strand) in &part.strands {
            let paired = scalp
                .mirror_pair
                .get(*scalp_index as usize)
                .copied()
                .unwrap_or(*scalp_index);
            let points_cm = strand
                .points_cm
                .iter()
                .map(|point| [-point[0], point[1], point[2]])
                .collect();
            mirrored.entry(paired).or_insert(HairStrand { points_cm });
        }
        part.strands = mirrored;
        self.touch(id);
        true
    }

    pub fn bump(&mut self) {
        self.edit_revision = self.edit_revision.wrapping_add(1);
    }

    pub fn touch(&mut self, id: u64) {
        self.bump();
        let stamp = self.edit_revision;
        if let Some(part) = self.parts.iter_mut().find(|part| part.id == id) {
            part.revision = stamp;
        }
    }

    fn touch_every_part(&mut self) {
        self.bump();
        let stamp = self.edit_revision;
        for part in &mut self.parts {
            part.revision = stamp;
        }
    }

    pub fn add_part(&mut self, provider_name: &str) -> u64 {
        self.next_part_id += 1;
        let id = self.next_part_id;
        self.parts.push(HairPart {
            kind: HairPartKind::Hair,
            revision: 0,
            scalp_texture: HairScalpTexture::default(),
            settings: HairSettings::default(),
            id,
            name: format!("Hair {id}"),
            visible: true,
            provider_name: provider_name.to_owned(),
            segments: DEFAULT_HAIR_SEGMENTS,
            segment_length_cm: DEFAULT_SEGMENT_LENGTH_CM,
            strands: BTreeMap::new(),
            style_joints: false,
        });
        self.activate_part(id, false);
        self.touch(id);
        id
    }

    #[must_use]
    pub fn scalp_part_id(&self) -> Option<u64> {
        self.parts
            .iter()
            .find(|part| part.kind.is_scalp())
            .map(|part| part.id)
    }

    #[must_use]
    pub fn editing_scalp_part_id(&self) -> Option<u64> {
        self.selected_part()
            .filter(|part| part.kind.is_scalp())
            .map(|part| part.id)
            .or_else(|| self.scalp_part_id())
    }

    #[must_use]
    pub fn next_scalp_name(&self) -> String {
        let taken = |name: &str| self.parts.iter().any(|part| part.name == name);
        if !taken(SCALP_PART_NAME) {
            return SCALP_PART_NAME.to_owned();
        }
        (2..)
            .map(|nth| format!("{SCALP_PART_NAME} {nth}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| SCALP_PART_NAME.to_owned())
    }

    pub fn add_scalp_part(&mut self, provider_name: &str) -> u64 {
        self.next_part_id += 1;
        let id = self.next_part_id;
        let mut settings = HairSettings::for_base_part();
        settings.wear(provider_name);
        let name = self.next_scalp_name();
        self.parts.insert(
            0,
            HairPart {
                kind: HairPartKind::Scalp,
                revision: 0,
                scalp_texture: HairScalpTexture::default(),
                settings,
                id,
                name,
                visible: true,
                provider_name: provider_name.to_owned(),
                segments: DEFAULT_HAIR_SEGMENTS,
                segment_length_cm: DEFAULT_SEGMENT_LENGTH_CM,
                strands: BTreeMap::new(),
                style_joints: false,
            },
        );
        self.touch(id);
        id
    }

    pub fn adopt_scalp(
        &mut self,
        provider_name: &str,
        settings: HairSettings,
        scalp_texture: HairScalpTexture,
        id: u64,
    ) {
        let mut settings = settings;
        settings.wear(provider_name);
        let name = self.next_scalp_name();
        self.parts.insert(
            0,
            HairPart {
                kind: HairPartKind::Scalp,
                revision: 0,
                scalp_texture,
                settings,
                id,
                name,
                visible: true,
                provider_name: provider_name.to_owned(),
                segments: DEFAULT_HAIR_SEGMENTS,
                segment_length_cm: DEFAULT_SEGMENT_LENGTH_CM,
                strands: BTreeMap::new(),
                style_joints: false,
            },
        );
    }

    pub fn remove_part(&mut self, id: u64) {
        let Some(index) = self.parts.iter().position(|part| part.id == id) else {
            return;
        };
        self.parts.remove(index);
        self.active_part_ids.remove(&id);
        if self.selected_part_id == Some(id) {
            self.selected_part_id = self
                .parts
                .get(index)
                .or_else(|| self.parts.get(index.saturating_sub(1)))
                .map(|part| part.id);
            if let Some(next) = self.selected_part_id {
                self.active_part_ids.insert(next);
            }
        }
        self.bump();
    }

    pub fn toggle_part_visible(&mut self, id: u64) {
        if let Some(part) = self.parts.iter_mut().find(|part| part.id == id) {
            part.visible = !part.visible;
            self.touch(id);
        }
    }

    pub fn part(&self, id: u64) -> Option<&HairPart> {
        self.parts.iter().find(|part| part.id == id)
    }

    pub fn selected_part(&self) -> Option<&HairPart> {
        self.selected_part_id.and_then(|id| self.part(id))
    }
}

impl HairPart {
    pub fn plant(&mut self, scalp: &ScalpAuthoring, scalp_indices: &[u32]) -> usize {
        let normals = scalp.normals();
        let mut planted = 0;
        for &index in scalp_indices {
            let slot = index as usize;
            if slot >= scalp.vertices_cm.len() || self.strands.contains_key(&index) {
                continue;
            }
            let root = scalp.vertices_cm[slot];
            let normal = normals.get(slot).copied().unwrap_or([0.0, 1.0, 0.0]);
            let mut points_cm = Vec::with_capacity(self.segments);
            for step in 0..self.segments {
                let reach = self.segment_length_cm * step as f32;
                points_cm.push([
                    root[0] + normal[0] * reach,
                    root[1] + normal[1] * reach,
                    root[2] + normal[2] * reach,
                ]);
            }
            self.strands.insert(index, HairStrand { points_cm });
            planted += 1;
        }
        planted
    }

    pub fn reset_strand_shapes(&mut self, scalp: &ScalpAuthoring) -> usize {
        let planted: Vec<u32> = self.strands.keys().copied().collect();
        for index in &planted {
            self.strands.remove(index);
        }
        self.plant(scalp, &planted)
    }

    /// Carry every strand from the cap it was planted on over to another one.
    ///
    /// A strand is keyed by the scalp vertex it grows from, and no two caps
    /// share a vertex ordering — Udane has 868 of them, Leyton and Soleil 922,
    /// Krayon 1948 — so a key kept across a swap names a different place on the
    /// head. Each root is re-seated on the vertex of the new cap nearest where
    /// it stood and the whole strand travels with it, so a style keeps its
    /// shape and stays where it was drawn.
    ///
    /// Two roots can land on one vertex when the new cap is the coarser of the
    /// two; the nearer one keeps the seat. Returns how many strands were lost
    /// that way, which is what the caller has to be honest about.
    pub fn reseat_onto(&mut self, from: &ScalpAuthoring, to: &ScalpAuthoring) -> usize {
        let mut claimed: BTreeMap<u32, (f32, HairStrand)> = BTreeMap::new();
        let mut lost = 0;
        for (index, strand) in std::mem::take(&mut self.strands) {
            let Some(root) = from.vertices_cm.get(index as usize).copied() else {
                lost += 1;
                continue;
            };
            let Some((seat, reach)) = nearest_scalp_vertex(&to.vertices_cm, root) else {
                lost += 1;
                continue;
            };
            let landing = to.vertices_cm[seat as usize];
            let shift = [
                landing[0] - root[0],
                landing[1] - root[1],
                landing[2] - root[2],
            ];
            let carried = HairStrand {
                points_cm: strand
                    .points_cm
                    .iter()
                    .map(|point| {
                        [
                            point[0] + shift[0],
                            point[1] + shift[1],
                            point[2] + shift[2],
                        ]
                    })
                    .collect(),
            };
            match claimed.get(&seat) {
                Some((held, _)) if *held <= reach => lost += 1,
                Some(_) => {
                    claimed.insert(seat, (reach, carried));
                    lost += 1;
                }
                None => {
                    claimed.insert(seat, (reach, carried));
                }
            }
        }
        self.strands = claimed
            .into_iter()
            .map(|(seat, (_, strand))| (seat, strand))
            .collect();
        lost
    }

    pub fn minimum_strand_length_cm(&self) -> f32 {
        let segments = self.segments.saturating_sub(1).max(1) as f32;
        (self.segment_length_cm * segments * 0.05).max(0.2)
    }

    pub fn scale_strands(&mut self, scalp_indices: &[u32], factor: f32) -> usize {
        let factor = factor.clamp(0.25, 4.0);
        let floor = self.minimum_strand_length_cm();
        let mut touched = 0;
        for index in scalp_indices {
            let Some(strand) = self.strands.get_mut(index) else {
                continue;
            };
            let total: f32 = strand
                .points_cm
                .windows(2)
                .map(|pair| {
                    let dx = pair[1][0] - pair[0][0];
                    let dy = pair[1][1] - pair[0][1];
                    let dz = pair[1][2] - pair[0][2];
                    (dx * dx + dy * dy + dz * dz).sqrt()
                })
                .sum();
            let factor = if factor > 1.0 && total > 1.0e-4 && total * factor < floor {
                floor / total
            } else {
                factor
            };
            let root = strand.points_cm[0];
            for point in strand.points_cm.iter_mut().skip(1) {
                point[0] = root[0] + (point[0] - root[0]) * factor;
                point[1] = root[1] + (point[1] - root[1]) * factor;
                point[2] = root[2] + (point[2] - root[2]) * factor;
            }
            touched += 1;
        }
        touched
    }

    pub fn set_strand_points(&mut self, strands: Vec<(u32, Vec<[f32; 3]>)>) -> usize {
        let mut applied = 0;
        for (index, points) in strands {
            let Some(strand) = self.strands.get_mut(&index) else {
                continue;
            };
            if points.len() != strand.points_cm.len() || points[0] != strand.points_cm[0] {
                continue;
            }
            strand.points_cm = points;
            applied += 1;
        }
        applied
    }

    pub fn unplant(&mut self, scalp_indices: &[u32]) -> usize {
        let before = self.strands.len();
        for index in scalp_indices {
            self.strands.remove(index);
        }
        before - self.strands.len()
    }

    pub fn resample_all(&mut self) {
        let segments = self.segments.clamp(2, MAX_HAIR_SEGMENTS);
        for strand in self.strands.values_mut() {
            strand.points_cm = resample_polyline(&strand.points_cm, segments);
        }
    }
}

pub fn resample_polyline(points: &[[f32; 3]], segments: usize) -> Vec<[f32; 3]> {
    let segments = segments.max(2);
    if points.len() == segments {
        return points.to_vec();
    }
    if points.len() < 2 {
        let point = points.first().copied().unwrap_or_default();
        return vec![point; segments];
    }
    let mut lengths = Vec::with_capacity(points.len());
    lengths.push(0.0f32);
    for pair in points.windows(2) {
        let d = [
            pair[1][0] - pair[0][0],
            pair[1][1] - pair[0][1],
            pair[1][2] - pair[0][2],
        ];
        let step = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        lengths.push(lengths.last().unwrap() + step);
    }
    let total = *lengths.last().unwrap();
    if total <= 0.0 {
        return vec![points[0]; segments];
    }
    let mut output = Vec::with_capacity(segments);
    let mut cursor = 1usize;
    for sample in 0..segments {
        let target = total * sample as f32 / (segments - 1) as f32;
        while cursor < lengths.len() - 1 && lengths[cursor] < target {
            cursor += 1;
        }
        let previous = lengths[cursor - 1];
        let next = lengths[cursor];
        let factor = if next > previous {
            (target - previous) / (next - previous)
        } else {
            0.0
        };
        let a = points[cursor - 1];
        let b = points[cursor];
        output.push([
            a[0] + (b[0] - a[0]) * factor,
            a[1] + (b[1] - a[1]) * factor,
            a[2] + (b[2] - a[2]) * factor,
        ]);
    }
    output
}

/// The vertex of a cap nearest a point, with the distance it stands away.
fn nearest_scalp_vertex(vertices_cm: &[[f32; 3]], point: [f32; 3]) -> Option<(u32, f32)> {
    let mut best: Option<(u32, f32)> = None;
    for (index, vertex) in vertices_cm.iter().enumerate() {
        let reach = (vertex[0] - point[0]).powi(2)
            + (vertex[1] - point[1]).powi(2)
            + (vertex[2] - point[2]).powi(2);
        if best.is_none_or(|(_, held)| reach < held) {
            best = Some((index as u32, reach));
        }
    }
    best.map(|(index, reach)| (index, reach.sqrt()))
}

pub fn build_scalp_authoring(scalp: &BuiltinHairScalp) -> Result<ScalpAuthoring, String> {
    let stored = &scalp.geometry.vertices_cm;
    if stored.is_empty() {
        return Err(format!(
            "scalp provider {} has no vertices",
            scalp.provider_name
        ));
    }

    let negated: Vec<[f32; 3]> = stored.iter().map(|v| [-v[0], v[1], v[2]]).collect();
    let export_negate_x = true;
    let vertices_cm: Vec<[f32; 3]> = if export_negate_x {
        negated
    } else {
        stored.clone()
    };

    let vertices_f64: Vec<[f64; 3]> = vertices_cm
        .iter()
        .map(|v| [v[0] as f64, v[1] as f64, v[2] as f64])
        .collect();
    let triangles = scalp.geometry.triangles.clone();
    let picking_triangles: Vec<[u32; 3]> = if export_negate_x {
        triangles.iter().map(|t| [t[0], t[2], t[1]]).collect()
    } else {
        triangles.clone()
    };
    let mesh = Mesh::new(vertices_f64, picking_triangles)
        .map_err(|err| format!("scalp mesh rejected: {err}"))?;
    let surface = SurfaceMesh::new(mesh).map_err(|err| format!("scalp surface rejected: {err}"))?;

    let mut normals: Vec<[f32; 3]> = surface.normals.as_slice().to_vec();
    let count = vertices_cm.len() as f32;
    let centroid = vertices_cm.iter().fold([0.0f32; 3], |acc, v| {
        [
            acc[0] + v[0] / count,
            acc[1] + v[1] / count,
            acc[2] + v[2] / count,
        ]
    });
    let outwardness: f32 = vertices_cm
        .iter()
        .zip(&normals)
        .map(|(vertex, normal)| {
            (vertex[0] - centroid[0]) * normal[0]
                + (vertex[1] - centroid[1]) * normal[1]
                + (vertex[2] - centroid[2]) * normal[2]
        })
        .sum();
    if outwardness < 0.0 {
        for normal in &mut normals {
            normal[0] = -normal[0];
            normal[1] = -normal[1];
            normal[2] = -normal[2];
        }
    }

    let mut mirror_pair = Vec::with_capacity(vertices_cm.len());
    for vertex in &vertices_cm {
        let mirrored = [-vertex[0], vertex[1], vertex[2]];
        let mut best = 0u32;
        let mut best_distance = f32::INFINITY;
        for (candidate_index, candidate) in vertices_cm.iter().enumerate() {
            let dx = candidate[0] - mirrored[0];
            let dy = candidate[1] - mirrored[1];
            let dz = candidate[2] - mirrored[2];
            let distance = dx * dx + dy * dy + dz * dz;
            if distance < best_distance {
                best_distance = distance;
                best = candidate_index as u32;
            }
        }
        mirror_pair.push(best);
    }

    Ok(ScalpAuthoring {
        vertices_cm,
        triangles,
        uvs: scalp.geometry.uvs.clone(),
        surface: Arc::new(surface),
        normals,
        export_negate_x,
        mirror_pair,
    })
}

pub fn hair_part_from_preset(
    geometry: &vkit_core::vam::HairGuideGeometry,
    scalp: &ScalpAuthoring,
    settings: HairSettings,
    scalp_texture: HairScalpTexture,
    name: String,
    id: u64,
    revision: u64,
) -> Result<HairPart, String> {
    if geometry.scalp_vertex_count != scalp.vertices_cm.len() {
        return Err(format!(
            "{} is planted on a {}-vertex scalp; ours has {}",
            geometry.provider_name,
            geometry.scalp_vertex_count,
            scalp.vertices_cm.len()
        ));
    }
    let segments = geometry.segments.clamp(2, MAX_HAIR_SEGMENTS);
    let mut strands = BTreeMap::new();
    for guide in &geometry.guides {
        if guide.points_cm.len() < 2 || guide.scalp_index as usize >= scalp.vertices_cm.len() {
            continue;
        }
        let flipped: Vec<[f32; 3]> = guide
            .points_cm
            .iter()
            .map(|point| {
                if scalp.export_negate_x {
                    [-point[0], point[1], point[2]]
                } else {
                    *point
                }
            })
            .collect();
        let points_cm = if flipped.len() == segments {
            flipped
        } else {
            resample_polyline(&flipped, segments)
        };
        strands.insert(guide.scalp_index, HairStrand { points_cm });
    }
    if strands.is_empty() {
        return Err(format!(
            "{} planted nothing we can edit",
            geometry.provider_name
        ));
    }
    Ok(HairPart {
        kind: HairPartKind::Hair,
        revision,
        scalp_texture,
        settings,
        id,
        name,
        visible: true,
        provider_name: geometry.provider_name.clone(),
        segments,
        segment_length_cm: if geometry.segment_length_cm.is_finite()
            && geometry.segment_length_cm > 0.0
        {
            geometry.segment_length_cm
        } else {
            DEFAULT_SEGMENT_LENGTH_CM
        },
        strands,
        style_joints: !geometry.nearby_joints.is_empty(),
    })
}

impl HairProject {
    pub fn adopt_parts(&mut self, parts: Vec<HairPart>) {
        self.parts = parts;
        self.selected_part_id = None;
        self.active_part_ids.clear();
        self.touch_every_part();
    }

    pub fn next_ids(&mut self, count: usize) -> Vec<u64> {
        (0..count)
            .map(|_| {
                self.next_part_id += 1;
                self.next_part_id
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vkit_core::vam::HairScalpGeometry;

    fn synthetic_cap() -> BuiltinHairScalp {
        BuiltinHairScalp {
            provider_name: "TestScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                vertices_cm: vec![
                    [-1.0, 10.0, -1.0],
                    [1.0, 10.0, -1.0],
                    [1.0, 10.0, 1.0],
                    [-1.0, 10.0, 1.0],
                    [0.0, 10.5, 0.0],
                ],
                uvs: vec![[0.0, 0.0]; 5],
                triangles: vec![[0, 4, 1], [1, 4, 2], [2, 4, 3], [3, 4, 0]],
            },
        }
    }

    #[test]
    fn scalp_normals_point_out_of_the_head_even_when_mirrored() {
        let scalp = synthetic_cap();
        let authoring = build_scalp_authoring(&scalp).expect("build");
        assert!(authoring.export_negate_x);

        let count = authoring.vertices_cm.len() as f32;
        let centroid = authoring.vertices_cm.iter().fold([0.0f32; 3], |acc, v| {
            [
                acc[0] + v[0] / count,
                acc[1] + v[1] / count,
                acc[2] + v[2] / count,
            ]
        });
        for (vertex, normal) in authoring.vertices_cm.iter().zip(authoring.normals()) {
            let outward = (vertex[0] - centroid[0]) * normal[0]
                + (vertex[1] - centroid[1] + 0.25) * normal[1]
                + (vertex[2] - centroid[2]) * normal[2];
            assert!(
                normal[1] > 0.0,
                "normal {normal:?} at {vertex:?} points into the head (outward metric {outward})"
            );
        }
    }

    #[test]
    fn planted_strands_grow_along_the_corrected_normals() {
        let scalp = synthetic_cap();
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let mut project = HairProject::default();
        let id = project.add_part("TestScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.plant(&authoring, &[0, 1, 2, 3]);

        for strand in part.strands.values() {
            let root = strand.points_cm[0];
            let tip = strand.points_cm.last().unwrap();
            assert!(
                tip[1] > root[1],
                "strand grew downward: root {root:?} tip {tip:?}"
            );
        }
    }

    #[test]
    fn parts_get_stable_ids_and_neighbor_selection_on_remove() {
        let mut project = HairProject::default();
        let a = project.add_part("UdaneScalp");
        let b = project.add_part("UdaneScalp");
        let c = project.add_part("LeytonScalp");
        assert_eq!(project.selected_part_id, Some(c));

        project.activate_part(b, true);
        project.remove_part(b);
        assert_eq!(project.selected_part_id, Some(c));
        project.remove_part(c);
        assert_eq!(project.selected_part_id, Some(a));
        project.remove_part(a);
        assert_eq!(project.selected_part_id, None);

        let d = project.add_part("SoleilScalp");
        assert!(d > c);
    }

    #[test]
    fn a_crushed_strand_snaps_to_the_floor_and_grows_from_there() {
        let mut project = HairProject::default();
        let id = project.add_part("TestScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.segments = 8;
        part.segment_length_cm = 1.0;
        part.strands.insert(
            0,
            HairStrand {
                points_cm: (0..8)
                    .map(|i| [0.0, 10.0 + i as f32 * 1.0e-4, 0.0])
                    .collect(),
            },
        );
        let floor = part.minimum_strand_length_cm();
        assert!(floor > 0.1, "the floor is a visible stub: {floor}");

        part.scale_strands(&[0], 1.01);
        let part = project.parts.iter().find(|p| p.id == id).unwrap();
        let total: f32 = part.strands[&0]
            .points_cm
            .windows(2)
            .map(|pair| {
                ((pair[1][0] - pair[0][0]).powi(2)
                    + (pair[1][1] - pair[0][1]).powi(2)
                    + (pair[1][2] - pair[0][2]).powi(2))
                .sqrt()
            })
            .sum();
        assert!(
            total >= floor * 0.99,
            "one grow tick lifts the strand to the floor: {total} vs {floor}",
        );
    }

    #[test]
    fn a_new_cap_wears_the_colour_a_visible_scalp_wears_and_a_copy_of_it_wears_nothing() {
        use crate::hair_settings::{
            COLOR_CHANNELS, SCALP_COLOR_KEY, SCALP_OPACITY_KEY, VISIBLE_SCALP_COLOR, param_by_key,
        };

        let opacity = param_by_key(SCALP_OPACITY_KEY).expect("the cap has an opacity");
        let cap = param_by_key(SCALP_COLOR_KEY).expect("the cap has a colour");
        let root = param_by_key("rootColor").expect("the strands have a root colour");

        let mut project = HairProject::default();
        let base = project.add_scalp_part("UdaneScalp");
        let settings = |project: &HairProject, id: u64| {
            project
                .parts
                .iter()
                .find(|part| part.id == id)
                .map(|part| part.settings.clone())
                .expect("the part is in the project")
        };

        let plain = HairSettings::default();
        let worn = settings(&project, base);
        for channel in 0..COLOR_CHANNELS.len() {
            assert_eq!(
                worn.color_channel(cap, channel),
                VISIBLE_SCALP_COLOR[channel.min(2)],
                "channel {channel} of the cap is what a scalp meant to be seen wears"
            );
            assert!(
                worn.color_channel(cap, channel) > plain.color_channel(root, channel),
                "the cap has to read apart from the strands standing on it"
            );
        }
        assert!(
            worn.color_channel(cap, 0) > 3.0 * plain.color_channel(root, 0).max(8.0),
            "a scalp is skin, not the hair colour lifted a step: the library puts              it around twelve times the root's luminance"
        );
        assert!(
            worn.color_channel(cap, 0) < plain.color_channel(cap, 0),
            "and it is still far from the white the cap ships with"
        );

        let copy = project.duplicate_part(base).expect("the part duplicates");
        let copied = settings(&project, copy);
        assert_eq!(
            copied.get(opacity),
            opacity.default,
            "two caps on one head is one cap too many"
        );
        for channel in 0..COLOR_CHANNELS.len() {
            assert_eq!(
                copied.color_channel(cap, channel),
                worn.color_channel(cap, channel),
                "the tint rides along, so revealing the copy's cap matches"
            );
        }
    }

    #[test]
    fn a_change_to_one_part_leaves_every_other_part_alone() {
        let mut project = HairProject::default();
        let first = project.add_part("UdaneScalp");
        let second = project.add_part("UdaneScalp");
        let third = project.add_part("UdaneScalp");

        let stamp = |project: &HairProject, id: u64| {
            project
                .parts
                .iter()
                .find(|part| part.id == id)
                .map(|part| part.revision)
                .expect("the part is in the project")
        };
        let before = [
            stamp(&project, first),
            stamp(&project, second),
            stamp(&project, third),
        ];

        project.touch(second);
        assert_eq!(stamp(&project, first), before[0], "the first part is idle");
        assert_eq!(stamp(&project, third), before[2], "the third part is idle");
        assert_ne!(
            stamp(&project, second),
            before[1],
            "the part that changed says so"
        );

        assert!(
            stamp(&project, second) > before[1],
            "stamps only climb, so a number is never reused on different content"
        );
    }

    #[test]
    fn stepping_back_in_time_stamps_every_part_it_returns() {
        let mut project = HairProject::default();
        let first = project.add_part("UdaneScalp");
        project.record(HairEdit::Stroke);
        project.touch(first);
        let stamp = |project: &HairProject| project.parts[0].revision;
        let after_edit = stamp(&project);

        assert!(project.undo(), "there is a step to take back");
        assert!(
            stamp(&project) > after_edit,
            "an older document must not arrive wearing an older stamp, or a              cache keyed on it hands back the preview of a future that was undone"
        );
    }

    #[test]
    fn a_preset_becomes_layers_in_our_own_handedness_on_the_same_scalp_vertices() {
        use crate::hair_settings::HairSettings;
        use vkit_core::vam::{BuiltinHairScalp, HairGuide, HairGuideGeometry, HairScalpGeometry};

        let vertices_cm: Vec<[f32; 3]> = (0..6)
            .map(|index| [index as f32, 10.0, if index % 2 == 0 { 0.0 } else { 1.0 }])
            .collect();
        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                uvs: vec![[0.0, 0.0]; vertices_cm.len()],
                triangles: vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
                vertices_cm: vertices_cm.clone(),
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        assert!(authoring.export_negate_x, "the fixture exercises the flip");

        let file_points = vec![[1.0_f32, 10.0, 0.0], [1.5, 11.0, 0.0], [2.0, 12.0, 0.0]];
        let geometry = HairGuideGeometry {
            provider_name: "UdaneScalp".to_owned(),
            segments: 3,
            segment_length_cm: 1.25,
            scalp_vertex_count: vertices_cm.len(),
            guides: vec![HairGuide {
                scalp_index: 4,
                points_cm: file_points.clone(),
                rigidity: vec![1.0; 3],
            }],
            guide_triangles: Vec::new(),
            root_map: vec![4],
            nearby_joints: Vec::new(),
        };

        let part = hair_part_from_preset(
            &geometry,
            &authoring,
            HairSettings::default(),
            HairScalpTexture::default(),
            "Bob".to_owned(),
            7,
            0,
        )
        .expect("the preset converts");

        assert_eq!(part.provider_name, "UdaneScalp");
        assert_eq!(part.segments, 3);
        assert!((part.segment_length_cm - 1.25).abs() < f32::EPSILON);
        assert!(!part.style_joints, "this preset ships no joints");

        let strand = part.strands.get(&4).expect("the scalp index is untouched");
        for (authored, from_file) in strand.points_cm.iter().zip(&file_points) {
            assert!(
                (authored[0] + from_file[0]).abs() < 1.0e-4,
                "X must arrive flipped: {authored:?} against {from_file:?}"
            );
            assert!((authored[1] - from_file[1]).abs() < 1.0e-4);
            assert!((authored[2] - from_file[2]).abs() < 1.0e-4);
        }

        let round_tripped = crate::hair_export::export_doc(&part, &authoring).expect("doc");
        assert_eq!(
            round_tripped.strands_by_scalp_cm[&4], file_points,
            "what came out of the file is what goes back into it"
        );
    }

    #[test]
    fn a_preset_planted_on_a_scalp_we_do_not_have_is_refused_rather_than_scattered() {
        use crate::hair_settings::HairSettings;
        use vkit_core::vam::{BuiltinHairScalp, HairGuide, HairGuideGeometry, HairScalpGeometry};

        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                uvs: vec![[0.0, 0.0]; 4],
                triangles: vec![[0, 1, 2], [1, 3, 2]],
                vertices_cm: vec![
                    [0.0, 10.0, 0.0],
                    [1.0, 10.0, 0.0],
                    [0.0, 10.0, 1.0],
                    [1.0, 10.0, 1.0],
                ],
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let geometry = HairGuideGeometry {
            provider_name: "UdaneScalp".to_owned(),
            segments: 2,
            segment_length_cm: 1.0,
            scalp_vertex_count: 9000,
            guides: vec![HairGuide {
                scalp_index: 0,
                points_cm: vec![[0.0, 10.0, 0.0], [0.0, 11.0, 0.0]],
                rigidity: vec![1.0; 2],
            }],
            guide_triangles: Vec::new(),
            root_map: vec![0],
            nearby_joints: Vec::new(),
        };

        let refused = hair_part_from_preset(
            &geometry,
            &authoring,
            HairSettings::default(),
            HairScalpTexture::default(),
            "Bob".to_owned(),
            1,
            0,
        );
        assert!(
            refused.is_err(),
            "a scalp of a different size means the indices mean something else"
        );
    }

    #[test]
    fn clicking_the_only_lit_layer_again_puts_every_layer_out() {
        let mut project = HairProject::default();
        let first = project.add_part("UdaneScalp");
        let second = project.add_part("UdaneScalp");

        assert_eq!(
            project.editable_parts(),
            vec![second],
            "a new layer arrives lit"
        );

        project.activate_part(first, false);
        assert_eq!(project.editable_parts(), vec![first]);
        assert_eq!(project.selected_part_id, Some(first));

        project.activate_part(first, false);
        assert!(
            project.editable_parts().is_empty(),
            "a second plain click puts the layer out, so nothing is highlighted"
        );
        assert_eq!(project.selected_part_id, None);

        project.activate_part(first, false);
        assert_eq!(
            project.editable_parts(),
            vec![first],
            "and a third click lights it again"
        );

        project.activate_part(second, true);
        assert_eq!(project.editable_parts(), vec![first, second]);
        project.activate_part(second, false);
        assert_eq!(
            project.editable_parts(),
            vec![second],
            "a plain click on one of several narrows to it rather than putting it out"
        );
    }

    #[test]
    fn an_imported_part_carries_the_sheet_it_was_handed_all_the_way_to_the_file() {
        use crate::hair_settings::HairSettings;
        use vkit_core::vam::{BuiltinHairScalp, HairGuide, HairGuideGeometry, HairScalpGeometry};

        let vertices_cm: Vec<[f32; 3]> = (0..6)
            .map(|index| [index as f32, 10.0, if index % 2 == 0 { 0.0 } else { 1.0 }])
            .collect();
        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                uvs: vec![[0.0, 0.0]; vertices_cm.len()],
                triangles: vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
                vertices_cm: vertices_cm.clone(),
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let geometry = HairGuideGeometry {
            provider_name: "UdaneScalp".to_owned(),
            segments: 2,
            segment_length_cm: 1.0,
            scalp_vertex_count: vertices_cm.len(),
            guides: vec![HairGuide {
                scalp_index: 0,
                points_cm: vec![[0.0, 10.0, 0.0], [0.0, 11.0, 0.0]],
                rigidity: vec![1.0; 2],
            }],
            guide_triangles: Vec::new(),
            root_map: vec![0],
            nearby_joints: Vec::new(),
        };

        let sheet = std::path::PathBuf::from("scalp sheet.PNG");
        let part = hair_part_from_preset(
            &geometry,
            &authoring,
            HairSettings::default(),
            HairScalpTexture {
                diffuse: Some(sheet.clone()),
                alpha: None,
            },
            "Bob".to_owned(),
            1,
            0,
        )
        .expect("the preset converts");

        assert_eq!(
            part.scalp_texture,
            HairScalpTexture {
                diffuse: Some(sheet),
                alpha: None
            },
            "a sheet pulled out of a .var must ride on the part, or the cap              comes back wearing the provider's own skin"
        );

        let material = crate::hair_export::authoring_scalp_material(&part);
        assert!(
            material
                .diffuse_color
                .iter()
                .all(|channel| channel.is_finite()),
            "the material still resolves with a custom sheet on it"
        );
    }

    #[test]
    fn a_hair_layer_no_longer_smuggles_the_cap() {
        let mut project = HairProject::default();
        let first = project.add_part("UdaneScalp");
        assert!(
            !project.part(first).unwrap().settings.shows_scalp_cap(),
            "the cap belongs to the scalp layer now, so growing hair must not raise one",
        );
        assert_eq!(project.scalp_part_id(), None);
    }

    #[test]
    fn cap_layers_stack_and_the_page_follows_the_selected_one() {
        let mut project = HairProject::default();
        project.add_part("UdaneScalp");
        let scalp = project.add_scalp_part("UdaneScalp");

        assert_eq!(
            project.parts.first().unwrap().id,
            scalp,
            "the cap heads the list"
        );
        assert_eq!(project.scalp_part_id(), Some(scalp));
        let part = project.part(scalp).unwrap();
        assert!(part.kind.is_scalp());
        assert!(part.strands.is_empty(), "a cap plants no hair");
        assert!(
            part.settings.shows_scalp_cap(),
            "a cap that cannot be seen is not worth a layer",
        );
        assert_eq!(part.name, SCALP_PART_NAME);

        let second = project.add_scalp_part("LeytonScalp");
        assert_ne!(second, scalp, "VaM styles do wear two caps at once");
        assert_eq!(
            project.parts.iter().filter(|p| p.kind.is_scalp()).count(),
            2
        );
        assert_eq!(
            project.part(second).unwrap().name,
            "scalp 2",
            "a second cap says so in the layer list",
        );
        project.activate_part(scalp, false);
        assert_eq!(
            project.editing_scalp_part_id(),
            Some(scalp),
            "the page follows the selection once there is more than one cap",
        );
    }

    #[test]
    fn the_scalp_page_is_reachable_now_that_it_is_not_a_parameter_group() {
        assert!(
            !crate::hair_settings::HairParamGroup::ALL
                .contains(&crate::hair_settings::HairParamGroup::Scalp),
            "the cap has its own page, so it must not also claim a parameter segment",
        );
        assert!(
            crate::hair_settings::HAIR_PARAMS
                .iter()
                .any(|param| param.group == crate::hair_settings::HairParamGroup::Scalp),
            "a page with nothing on it is worse than no page",
        );
    }

    #[test]
    fn cap_names_never_collide_after_one_is_deleted() {
        let mut project = HairProject::default();
        let first = project.add_scalp_part("UdaneScalp");
        let second = project.add_scalp_part("LeytonScalp");
        assert_eq!(project.part(first).unwrap().name, "scalp");
        assert_eq!(project.part(second).unwrap().name, "scalp 2");

        project.remove_part(first);
        let third = project.add_scalp_part("KrayonScalp");
        assert_eq!(
            project.part(third).unwrap().name,
            "scalp",
            "counting the survivors would have handed out scalp 2 twice, and two              layers of one name overwrite each other on export",
        );
        let names: Vec<&str> = project.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1]);
    }

    #[test]
    fn a_cap_layer_still_builds_the_geometry_the_viewport_draws_it_from() {
        let mut project = HairProject::default();
        let id = project.add_scalp_part("UdaneScalp");
        let authoring = build_scalp_authoring(&synthetic_cap()).expect("scalp");
        let part = project.part(id).unwrap();

        let geometry = crate::hair_export::authoring_guide_geometry(part, &authoring, false)
            .expect("a cap with no hair on it is still something to draw");
        assert!(geometry.guides.is_empty());
        assert_eq!(geometry.provider_name, "UdaneScalp");
    }

    #[test]
    fn a_cap_layer_exports_as_a_hair_item_with_no_guides() {
        let mut project = HairProject::default();
        let id = project.add_scalp_part("UdaneScalp");
        let authoring = build_scalp_authoring(&synthetic_cap()).expect("scalp");
        let part = project.part(id).unwrap();

        let doc = crate::hair_export::export_doc(part, &authoring)
            .expect("VaM ships hairs that are nothing but a cap, so we must be able to write one");
        assert!(
            doc.strands_by_scalp_cm.is_empty(),
            "the cap layer grows nothing",
        );
        assert_eq!(doc.provider_name, "UdaneScalp");
    }
}

#[cfg(test)]
mod reseat_tests {
    use super::*;

    fn cap(provider: &str, vertices_cm: Vec<[f32; 3]>) -> ScalpAuthoring {
        let count = vertices_cm.len();
        let triangles = (2..count)
            .map(|corner| [0, (corner - 1) as u32, corner as u32])
            .collect();
        build_scalp_authoring(&BuiltinHairScalp {
            provider_name: provider.to_owned(),
            geometry: vkit_core::vam::HairScalpGeometry {
                materials: Vec::new(),
                vertices_cm,
                uvs: vec![[0.0, 0.0]; count],
                triangles,
            },
        })
        .expect("a cap")
    }

    /// The caps do not share a vertex ordering, so a strand that keeps its key
    /// across a swap grows from somewhere else entirely.
    #[test]
    fn a_strand_lands_on_the_vertex_nearest_where_it_stood() {
        // Vertex 2 of the worn cap stands at x = 4. On the wanted cap the
        // nearest vertex to that is index 1, not index 2.
        let worn = cap(
            "UdaneScalp",
            vec![[0.0, 10.0, 0.0], [2.0, 10.0, 0.0], [4.0, 10.0, 0.0]],
        );
        let wanted = cap(
            "LeytonScalp",
            vec![
                [-9.0, 10.0, 0.0],
                [4.2, 10.0, 0.0],
                [-4.0, 10.0, 0.0],
                [9.0, 10.0, 0.0],
            ],
        );
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|part| part.id == id).unwrap();
        part.plant(&worn, &[2]);
        let tip_before = part.strands[&2].points_cm[part.segments - 1];

        assert_eq!(part.reseat_onto(&worn, &wanted), 0);
        let seats: Vec<u32> = part.strands.keys().copied().collect();
        assert_eq!(
            seats,
            vec![1],
            "the root belongs on the nearest vertex of the cap it moved to",
        );
        let root = part.strands[&1].points_cm[0];
        // `build_scalp_authoring` negates x, so the caps read back mirrored;
        // what matters is that the root sits exactly on its new seat.
        assert!(
            (root[0] - wanted.vertices_cm[1][0]).abs() < 1.0e-4,
            "root landed at {root:?}, seat is {:?}",
            wanted.vertices_cm[1],
        );
        let tip_after = part.strands[&1].points_cm[part.segments - 1];
        let shift = [
            root[0] - worn.vertices_cm[2][0],
            root[1] - worn.vertices_cm[2][1],
            root[2] - worn.vertices_cm[2][2],
        ];
        for axis in 0..3 {
            assert!(
                (tip_after[axis] - (tip_before[axis] + shift[axis])).abs() < 1.0e-4,
                "the whole strand travels with its root, keeping its shape",
            );
        }
    }

    #[test]
    fn two_roots_crowding_one_seat_leave_the_nearer_one_and_are_counted() {
        let worn = cap(
            "UdaneScalp",
            vec![[0.0, 10.0, 0.0], [0.1, 10.0, 0.0], [8.0, 10.0, 0.0]],
        );
        let wanted = cap(
            "KrayonScalp",
            vec![[0.0, 10.0, 0.0], [8.0, 10.0, 0.0], [0.0, 10.0, 9.0]],
        );
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|part| part.id == id).unwrap();
        part.plant(&worn, &[0, 1, 2]);

        assert_eq!(
            part.reseat_onto(&worn, &wanted),
            1,
            "the coarser cap has one seat for two roots, and the loss is reported",
        );
        assert_eq!(part.strands.len(), 2);
        let root = part.strands[&0].points_cm[0];
        assert!(
            (root[0] - wanted.vertices_cm[0][0]).abs() < 1.0e-4,
            "the nearer root keeps the seat",
        );
    }

    #[test]
    fn nothing_is_lost_when_the_cap_is_the_one_already_worn() {
        let worn = cap(
            "UdaneScalp",
            vec![[0.0, 10.0, 0.0], [2.0, 10.0, 0.0], [4.0, 10.0, 0.0]],
        );
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|part| part.id == id).unwrap();
        part.plant(&worn, &[0, 1, 2]);
        let before: Vec<(u32, Vec<[f32; 3]>)> = part
            .strands
            .iter()
            .map(|(index, strand)| (*index, strand.points_cm.clone()))
            .collect();
        assert_eq!(part.reseat_onto(&worn, &worn), 0);
        let after: Vec<(u32, Vec<[f32; 3]>)> = part
            .strands
            .iter()
            .map(|(index, strand)| (*index, strand.points_cm.clone()))
            .collect();
        assert_eq!(after, before);
    }
}
