use crate::math::Vec3;

pub const RIG_TRANSFER_NEIGHBORS: usize = 128;
pub const RIG_TRANSFER_POWER: f64 = 4.0;
const EXACT_POINT_EPSILON_CM: f64 = 1.0e-7;

pub fn transfer_rig_point_idw(
    point: Vec3,
    base_vertices: &[[f64; 3]],
    result_vertices: &[[f64; 3]],
) -> Result<Vec3, String> {
    if base_vertices.len() != result_vertices.len() {
        return Err(format!(
            "rig transfer vertex counts differ ({} != {})",
            base_vertices.len(),
            result_vertices.len()
        ));
    }
    if base_vertices.is_empty() || !point.is_finite() {
        return Err("rig transfer requires a finite point and non-empty geometry".to_owned());
    }

    let mut nearest = base_vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let base = Vec3::from(*vertex);
            (index, (base - point).norm_squared())
        })
        .filter(|(_, distance_squared)| distance_squared.is_finite())
        .collect::<Vec<_>>();
    let keep = nearest.len().min(RIG_TRANSFER_NEIGHBORS);
    nearest.select_nth_unstable_by(keep.saturating_sub(1), |left, right| {
        left.1.total_cmp(&right.1)
    });
    nearest.truncate(keep);

    let epsilon_squared = EXACT_POINT_EPSILON_CM * EXACT_POINT_EPSILON_CM;
    if let Some(&(index, _)) = nearest
        .iter()
        .find(|(_, distance_squared)| *distance_squared <= epsilon_squared)
    {
        return Ok(point + Vec3::from(result_vertices[index]) - Vec3::from(base_vertices[index]));
    }

    let mut weighted_delta = Vec3::ZERO;
    let mut total_weight = 0.0;
    for (index, distance_squared) in nearest {
        let distance = distance_squared.sqrt().max(EXACT_POINT_EPSILON_CM);
        let weight: f64 = 1.0 / distance.powf(RIG_TRANSFER_POWER);
        let delta = Vec3::from(result_vertices[index]) - Vec3::from(base_vertices[index]);
        if weight.is_finite() && delta.is_finite() {
            weighted_delta += delta * weight;
            total_weight += weight;
        }
    }
    if !total_weight.is_finite() || total_weight <= 0.0 {
        return Err("rig transfer could not find finite neighboring vertices".to_owned());
    }
    Ok(point + weighted_delta / total_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_joint_transfer_preserves_translation() {
        let base = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let result = [[2.0, -3.0, 4.0], [3.0, -3.0, 4.0], [2.0, -2.0, 4.0]];
        let transferred = transfer_rig_point_idw(Vec3::ZERO, &base, &result).unwrap();
        assert_eq!(transferred, Vec3::new(2.0, -3.0, 4.0));
    }

    #[test]
    fn symmetric_scale_does_not_shift_the_center() {
        let base = [
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let result = [
            [-2.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, -2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        let transferred = transfer_rig_point_idw(Vec3::ZERO, &base, &result).unwrap();
        assert!(transferred.norm_squared() < 1.0e-24);
    }
}
