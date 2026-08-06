use approx::assert_abs_diff_eq;
use vkit_core::math::{Mat3, SimilarityError, SimilarityTransform, Vec3, estimate_similarity};

fn assert_vec3_close(actual: Vec3, expected: Vec3, epsilon: f64) {
    assert_abs_diff_eq!(actual.x, expected.x, epsilon = epsilon);
    assert_abs_diff_eq!(actual.y, expected.y, epsilon = epsilon);
    assert_abs_diff_eq!(actual.z, expected.z, epsilon = epsilon);
}

#[test]
fn weighted_umeyama_recovers_known_similarity() {
    let source = [
        Vec3::new(-1.0, 0.5, 2.0),
        Vec3::new(2.0, -0.25, 0.0),
        Vec3::new(0.25, 3.0, -1.0),
        Vec3::new(-2.0, -1.5, 0.75),
        Vec3::new(1.0, 1.0, 1.0),
    ];
    let angle = 0.37_f64;
    let expected = SimilarityTransform {
        scale: 2.4,
        rotation: Mat3::from_rows([
            [angle.cos(), 0.0, angle.sin()],
            [0.0, 1.0, 0.0],
            [-angle.sin(), 0.0, angle.cos()],
        ]),
        translation: Vec3::new(3.0, -1.5, 0.7),
    };
    let target = expected.apply_slice(&source);
    let fitted = estimate_similarity(&source, &target, Some(&[1.0, 4.0, 0.5, 3.0, 2.0]))
        .expect("well-conditioned points");

    assert_abs_diff_eq!(fitted.scale, expected.scale, epsilon = 1.0e-12);
    assert_abs_diff_eq!(fitted.rotation.determinant(), 1.0, epsilon = 1.0e-12);
    for (actual, wanted) in fitted.apply_slice(&source).into_iter().zip(target) {
        assert_vec3_close(actual, wanted, 2.0e-12);
    }
}

#[test]
fn zero_weight_excludes_a_large_outlier() {
    let source = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let expected = SimilarityTransform {
        scale: 1.25,
        rotation: Mat3::IDENTITY,
        translation: Vec3::new(2.0, -3.0, 4.0),
    };
    let mut target = expected.apply_slice(&source);
    target[3] = Vec3::new(500.0, -700.0, 900.0);
    let fitted = estimate_similarity(&source, &target, Some(&[1.0, 1.0, 1.0, 0.0]))
        .expect("three non-collinear weighted points");

    for point in source.iter().copied().take(3) {
        assert_vec3_close(fitted.apply(point), expected.apply(point), 1.0e-12);
    }
}

#[test]
fn reflection_is_not_returned_as_a_rotation() {
    let source = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 0.0, 3.0),
    ];
    let target: Vec<_> = source
        .iter()
        .map(|point| Vec3::new(-point.x, point.y, point.z))
        .collect();
    let fitted = estimate_similarity(&source, &target, None).expect("valid point clouds");
    assert!(fitted.rotation.determinant() > 0.999_999_999);
}

#[test]
fn invalid_similarity_inputs_fail_explicitly() {
    let points = [Vec3::ZERO; 3];
    assert_eq!(
        estimate_similarity(&points[..2], &points[..2], None),
        Err(SimilarityError::TooFewPoints(2))
    );
    assert_eq!(
        estimate_similarity(&points, &points, Some(&[1.0, -1.0, 1.0])),
        Err(SimilarityError::InvalidWeight(1))
    );
    assert_eq!(
        estimate_similarity(&points, &points, Some(&[0.0, 0.0, 0.0])),
        Err(SimilarityError::NoPositiveWeight)
    );
    assert_eq!(
        estimate_similarity(&points, &points, None),
        Err(SimilarityError::DegenerateSource)
    );
}

#[test]
fn matrix_transpose_and_vector_arithmetic_are_stable() {
    let matrix = Mat3::from_rows([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 10.0]]);
    let vector = Vec3::new(2.0, -1.0, 0.5);
    assert_vec3_close(
        matrix.transform_vector(vector),
        Vec3::new(1.5, 6.0, 11.0),
        0.0,
    );
    assert_vec3_close(
        matrix.transpose().transform_vector(vector),
        Vec3::new(1.5, 3.0, 5.0),
        0.0,
    );
}
