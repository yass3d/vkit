use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const CACHE_GENERATION: u32 = 1;

const STAMP_FILE: &str = "cache.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheOutcome {
    Fresh,

    Kept,

    Reset,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CacheStamp {
    generation: u32,
    #[serde(default)]
    written_by: String,
}

fn stamp_path(root: &Path) -> PathBuf {
    root.join(STAMP_FILE)
}

fn read_generation(root: &Path) -> Option<u32> {
    let text = fs::read_to_string(stamp_path(root)).ok()?;
    serde_json::from_str::<CacheStamp>(&text)
        .ok()
        .map(|stamp| stamp.generation)
}

fn write_stamp(root: &Path, app_version: &str) -> io::Result<()> {
    let stamp = CacheStamp {
        generation: CACHE_GENERATION,
        written_by: app_version.to_owned(),
    };
    let text = serde_json::to_string_pretty(&stamp)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(stamp_path(root), text)
}

fn empty_the_cache(root: &Path) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let removed = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if let Err(error) = removed
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(error);
        }
    }
    Ok(())
}

pub fn ensure_cache_generation(root: &Path, app_version: &str) -> io::Result<CacheOutcome> {
    if !root.exists() {
        fs::create_dir_all(root)?;
        write_stamp(root, app_version)?;
        return Ok(CacheOutcome::Fresh);
    }
    let found = read_generation(root);
    if found == Some(CACHE_GENERATION) {
        return Ok(CacheOutcome::Kept);
    }
    let empty = fs::read_dir(root)?.next().is_none();
    empty_the_cache(root)?;
    write_stamp(root, app_version)?;
    Ok(if empty {
        CacheOutcome::Fresh
    } else {
        CacheOutcome::Reset
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    #[test]
    fn a_cache_root_that_is_not_there_yet_is_made_and_stamped() {
        let dir = scratch();
        let root = dir.path().join("cache");
        assert_eq!(
            ensure_cache_generation(&root, "0.0.5").unwrap(),
            CacheOutcome::Fresh
        );
        assert_eq!(read_generation(&root), Some(CACHE_GENERATION));
    }

    #[test]
    fn a_cache_from_this_generation_is_left_exactly_alone() {
        let dir = scratch();
        let root = dir.path().join("cache");
        ensure_cache_generation(&root, "0.0.5").unwrap();
        let kept = root.join("morphs");
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("expensive.fmmcache"), b"hours of decoding").unwrap();

        assert_eq!(
            ensure_cache_generation(&root, "0.0.6").unwrap(),
            CacheOutcome::Kept,
            "a later release with the same shapes must not throw the cache away",
        );
        assert!(kept.join("expensive.fmmcache").exists());
    }

    #[test]
    fn a_cache_from_before_the_stamp_existed_is_emptied() {
        let dir = scratch();
        let root = dir.path().join("cache");
        let old = root.join("builtin-skins").join("f_1");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("stale.fmskintex"), b"written by an older build").unwrap();
        fs::write(root.join("loose.fmskintex"), b"also stale").unwrap();

        assert_eq!(
            ensure_cache_generation(&root, "0.0.5").unwrap(),
            CacheOutcome::Reset,
            "no stamp means it predates the mechanism, which is the case this exists for",
        );
        assert!(!old.exists());
        assert!(!root.join("loose.fmskintex").exists());
        assert_eq!(read_generation(&root), Some(CACHE_GENERATION));
    }

    #[test]
    fn a_cache_from_another_generation_is_emptied() {
        let dir = scratch();
        let root = dir.path().join("cache");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            stamp_path(&root),
            serde_json::to_string(&CacheStamp {
                generation: CACHE_GENERATION + 1,
                written_by: "a build from the future".to_owned(),
            })
            .unwrap(),
        )
        .unwrap();
        fs::create_dir_all(root.join("packages")).unwrap();

        assert_eq!(
            ensure_cache_generation(&root, "0.0.5").unwrap(),
            CacheOutcome::Reset,
            "a generation we do not recognise is as unusable as an older one",
        );
        assert!(!root.join("packages").exists());
    }

    #[test]
    fn an_unreadable_stamp_counts_as_no_stamp() {
        let dir = scratch();
        let root = dir.path().join("cache");
        fs::create_dir_all(&root).unwrap();
        fs::write(stamp_path(&root), b"{ this is not json").unwrap();
        fs::create_dir_all(root.join("morphs")).unwrap();

        assert_eq!(
            ensure_cache_generation(&root, "0.0.5").unwrap(),
            CacheOutcome::Reset
        );
        assert_eq!(read_generation(&root), Some(CACHE_GENERATION));
    }

    #[test]
    fn an_empty_root_without_a_stamp_is_fresh_rather_than_reset() {
        let dir = scratch();
        let root = dir.path().join("cache");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(
            ensure_cache_generation(&root, "0.0.5").unwrap(),
            CacheOutcome::Fresh,
            "there was nothing to lose, so nothing was lost",
        );
    }
}
