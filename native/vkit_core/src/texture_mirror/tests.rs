use super::*;

use crate::vam::{G2UvTriangle, UvMaterialRegion};

fn asymmetric_atlas() -> (Vec<[f64; 3]>, G2UvMapping) {
    let vertices = vec![
        [-2.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [-2.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [2.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [2.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    let triangle = |positions: [u32; 3], uvs: [[f32; 2]; 3]| G2UvTriangle {
        canonical_face_index: 0,
        canonical_triangle_index: 0,
        material_region: UvMaterialRegion::Face,
        on_head: true,
        position_indices: positions,
        uvs,
    };

    let mapping = G2UvMapping {
        source_path: std::path::PathBuf::from("fixture"),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        faces: Vec::new(),
        triangles: vec![
            triangle([0, 1, 2], [[0.0, 1.0], [1.0, 1.0], [0.0, 0.6]]),
            triangle([1, 3, 2], [[1.0, 1.0], [1.0, 0.6], [0.0, 0.6]]),
            triangle([4, 5, 6], [[0.0, 0.4], [1.0, 0.4], [0.0, 0.0]]),
            triangle([5, 7, 6], [[1.0, 0.4], [1.0, 0.0], [0.0, 0.0]]),
        ],
        uncovered_triangles: 0,
    };
    (vertices, mapping)
}

fn filled(width: u32, height: u32, top: [u8; 4], bottom: [u8; 4]) -> TextureBakeImage {
    let mut image = TextureBakeImage::transparent(width, height).unwrap();
    for y in 0..height {
        for x in 0..width {
            let index = (y as usize * width as usize + x as usize) * 4;

            let colour = if y < height / 2 { top } else { bottom };
            image.rgba8[index..index + 4].copy_from_slice(&colour);
        }
    }
    image
}

fn pixel(image: &TextureBakeImage, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width as usize + x as usize) * 4;
    [
        image.rgba8[index],
        image.rgba8[index + 1],
        image.rgba8[index + 2],
        image.rgba8[index + 3],
    ]
}

#[test]
fn triangles_pair_through_the_mesh_rather_than_through_the_atlas() {
    let (vertices, mapping) = asymmetric_atlas();
    let map = FaceMirrorMap::build(&mapping, &vertices).unwrap();
    assert_eq!(map.paired(), 4);
    assert!(map.is_usable());
}

#[test]
fn one_half_is_repainted_from_the_other() {
    let (vertices, mapping) = asymmetric_atlas();
    let map = FaceMirrorMap::build(&mapping, &vertices).unwrap();
    const LEFT: [u8; 4] = [200, 40, 40, 255];
    const RIGHT: [u8; 4] = [40, 80, 200, 255];

    let mut image = filled(32, 32, LEFT, RIGHT);
    let filled_count = map.apply(&mut image, &mapping, FaceMirror::ToNegativeX);
    assert_eq!(filled_count, 2, "both left triangles should be redrawn");

    assert_eq!(pixel(&image, 16, 4), RIGHT);

    assert_eq!(pixel(&image, 16, 28), RIGHT);
}

#[test]
fn the_direction_decides_which_half_survives() {
    let (vertices, mapping) = asymmetric_atlas();
    let map = FaceMirrorMap::build(&mapping, &vertices).unwrap();
    const LEFT: [u8; 4] = [200, 40, 40, 255];
    const RIGHT: [u8; 4] = [40, 80, 200, 255];

    let mut image = filled(32, 32, LEFT, RIGHT);
    map.apply(&mut image, &mapping, FaceMirror::ToPositiveX);
    assert_eq!(pixel(&image, 16, 4), LEFT);
    assert_eq!(pixel(&image, 16, 28), LEFT);
}

#[test]
fn off_leaves_the_bake_untouched() {
    let (vertices, mapping) = asymmetric_atlas();
    let map = FaceMirrorMap::build(&mapping, &vertices).unwrap();
    let mut image = filled(32, 32, [200, 40, 40, 255], [40, 80, 200, 255]);
    let before = image.rgba8.clone();
    assert_eq!(map.apply(&mut image, &mapping, FaceMirror::Off), 0);
    assert_eq!(image.rgba8, before);
}

#[test]
fn an_asymmetric_head_is_refused() {
    let (_, mapping) = asymmetric_atlas();
    let lopsided = vec![
        [-2.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [-2.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [2.3, 1.4, 0.0],
        [1.1, 1.7, 0.0],
        [2.9, 0.2, 0.0],
        [1.6, 0.3, 0.0],
    ];
    assert_eq!(
        FaceMirrorMap::build(&mapping, &lopsided).unwrap_err(),
        FaceMirrorError::AsymmetricHead
    );
}
