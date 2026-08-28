use vkit_core::vam::hair_joints::{DEFAULT_JOINT_SEARCH_CM, build_style_joints};

fn head(strands: usize, points: usize) -> Vec<Vec<[f32; 3]>> {
    let side = (strands as f32).sqrt().ceil() as usize;
    let pitch = 0.5;
    let _ = side;
    (0..strands)
        .map(|strand| {
            let x = (strand % side) as f32 * pitch;
            let z = (strand / side) as f32 * pitch;
            (0..points)
                .map(|step| [x + step as f32 * 0.01, -(step as f32) * 0.4, z])
                .collect()
        })
        .collect()
}

#[test]
fn the_cost_follows_the_number_of_points_and_not_its_square() {
    let time = |strands: usize, points: usize| {
        let curves = head(strands, points);
        let borrowed: Vec<&[[f32; 3]]> = curves.iter().map(Vec::as_slice).collect();
        let started = std::time::Instant::now();
        let groups = build_style_joints(&borrowed, DEFAULT_JOINT_SEARCH_CM);
        let elapsed = started.elapsed().as_secs_f64();
        (elapsed, groups.iter().flatten().count())
    };

    let (small, small_joints) = time(100, 25);
    let (large, large_joints) = time(400, 50);
    assert!(small_joints > 0 && large_joints > 0, "no joints to time");

    let growth = large / small.max(1.0e-6);
    assert!(
        growth < 16.0,
        "8x the hair cost {growth:.1}x the time ({small:.4}s -> {large:.4}s)",
    );
}
