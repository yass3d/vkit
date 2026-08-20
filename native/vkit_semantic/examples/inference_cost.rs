fn main() {
    const SIDE: usize = vkit_semantic::MODEL_INPUT_WIDTH;
    let frame = vec![0.5_f32; SIDE * SIDE * 3];

    let warmup = std::time::Instant::now();
    let ready = vkit_semantic::detect_landmarks_rgb_f32(&frame, SIDE, SIDE).is_ok();
    println!("first call (model unpack included): {:?}", warmup.elapsed());
    if !ready {
        println!("the model refused this input; nothing to time");
        return;
    }

    let runs = 20;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        let _ = vkit_semantic::detect_landmarks_rgb_f32(&frame, SIDE, SIDE);
    }
    let each = started.elapsed() / runs;
    println!("steady state: {each:?} per detection");
    println!("one fit runs two of them: {:?}", each * 2);
}
