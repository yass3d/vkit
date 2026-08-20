use super::*;

pub fn write_export_snapshot(snapshot: &ExportSnapshot) -> Result<ExportReceipt, WorkflowError> {
    validate_output_format(&snapshot.output_path)?;
    let mut receipt = write_vam_morph_pair(snapshot)?;
    if let Some(textures) = snapshot.baked_textures.as_ref() {
        receipt
            .committed_paths
            .extend(write_baked_texture_export(textures)?);
    }
    Ok(receipt)
}

#[must_use]
pub fn texture_bundle_paths(snapshot: &TextureExportSnapshot) -> Vec<PathBuf> {
    texture_export_images(&snapshot.images, snapshot.pbr_convention)
        .iter()
        .map(|(channel, image)| {
            snapshot.directory.join(texture_export_filename(
                &snapshot.prefix,
                *channel,
                is_opaque(&image.rgba8),
            ))
        })
        .collect()
}

pub fn write_texture_bundle(
    snapshot: &TextureExportSnapshot,
) -> Result<Vec<PathBuf>, WorkflowError> {
    write_baked_texture_export(snapshot)
}

pub(super) fn write_baked_texture_export(
    snapshot: &TextureExportSnapshot,
) -> Result<Vec<PathBuf>, WorkflowError> {
    if snapshot.images.is_empty() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(&snapshot.directory).map_err(|error| {
        WorkflowError::TextureExport(format!(
            "could not create {}: {error}",
            snapshot.directory.display()
        ))
    })?;
    let workspace = tempfile::Builder::new()
        .prefix(".vkit-texture-export-")
        .tempdir_in(&snapshot.directory)
        .map_err(|error| {
            WorkflowError::TextureExport(format!(
                "could not create a staging folder in {}: {error}",
                snapshot.directory.display()
            ))
        })?;
    let export_images = texture_export_images(&snapshot.images, snapshot.pbr_convention);
    let mut staged = Vec::with_capacity(export_images.len());
    for (channel, image) in export_images {
        let expected_len = usize::try_from(image.width)
            .ok()
            .and_then(|width| {
                usize::try_from(image.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                WorkflowError::TextureExport(format!(
                    "{} channel dimensions are too large",
                    channel.label()
                ))
            })?;
        if image.rgba8.len() != expected_len {
            return Err(WorkflowError::TextureExport(format!(
                "{} channel contains {} bytes, expected {expected_len}",
                channel.label(),
                image.rgba8.len()
            )));
        }
        let opaque = is_opaque(&image.rgba8);
        let filename = texture_export_filename(&snapshot.prefix, channel, opaque);

        let staged_path = write_texture_channel(
            &workspace.path().join(&filename),
            channel.export_container_for(opaque),
            &image.rgba8,
            image.width,
            image.height,
        )
        .map_err(|error| {
            WorkflowError::TextureExport(format!("could not encode {filename}: {error}"))
        })?;
        let filename = staged_path
            .file_name()
            .map(|name| PathBuf::from(name.to_owned()))
            .unwrap_or_else(|| PathBuf::from(&filename));
        if !staged_path
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        {
            return Err(WorkflowError::TextureExport(format!(
                "encoded texture is empty: {}",
                staged_path.display()
            )));
        }
        staged.push((staged_path, snapshot.directory.join(filename)));
    }

    let mut committed = Vec::with_capacity(staged.len());
    for (staged_path, destination) in staged {
        promote_completed_file(&staged_path, &destination).map_err(|error| {
            WorkflowError::TextureExport(format!(
                "could not commit {}: {error}",
                destination.display()
            ))
        })?;
        committed.push(destination);
    }
    Ok(committed)
}

pub(super) fn promote_completed_file(staged: &Path, destination: &Path) -> std::io::Result<()> {
    match fs::rename(staged, destination) {
        Ok(()) => return Ok(()),
        Err(initial_error) if !destination.is_file() => return Err(initial_error),
        Err(_) => {}
    }
    let backup_name = destination
        .file_name()
        .map(|name| {
            let mut value = std::ffi::OsString::from("previous-");
            value.push(name);
            value
        })
        .unwrap_or_else(|| std::ffi::OsString::from("previous-export"));
    let backup = staged.with_file_name(backup_name);
    fs::rename(destination, &backup)?;
    match fs::rename(staged, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(commit_error) => {
            let _ = fs::rename(&backup, destination);
            Err(commit_error)
        }
    }
}

pub(super) fn write_texture_channel(
    path: &Path,
    container: crate::texture_project::TextureContainer,
    rgba8: &[u8],
    width: u32,
    height: u32,
) -> Result<PathBuf, image::ImageError> {
    use crate::texture_project::TextureContainer;
    use image::ImageEncoder as _;

    match container {
        TextureContainer::Jpeg if crate::texture_project::is_opaque(rgba8) => {
            let rgb: Vec<u8> = rgba8
                .chunks_exact(4)
                .flat_map(|pixel| &pixel[..3])
                .copied()
                .collect();
            let file = fs::File::create(path).map_err(image::ImageError::IoError)?;
            image::codecs::jpeg::JpegEncoder::new_with_quality(
                std::io::BufWriter::new(file),
                TextureContainer::JPEG_QUALITY,
            )
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)?;
            Ok(path.to_path_buf())
        }
        TextureContainer::Png | TextureContainer::Jpeg => {
            let png = path.with_extension("png");
            write_narrow_png(&png, rgba8, width, height)?;
            Ok(png)
        }
    }
}

pub(super) fn write_narrow_png(
    path: &Path,
    rgba8: &[u8],
    width: u32,
    height: u32,
) -> Result<(), image::ImageError> {
    let (color, pixels) = narrowest_png_encoding(rgba8);
    let file = fs::File::create(path).map_err(image::ImageError::IoError)?;

    use image::ImageEncoder as _;
    image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(&pixels, width, height, color.into())
}

pub(super) fn narrowest_png_encoding(rgba8: &[u8]) -> (image::ColorType, Cow<'_, [u8]>) {
    let opaque = rgba8.chunks_exact(4).all(|pixel| pixel[3] == 255);
    let grey = rgba8
        .chunks_exact(4)
        .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]);
    match (opaque, grey) {
        (true, true) => (
            image::ColorType::L8,
            Cow::Owned(rgba8.iter().step_by(4).copied().collect()),
        ),
        (true, false) => (
            image::ColorType::Rgb8,
            Cow::Owned(
                rgba8
                    .chunks_exact(4)
                    .flat_map(|pixel| &pixel[..3])
                    .copied()
                    .collect(),
            ),
        ),
        (false, true) => (
            image::ColorType::La8,
            Cow::Owned(
                rgba8
                    .chunks_exact(4)
                    .flat_map(|pixel| [pixel[0], pixel[3]])
                    .collect(),
            ),
        ),
        (false, false) => (image::ColorType::Rgba8, Cow::Borrowed(rgba8)),
    }
}

pub(super) fn compose_vam_full_mesh(
    snapshot: &ExportSnapshot,
) -> Result<OrderedObjMesh, WorkflowError> {
    let provider = snapshot
        .provider
        .as_deref()
        .ok_or(WorkflowError::UnsupportedVaMTopology)?;
    let expected_sex = match snapshot.figure_sex {
        FigureSex::Female => vkit_core::vam::GeometrySex::Female,
        FigureSex::Male => vkit_core::vam::GeometrySex::Male,
    };
    if provider.sex() != expected_sex {
        return Err(WorkflowError::VaMProviderSexMismatch {
            provider_sex: provider.sex(),
            figure_sex: snapshot.figure_sex,
        });
    }
    snapshot.output.validate()?;
    let anchor_obj = provider.daz_anchor().to_ordered_obj(None)?;
    if snapshot.output.faces != anchor_obj.faces
        || snapshot.output.vertices.len() != anchor_obj.vertices.len()
    {
        return Err(VaMGeometryError::TargetTopologyMismatch.into());
    }
    let mut target = provider.daz_anchor().clone();
    target.vertices.clone_from(&snapshot.output.vertices);
    target.validate()?;
    let mut full = provider.compose_vam_output(&target)?;
    apply_genital_applications_to_vam_mesh(provider, &mut full, &snapshot.genital_applications)?;
    Ok(full)
}

pub(super) const CANONICAL_G2_BODY_VERTEX_COUNT: usize = 21_556;
const VAM_PAIR_ZERO_TOLERANCE_CM: f64 = 1.0e-12;

pub(super) fn write_vam_morph_pair(
    snapshot: &ExportSnapshot,
) -> Result<ExportReceipt, WorkflowError> {
    let (vmi_path, vmb_path) = vam_morph_pair_paths(&snapshot.output_path)?;
    let route = classify_vam_morph_route(&vmi_path)
        .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?
        .ok_or_else(|| {
            WorkflowError::VaMMorphPair(
                "destination must be under the exact Custom/Atom/Person/Morphs/{female|male|female_genitalia|male_genitalia} route; VMI has no trustworthy routing field"
                    .to_owned(),
            )
        })?;
    let expected_sex = match snapshot.figure_sex {
        FigureSex::Female => vkit_core::vam::GeometrySex::Female,
        FigureSex::Male => vkit_core::vam::GeometrySex::Male,
    };
    if route.sex != expected_sex {
        return Err(WorkflowError::VaMMorphPair(format!(
            "destination route is {:?}, but the current figure is {:?}",
            route.sex, expected_sex
        )));
    }
    let (deltas, vmb_bytes, empty_detail) = match route.routing {
        VmbVertexRouting::Full => {
            let deltas = sparse_result_delta(&snapshot.template_basis, &snapshot.output)
                .map_err(WorkflowError::VaMMorphPair)?;
            if deltas.is_empty() {
                return Err(WorkflowError::VaMMorphPair(
                    "the Save result is identical to the loaded G2 template; make a fit, sculpt, or morph change before exporting"
                        .to_owned(),
                ));
            }
            let encoded = encode_vmb_daz_cm(&deltas, CANONICAL_G2_BODY_VERTEX_COUNT)
                .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?;
            (
                deltas,
                encoded,
                "the Save result is identical to the loaded G2 template; make a fit, sculpt, or morph change before exporting",
            )
        }
        VmbVertexRouting::Genitalia => {
            if snapshot.topology != OutputTopology::VaM {
                return Err(WorkflowError::VaMMorphPair(
                    "genital-only pair export requires VaM output topology".to_owned(),
                ));
            }
            let provider = snapshot
                .provider
                .as_deref()
                .ok_or(WorkflowError::UnsupportedVaMTopology)?;
            let full = compose_vam_full_mesh(snapshot)?;
            let deltas = sparse_genital_result_delta(provider, &full)?;
            if deltas.is_empty() {
                return Err(WorkflowError::VaMMorphPair(
                    "the active provider-bound genital morph state is identical to the enrolled VaM basis"
                        .to_owned(),
                ));
            }
            let encoded = encode_vmb_daz_cm_for_topology(
                &deltas,
                provider.vam_basis().vertices.len(),
                Some(VmbVertexRouting::Genitalia),
            )
            .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?;
            (
                deltas,
                encoded,
                "the active provider-bound genital morph state is identical to the enrolled VaM basis",
            )
        }
    };
    debug_assert!(!deltas.is_empty(), "empty {empty_detail}");
    let raw_entry_count = vmb_raw_entry_count(&vmb_bytes)
        .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?;
    if raw_entry_count != deltas.len() {
        return Err(WorkflowError::VaMMorphPair(format!(
            "generated VMB declares {raw_entry_count} rows for {} sparse deltas",
            deltas.len()
        )));
    }
    let mut metadata = build_shape_vmi_with_options(&snapshot.vam_metadata, raw_entry_count);
    metadata.formulas = Some(
        if route.routing == VmbVertexRouting::Full && snapshot.vam_bone_correction {
            snapshot.vam_bone_formulas.clone()
        } else {
            Vec::new()
        },
    );
    let vmi_bytes = encode_vmi_pretty(&metadata)
        .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?;

    create_export_parent_directory(&vmi_path).map_err(WorkflowError::VaMMorphPair)?;
    write_vam_morph_pair_atomic(&vmi_path, &vmi_bytes, &vmb_path, &vmb_bytes)?;
    Ok(ExportReceipt {
        committed_paths: vec![vmi_path, vmb_path],
    })
}

pub(super) fn create_export_parent_directory(path: &Path) -> Result<(), String> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create the destination folder {}: {error}",
            parent.display()
        )
    })
}

pub(super) fn sparse_genital_result_delta(
    provider: &VaMGeometryProvider,
    result: &OrderedObjMesh,
) -> Result<Vec<SparseDelta>, WorkflowError> {
    sparse_appended_result_delta(
        provider.receipt().shared_vertex_count,
        provider.receipt().metres_to_centimetres,
        provider.vam_basis(),
        result,
    )
}

pub(super) fn sparse_appended_result_delta(
    shared: usize,
    scale: f64,
    basis: &OrderedObjMesh,
    result: &OrderedObjMesh,
) -> Result<Vec<SparseDelta>, WorkflowError> {
    result
        .validate()
        .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?;
    basis
        .validate()
        .map_err(|error| WorkflowError::VaMMorphPair(error.to_string()))?;
    if result.vertices.len() != basis.vertices.len() || result.faces != basis.faces {
        return Err(WorkflowError::VaMMorphPair(
            "composed genital result does not match the validated VaM provider topology".to_owned(),
        ));
    }
    if shared != CANONICAL_G2_BODY_VERTEX_COUNT || shared >= result.vertices.len() {
        return Err(WorkflowError::VaMMorphPair(
            "validated VaM provider has an incompatible appended-graft boundary".to_owned(),
        ));
    }
    if !scale.is_finite() || scale <= 0.0 {
        return Err(WorkflowError::VaMMorphPair(
            "validated VaM provider has an invalid unit scale".to_owned(),
        ));
    }
    let mut deltas = Vec::new();
    for vertex_index in shared..result.vertices.len() {
        let mut delta_cm = [0.0; 3];
        for (axis, delta) in delta_cm.iter_mut().enumerate() {
            *delta =
                (result.vertices[vertex_index][axis] - basis.vertices[vertex_index][axis]) * scale;
        }
        if delta_cm
            .iter()
            .any(|value| value.abs() > VAM_PAIR_ZERO_TOLERANCE_CM)
        {
            deltas.push(SparseDelta {
                vertex_index: u32::try_from(vertex_index).map_err(|_| {
                    WorkflowError::VaMMorphPair(
                        "VaM genital vertex index exceeds the VMB u32 range".to_owned(),
                    )
                })?,
                delta_cm,
            });
        }
    }
    Ok(deltas)
}

pub(super) fn sparse_result_delta(
    basis: &OrderedObjMesh,
    result: &OrderedObjMesh,
) -> Result<Vec<SparseDelta>, String> {
    basis.validate().map_err(|error| error.to_string())?;
    result.validate().map_err(|error| error.to_string())?;
    if basis.vertices.len() != CANONICAL_G2_BODY_VERTEX_COUNT
        || result.vertices.len() != CANONICAL_G2_BODY_VERTEX_COUNT
    {
        return Err(format!(
            "body morphs require canonical {CANONICAL_G2_BODY_VERTEX_COUNT}-vertex G2 geometry (basis={}, result={})",
            basis.vertices.len(),
            result.vertices.len()
        ));
    }
    if basis.faces != result.faces {
        return Err(
            "the Save result no longer has the loaded template's polygon topology".to_owned(),
        );
    }
    build_sparse_deltas_daz_cm(
        &basis.vertices,
        &result.vertices,
        VAM_PAIR_ZERO_TOLERANCE_CM,
    )
    .map_err(|error| error.to_string())
}

pub(super) fn vam_morph_pair_paths(path: &Path) -> Result<(PathBuf, PathBuf), WorkflowError> {
    match crate::persistence::vam_morph_pair_sibling_paths(path) {
        Some((vmi, vmb)) if vmi.as_path() == path => Ok((vmi, vmb)),
        _ => Err(WorkflowError::VaMMorphPair(
            "select a .vmi destination; the .vmb sibling is created automatically".to_owned(),
        )),
    }
}

pub(super) fn validate_output_format(path: &Path) -> Result<(), WorkflowError> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(MORPH_EXPORT_EXTENSION))
    {
        return Ok(());
    }
    Err(WorkflowError::OutputFormatMismatch {
        format: MORPH_EXPORT_LABEL,
        expected_extension: MORPH_EXPORT_EXTENSION,
    })
}
