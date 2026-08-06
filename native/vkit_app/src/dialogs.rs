use std::path::{Path, PathBuf};

use rfd::FileDialog;

use crate::i18n::{TextKey, text};
use crate::state::{AppState, DialogIntent, MORPH_EXPORT_EXTENSION, PackageSlot};
use crate::texture_project::TextureSourceMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogSpec {
    pub title: TextKey,
    pub extensions: &'static [&'static str],
    pub save: bool,
    pub folder: bool,
}

pub const fn spec_for(intent: DialogIntent) -> DialogSpec {
    match intent {
        DialogIntent::OpenScan => DialogSpec {
            title: TextKey::DialogOpenScan,
            extensions: &["obj", "glb", "fbx"],
            save: false,
            folder: false,
        },
        DialogIntent::OpenTextureImage(TextureSourceMode::MaterialUv) => DialogSpec {
            title: TextKey::DialogAddUvLayer,
            extensions: &["png", "jpg", "jpeg"],
            save: false,
            folder: false,
        },
        DialogIntent::OpenTextureImage(_) => DialogSpec {
            title: TextKey::DialogAddTextureLayer,
            extensions: &["png", "jpg", "jpeg"],
            save: false,
            folder: false,
        },
        DialogIntent::ChooseOutput => DialogSpec {
            title: TextKey::DialogSaveMorphPair,
            extensions: &[MORPH_EXPORT_EXTENSION],
            save: true,
            folder: false,
        },
        DialogIntent::ChooseVaMRoot => DialogSpec {
            title: TextKey::DialogChooseVamFolder,
            extensions: &[],
            save: false,
            folder: true,
        },
    }
}

pub fn show(intent: DialogIntent, state: &AppState) -> Option<PathBuf> {
    let spec = spec_for(intent);
    let mut dialog = FileDialog::new().set_title(text(state.locale, spec.title));
    if let Some(first) = spec.extensions.first() {
        dialog = dialog.add_filter(first.to_ascii_uppercase(), spec.extensions);
    }

    match intent {
        DialogIntent::OpenScan => {
            if let Some(parent) = state.scan_path.as_deref().and_then(Path::parent) {
                dialog = dialog.set_directory(parent);
            }
        }

        DialogIntent::OpenTextureImage(TextureSourceMode::MaterialUv) => {
            if let Some(directory) = state.vam_texture_directory() {
                dialog = dialog.set_directory(directory);
            }
        }
        DialogIntent::OpenTextureImage(_) => {
            let source = state
                .texture_project
                .selected_layer()
                .and_then(|layer| layer.source_path.as_deref())
                .or(state.scan_path.as_deref());
            if let Some(parent) = source.and_then(Path::parent) {
                dialog = dialog.set_directory(parent);
            }
        }
        DialogIntent::ChooseOutput => {
            let output = Path::new(&state.output_path);
            if let Some(parent) = output.parent().filter(|path| path.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            let suggested = normalize_export_extension(output.to_path_buf());
            if let Some(name) = suggested.file_name().and_then(|name| name.to_str()) {
                dialog = dialog.set_file_name(name);
            }
        }
        DialogIntent::ChooseVaMRoot => {
            if let Some(root) = state.vam_root.as_deref() {
                dialog = dialog.set_directory(root);
            }
        }
    }

    let selected = if spec.folder {
        dialog.pick_folder()
    } else if spec.save {
        dialog.save_file()
    } else {
        dialog.pick_file()
    }?;
    Some(if spec.save {
        normalize_export_extension(selected)
    } else {
        selected
    })
}

pub(crate) fn pick_package_files(state: &AppState, slot: PackageSlot) -> Option<Vec<PathBuf>> {
    let (title, extensions, start) = match slot {
        PackageSlot::Morph => (
            TextKey::PackageAddMorphs,
            &["vmi", "vmb"][..],
            state.vam_morph_directory(),
        ),
        PackageSlot::Texture => (
            TextKey::PackageAddTextures,
            &["png", "jpg", "jpeg", "tif"][..],
            state.vam_texture_directory(),
        ),
    };
    let mut dialog = FileDialog::new().set_title(text(state.locale, title));
    if let Some(first) = extensions.first() {
        dialog = dialog.add_filter(first.to_ascii_uppercase(), extensions);
    }
    if let Some(directory) = start {
        dialog = dialog.set_directory(directory);
    }
    dialog.pick_files()
}

pub(crate) fn normalize_export_extension(mut path: PathBuf) -> PathBuf {
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(MORPH_EXPORT_EXTENSION))
    {
        path.set_extension(MORPH_EXPORT_EXTENSION);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dialog_intent_has_a_narrow_file_contract() {
        assert_eq!(
            spec_for(DialogIntent::OpenScan).extensions,
            &["obj", "glb", "fbx"],
            "a head can still arrive in any of these; only what Vkit writes is narrowed"
        );
        assert_eq!(spec_for(DialogIntent::ChooseOutput).extensions, &["vmi"]);
        assert!(spec_for(DialogIntent::ChooseOutput).save);
        assert!(!spec_for(DialogIntent::OpenScan).save);
        assert!(spec_for(DialogIntent::ChooseVaMRoot).extensions.is_empty());
        assert!(spec_for(DialogIntent::ChooseVaMRoot).folder);
    }

    #[test]
    fn the_save_dialog_names_the_morph_pair_and_leaves_a_matching_case_alone() {
        assert_eq!(
            normalize_export_extension(PathBuf::from("head")),
            PathBuf::from("head.vmi")
        );
        assert_eq!(
            normalize_export_extension(PathBuf::from("head.VMI")),
            PathBuf::from("head.VMI")
        );
        assert_eq!(
            normalize_export_extension(PathBuf::from("head.vmb")),
            PathBuf::from("head.vmi")
        );
        assert_eq!(
            normalize_export_extension(PathBuf::from("head.anything")),
            PathBuf::from("head.vmi")
        );
    }
}
