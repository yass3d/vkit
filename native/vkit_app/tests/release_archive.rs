use std::path::{Path, PathBuf};
use std::process::Command;

const COMPILED_IN_FROM_ABOVE_THE_CRATE: [&str; 2] =
    ["design/logo.png", "build/windows/THIRD-PARTY-NOTICE.txt"];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is nested two levels under the repository root")
}

fn git(root: &Path, arguments: &[&str], path: &str) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .arg(path)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn is_in_the_export_archive(root: &Path, relative: &str) -> Option<bool> {
    let archive = git(
        root,
        &[
            "archive",
            "--worktree-attributes",
            "--format=tar",
            "HEAD",
            "--",
        ],
        relative,
    )?;
    Some(
        archive
            .windows(relative.len())
            .any(|window| window == relative.as_bytes()),
    )
}

#[test]
fn nothing_the_binary_is_compiled_from_is_kept_out_of_the_release_archive() {
    let root = repository_root();
    if !root.join(".git").exists() {
        eprintln!("skipping: {} is not a git checkout", root.display());
        return;
    }

    for relative in COMPILED_IN_FROM_ABOVE_THE_CRATE {
        assert!(
            root.join(relative).is_file(),
            "{relative} is compiled in but is not in the tree; this list moves with it"
        );
        let Some(in_head) = git(&root, &["ls-tree", "--name-only", "HEAD", "--"], relative) else {
            eprintln!("skipping: git cannot answer for {}", root.display());
            return;
        };
        if in_head.is_empty() {
            eprintln!("skipping {relative}: not committed yet, so HEAD has nothing to say");
            continue;
        }
        let Some(exported) = is_in_the_export_archive(&root, relative) else {
            eprintln!("skipping: git cannot answer for {}", root.display());
            return;
        };
        assert!(
            exported,
            "{relative} is export-ignored, so `git archive` -- and with it every downloaded \
             release source zip -- omits a file the build reads"
        );
    }
}
