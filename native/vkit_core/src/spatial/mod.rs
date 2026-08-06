pub use vkit_geometry_core::{
    GeometryKernelError, SurfaceProjection, SurfaceProjector, SurfaceProjectorError,
    deformation_safety_mask, triangle_intersection_count,
};

use crate::formats::Mesh;

pub fn projector_for_mesh(mesh: &Mesh) -> Result<SurfaceProjector, SurfaceProjectorError> {
    SurfaceProjector::new(&mesh.vertices, &mesh.triangles, None, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_projector_keeps_original_primitive_identity() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 2.0],
                [1.0, 0.0, 2.0],
                [0.0, 1.0, 2.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap();
        let projector = projector_for_mesh(&mesh).unwrap();
        let hit = projector.project([0.25, 0.25, 1.8]).unwrap();
        assert_eq!(hit.primitive_id, 1);
        assert_eq!(hit.point, [0.25, 0.25, 2.0]);
    }
}
