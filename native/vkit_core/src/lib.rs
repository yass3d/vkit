#![forbid(unsafe_code)]
pub mod anatomy;
pub mod cache;
pub mod fit;
pub mod formats;
pub mod math;
pub mod pipeline;
pub mod pixels;
pub mod quality;
pub mod restore_region;
pub mod rig;
pub mod sculpt;
pub mod spatial;
pub mod surface_smoothing;
pub mod symmetry;
pub mod texture_bake;
pub mod texture_mirror;
pub mod texture_transfer;
pub mod vam;

pub const MIN_ALIGNMENT_PAIRS: usize = 8;

pub const MIN_FIT_PAIRS: usize = 12;

pub const APP_DIR_NAME: &str = "Vkit";

pub fn cache_root() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| {
        std::path::PathBuf::from(base)
            .join(APP_DIR_NAME)
            .join("cache")
    })
}

pub const G2F_VERTEX_COUNT: usize = 21_556;

pub const G2F_POLYGON_COUNT: usize = 21_098;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_contract_constants_are_frozen() {
        assert_eq!(MIN_ALIGNMENT_PAIRS, 8);
        assert_eq!(MIN_FIT_PAIRS, 12);
        assert_eq!(G2F_VERTEX_COUNT, 21_556);
        assert_eq!(G2F_POLYGON_COUNT, 21_098);
    }
}
