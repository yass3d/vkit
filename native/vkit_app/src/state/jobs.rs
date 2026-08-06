use super::*;

#[derive(Clone, Debug)]
pub(super) struct ProviderDiscoveryContext {
    pub(super) vam_root: Option<PathBuf>,
    pub(super) explicit_base: Option<PathBuf>,
    pub(super) locale: Locale,
}

impl ProviderDiscoveryContext {
    pub(super) fn explicit_candidates(&self, sex: GeometrySex) -> Vec<PathBuf> {
        let mut candidates = Vec::with_capacity(2);
        if let Some(path) = self.explicit_base.as_deref() {
            candidates.push(path.to_path_buf());
        }
        if let Some(path) = AppState::provider_environment_path(sex)
            && !candidates.contains(&path)
        {
            candidates.push(path);
        }
        candidates
    }

    pub(super) fn discover(
        &self,
        anchor: Option<&DazGeometry>,
        sex: GeometrySex,
    ) -> Result<DiscoveredGeometryBase, String> {
        let root = self
            .vam_root
            .as_deref()
            .and_then(|path| VaMRoot::open(path).ok());
        let explicit_candidates = self.explicit_candidates(sex);
        discover_geometry_base(GeometryBaseRequest {
            root: root.as_ref(),
            sex,
            licensed_anchor: anchor,
            explicit_candidates: &explicit_candidates,
            cache_dir: AppState::vam_extraction_cache_dir().as_deref(),
        })
        .map_err(|notes| {
            format!(
                "{} — {notes}",
                crate::i18n::text(self.locale, TextKey::VaMGeometryBaseHowTo)
            )
        })
    }

    pub(super) fn resolve_provider(
        &self,
        anchor: &DazGeometry,
        sex: GeometrySex,
    ) -> Result<(Arc<VaMGeometryProvider>, PathBuf, VaMGeometryBaseProvenance), String> {
        let discovered = self.discover(Some(anchor), sex)?;
        let provenance = VaMGeometryBaseProvenance::from_discovery(&discovered);
        Ok((
            Arc::new(discovered.provider),
            discovered.base_path,
            provenance,
        ))
    }
}

#[derive(Debug)]
pub enum WorkspaceLoadJob {
    Template(TemplateLoadJob),
    Result { path: PathBuf },
    VaMMorphPair(VaMPairLoadJob),
    DirectEditSource(DirectEditLoadJob),
}

impl WorkspaceLoadJob {
    pub fn kind(&self) -> WorkspaceLoadKind {
        match self {
            Self::Template(_) => WorkspaceLoadKind::Template,
            Self::Result { .. } => WorkspaceLoadKind::Result,
            Self::VaMMorphPair(_) => WorkspaceLoadKind::VaMMorphPair,
            Self::DirectEditSource(_) => WorkspaceLoadKind::DirectEditSource,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Template(job) => &job.path,
            Self::Result { path } => path,
            Self::VaMMorphPair(job) => &job.path,
            Self::DirectEditSource(job) => &job.source.path,
        }
    }

    pub fn run(self) -> WorkspaceLoadOutcome {
        match self {
            Self::Template(job) => WorkspaceLoadOutcome::Template(job.run().map(Box::new)),
            Self::Result { path } => WorkspaceLoadOutcome::Result(
                load_ordered_obj(&path)
                    .map(Box::new)
                    .map_err(|error| error.to_string()),
            ),
            Self::VaMMorphPair(job) => WorkspaceLoadOutcome::VaMMorphPair(
                AppState::prepare_vam_morph_pair_job(&job).map(Box::new),
            ),
            Self::DirectEditSource(job) => WorkspaceLoadOutcome::DirectEditSource(
                AppState::prepare_direct_edit_source_job(&job).map(Box::new),
            ),
        }
    }
}

#[derive(Debug)]
pub struct TemplateLoadJob {
    pub(super) path: PathBuf,
    pub(super) anchor: Option<Arc<DazGeometry>>,
    pub(super) cached_provider: Option<Arc<VaMGeometryProvider>>,
    pub(super) cached_provider_path: Option<PathBuf>,
    pub(super) cached_provenance: Option<VaMGeometryBaseProvenance>,
    pub(super) current_sex: FigureSex,
    pub(super) discovery: ProviderDiscoveryContext,
}

impl TemplateLoadJob {
    pub(super) fn run(&self) -> Result<PreparedTemplateLoad, String> {
        template_ingest::load_template_source(
            &self.path,
            TemplateLoadContext {
                anchor: self.anchor.as_deref(),
                cached_provider: self.cached_provider.as_ref(),
                cached_provider_path: self.cached_provider_path.as_deref(),
                cached_provenance: self.cached_provenance,
                current_sex: self.current_sex,
            },
            |anchor, sex| self.discovery.resolve_provider(anchor, sex),
        )
    }
}

#[derive(Debug)]
pub struct VaMPairLoadJob {
    pub(super) path: PathBuf,

    pub(super) origin: MorphSource,
    pub(super) template_geometry: Option<Arc<DazGeometry>>,
    pub(super) provider: Option<Arc<VaMGeometryProvider>>,
    pub(super) figure_sex: FigureSex,
    pub(super) allow_vertex_only_pose: bool,
}

pub(super) fn morph_index_asset(
    morph: &crate::vam_morph_index::IndexedMorph,
) -> Option<ResolvedMorphPairAsset> {
    const MAX_MORPH_ASSET_BYTES: usize = 20 * 1024 * 1024;
    match &morph.location {
        MorphLocation::Loose(path) => Some(ResolvedMorphPairAsset::Loose(path.clone())),
        MorphLocation::Packed { archive, vmi_entry } => {
            let vmb_entry = std::path::Path::new(vmi_entry)
                .with_extension("vmb")
                .to_string_lossy()
                .replace('\\', "/");
            let vmi_bytes =
                vkit_core::vam::read_var_entry_bytes(archive, vmi_entry, MAX_MORPH_ASSET_BYTES)
                    .ok()?;
            let vmb_bytes =
                vkit_core::vam::read_var_entry_bytes(archive, &vmb_entry, MAX_MORPH_ASSET_BYTES)
                    .ok()?;
            Some(ResolvedMorphPairAsset::Packed(PackedMorphPairAsset {
                route_path: std::path::PathBuf::from(vmi_entry),
                stable_id: morph.uid.clone(),
                vmi_bytes,
                vmb_bytes,
            }))
        }
    }
}

#[derive(Debug)]
pub struct DirectEditLoadJob {
    pub(super) source: VaMEditSource,
    pub(super) vam_root: Option<PathBuf>,
    pub(super) template_geometry: Option<Arc<DazGeometry>>,
    pub(super) provider: Option<Arc<VaMGeometryProvider>>,
    pub(super) figure_sex: FigureSex,
    pub(super) cached_morphs: Vec<CachedBuiltinMorph>,
    pub(super) morph_index: Arc<VaMMorphIndex>,
}

#[derive(Debug)]
pub struct PreparedDirectEdit {
    pub(super) output: OrderedObjMesh,
    pub(super) overwrite_source: Option<PathBuf>,
    pub(super) missing_morphs: Vec<String>,
}

#[derive(Debug)]
pub enum WorkspaceLoadOutcome {
    Template(Result<Box<PreparedTemplateLoad>, String>),
    Result(Result<Box<OrderedObjMesh>, String>),
    VaMMorphPair(Result<Box<(MorphControl, String)>, String>),
    DirectEditSource(Result<Box<PreparedDirectEdit>, String>),
}

impl WorkspaceLoadOutcome {
    pub fn failure(kind: WorkspaceLoadKind, detail: String) -> Self {
        match kind {
            WorkspaceLoadKind::Template => Self::Template(Err(detail)),
            WorkspaceLoadKind::Result => Self::Result(Err(detail)),
            WorkspaceLoadKind::VaMMorphPair => Self::VaMMorphPair(Err(detail)),
            WorkspaceLoadKind::DirectEditSource => Self::DirectEditSource(Err(detail)),
        }
    }
}

pub(super) fn selected_vam_pair_paths(selected: &Path) -> Result<(PathBuf, PathBuf), String> {
    let (vmi, vmb) =
        crate::persistence::vam_morph_pair_sibling_paths(selected).ok_or_else(|| {
            "Choose either .vmi or .vmb; Vkit requires the exact same-stem sibling".to_owned()
        })?;
    if !vmi.is_file() || !vmb.is_file() {
        return Err(format!(
            "The pair is incomplete. Put both same-stem siblings beside each other: {} and {}",
            vmi.display(),
            vmb.display()
        ));
    }
    Ok((vmi, vmb))
}
