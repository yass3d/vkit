use std::path::Path;

use thiserror::Error;

use super::{geometry::GeometrySex, vmb::VmbVertexRouting};

const ROUTE_PREFIX: [&str; 4] = ["Custom", "Atom", "Person", "Morphs"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VaMMorphRoute {
    pub sex: GeometrySex,
    pub routing: VmbVertexRouting,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum VaMMorphRouteError {
    #[error(
        "path contains {match_count} canonical VaM morph route sequences; expected at most one"
    )]
    Ambiguous { match_count: usize },
}

pub fn classify_vam_morph_route(
    path: impl AsRef<Path>,
) -> Result<Option<VaMMorphRoute>, VaMMorphRouteError> {
    let path = path.as_ref().as_os_str().to_string_lossy();
    let components: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect();

    let mut route = None;
    let mut match_count = 0;
    for window in components.windows(ROUTE_PREFIX.len() + 1) {
        if !window[..ROUTE_PREFIX.len()]
            .iter()
            .zip(ROUTE_PREFIX)
            .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
        {
            continue;
        }
        let Some(candidate) = route_from_leaf(window[ROUTE_PREFIX.len()]) else {
            continue;
        };
        match_count += 1;
        route = Some(candidate);
    }

    if match_count > 1 {
        return Err(VaMMorphRouteError::Ambiguous { match_count });
    }
    Ok(route)
}

fn route_from_leaf(leaf: &str) -> Option<VaMMorphRoute> {
    if leaf.eq_ignore_ascii_case("female") {
        Some(VaMMorphRoute {
            sex: GeometrySex::Female,
            routing: VmbVertexRouting::Full,
        })
    } else if leaf.eq_ignore_ascii_case("male") {
        Some(VaMMorphRoute {
            sex: GeometrySex::Male,
            routing: VmbVertexRouting::Full,
        })
    } else if leaf.eq_ignore_ascii_case("female_genitalia") {
        Some(VaMMorphRoute {
            sex: GeometrySex::Female,
            routing: VmbVertexRouting::Genitalia,
        })
    } else if leaf.eq_ignore_ascii_case("male_genitalia") {
        Some(VaMMorphRoute {
            sex: GeometrySex::Male,
            routing: VmbVertexRouting::Genitalia,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(path: &str) -> Option<VaMMorphRoute> {
        classify_vam_morph_route(path).unwrap()
    }

    #[test]
    fn classifies_all_four_canonical_route_leaves() {
        for (leaf, sex, routing) in [
            ("female", GeometrySex::Female, VmbVertexRouting::Full),
            ("male", GeometrySex::Male, VmbVertexRouting::Full),
            (
                "female_genitalia",
                GeometrySex::Female,
                VmbVertexRouting::Genitalia,
            ),
            (
                "male_genitalia",
                GeometrySex::Male,
                VmbVertexRouting::Genitalia,
            ),
        ] {
            assert_eq!(
                route(&format!(
                    r"D:\Games\VaM\Custom\Atom\Person\Morphs\{leaf}\shape.vmi"
                )),
                Some(VaMMorphRoute { sex, routing })
            );
        }
    }

    #[test]
    fn accepts_case_separators_unc_prefix_and_nested_categories() {
        assert_eq!(
            route(
                r"\\server\share\VaM\CUSTOM/atom\PERSON/MoRpHs\FeMaLe_GeNiTaLiA\Creator\Face\shape.VMB",
            ),
            Some(VaMMorphRoute {
                sex: GeometrySex::Female,
                routing: VmbVertexRouting::Genitalia,
            })
        );
        assert_eq!(
            route("/opt/vam/Custom/Atom/Person/Morphs/MALE/Face/Eyes/shape.vmi"),
            Some(VaMMorphRoute {
                sex: GeometrySex::Male,
                routing: VmbVertexRouting::Full,
            })
        );
        assert_eq!(
            route("Custom/Atom/Person/Morphs/female"),
            Some(VaMMorphRoute {
                sex: GeometrySex::Female,
                routing: VmbVertexRouting::Full,
            })
        );
    }

    #[test]
    fn ignores_misleading_ancestor_and_noncanonical_leaf_names() {
        for path in [
            r"C:\female\male\female_genitalia\shape.vmi",
            r"C:\Custom\Atom\Person\NotMorphs\female\shape.vmi",
            r"C:\Custom\Atom\Person\Morphsish\male\shape.vmi",
            r"C:\Custom\Atom\Person\Morphs\femaleish\shape.vmi",
            r"C:\Custom\Atom\Person\Morphs\female.vmi",
            r"C:\Custom\Atom\Person\Morphs\Category\female\shape.vmi",
            r"C:\Elsewhere\Morphs\male_genitalia\shape.vmi",
            "",
        ] {
            assert_eq!(route(path), None, "unexpected route for {path}");
        }

        assert_eq!(
            route(r"C:\female\male_genitalia\VaM\Custom\Atom\Person\Morphs\male\shape.vmi",),
            Some(VaMMorphRoute {
                sex: GeometrySex::Male,
                routing: VmbVertexRouting::Full,
            })
        );
    }

    #[test]
    fn rejects_multiple_canonical_sequences_even_when_they_agree() {
        for (path, expected_count) in [
            (
                r"C:\Custom\Atom\Person\Morphs\female\Category\Custom\Atom\Person\Morphs\male\shape.vmi",
                2,
            ),
            (
                "Custom/Atom/Person/Morphs/female/Custom/Atom/Person/Morphs/female/file.vmb",
                2,
            ),
        ] {
            assert_eq!(
                classify_vam_morph_route(path),
                Err(VaMMorphRouteError::Ambiguous {
                    match_count: expected_count,
                })
            );
        }
    }
}
