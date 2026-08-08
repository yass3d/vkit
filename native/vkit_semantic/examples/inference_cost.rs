//! Times the landmark model, so a size decision can be made against a number
//! rather than a guess.
//!
//! Not a test: it measures the machine it runs on, and a threshold that passes
//! here would fail on someone else's laptop. Run it directly when the question
//! comes up.
//!
//!     cargo run --profile release-small -p vkit-semantic --example inference_cost

fn main() {
    const SIDE: usize = vkit_semantic::MODEL_INPUT_WIDTH;
    // A flat grey frame. The model does the same work whatever is in it, and a
    // fixed input keeps two builds comparing like with like.
    let frame = vec![0.5_f32; SIDE * SIDE * 3];

    // The first call unpacks and verifies the embedded model, which is not what
    // is being measured.
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
