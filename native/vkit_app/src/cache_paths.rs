use sha2::{Digest, Sha256};
use std::path::Path;

pub fn cache_key_for_root(root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root.to_string_lossy().to_lowercase().as_bytes());
    hex_prefix(&hasher.finalize(), 8)
}

pub fn hex_prefix(bytes: &[u8], count: usize) -> String {
    bytes
        .iter()
        .take(count)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
