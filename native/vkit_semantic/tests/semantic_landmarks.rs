use vkit_semantic::{
    DETECTOR_INPUT_HEIGHT, DETECTOR_INPUT_WIDTH, MODEL_INPUT_HEIGHT, MODEL_INPUT_WIDTH,
    SEMANTIC_LANDMARK_COUNT, SemanticLandmarkEngine, detect_faces_rgb_f32,
};

fn procedural_face_detector_input() -> Vec<f32> {
    let crop = procedural_face_crop();
    let ratio = MODEL_INPUT_WIDTH / DETECTOR_INPUT_WIDTH;
    let mut rgb = vec![0.0; DETECTOR_INPUT_WIDTH * DETECTOR_INPUT_HEIGHT * 3];
    for y in 0..DETECTOR_INPUT_HEIGHT {
        for x in 0..DETECTOR_INPUT_WIDTH {
            let mut sums = [0.0_f32; 3];
            for row in 0..ratio {
                for column in 0..ratio {
                    let offset = ((y * ratio + row) * MODEL_INPUT_WIDTH + x * ratio + column) * 3;
                    for (channel, sum) in sums.iter_mut().enumerate() {
                        *sum += crop[offset + channel];
                    }
                }
            }
            let offset = (y * DETECTOR_INPUT_WIDTH + x) * 3;
            let count = (ratio * ratio) as f32;
            for (channel, sum) in sums.iter().enumerate() {
                rgb[offset + channel] = sum / count;
            }
        }
    }
    rgb
}

#[test]
fn the_detector_finds_the_procedural_face_where_it_actually_is() {
    let rgb = procedural_face_detector_input();
    let faces = detect_faces_rgb_f32(&rgb, DETECTOR_INPUT_WIDTH, DETECTOR_INPUT_HEIGHT)
        .expect("the detector runs");

    let face = faces.first().expect("the procedural face is found");
    let center_x = face.x + face.width * 0.5;
    let center_y = face.y + face.height * 0.5;

    assert!(
        (0.35..=0.65).contains(&center_x),
        "face centre x {center_x} should sit near the middle"
    );
    assert!(
        (0.25..=0.75).contains(&center_y),
        "face centre y {center_y} should sit near the middle"
    );
    assert!(
        face.width > 0.15 && face.width < 1.2,
        "face width {} should be a sane fraction of the frame",
        face.width
    );
    assert!(face.score >= 0.5);
}

fn procedural_face_crop() -> Vec<f32> {
    let mut rgb = vec![0.0; MODEL_INPUT_WIDTH * MODEL_INPUT_HEIGHT * 3];
    for y in 0..MODEL_INPUT_HEIGHT {
        for x in 0..MODEL_INPUT_WIDTH {
            let xf = x as f32;
            let yf = y as f32;
            let mut gray = 0.0f32;
            if ellipse(xf, yf, 128.0, 126.0, 78.0, 108.0) <= 1.0 {
                gray = 205.0;
            }
            if ellipse(xf, yf, 92.0, 104.0, 22.0, 10.0) <= 1.0
                || ellipse(xf, yf, 164.0, 104.0, 22.0, 10.0) <= 1.0
            {
                gray = 55.0;
            }
            if squared_distance(xf, yf, 92.0, 104.0) <= 16.0
                || squared_distance(xf, yf, 164.0, 104.0) <= 16.0
            {
                gray = 230.0;
            }
            if segment_distance(xf, yf, 128.0, 105.0, 116.0, 152.0) <= 2.0
                || segment_distance(xf, yf, 116.0, 152.0, 140.0, 152.0) <= 2.0
            {
                gray = 95.0;
            }
            let mouth = ellipse(xf, yf, 128.0, 176.0, 32.0, 10.0);
            if (0.78..=1.22).contains(&mouth) && yf <= 178.0 {
                gray = 70.0;
            }
            let value = (gray * (0.85 + 0.30 * xf / 255.0)).clamp(0.0, 255.0) / 255.0;
            let offset = (y * MODEL_INPUT_WIDTH + x) * 3;
            rgb[offset..offset + 3].fill(value);
        }
    }
    rgb
}

fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)
}

fn squared_distance(x: f32, y: f32, cx: f32, cy: f32) -> f32 {
    (x - cx).powi(2) + (y - cy).powi(2)
}

#[allow(clippy::too_many_arguments)]
fn segment_distance(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let length_squared = dx * dx + dy * dy;
    let t = (((x - ax) * dx + (y - ay) * dy) / length_squared).clamp(0.0, 1.0);
    ((x - (ax + t * dx)).powi(2) + (y - (ay + t * dy)).powi(2)).sqrt()
}

#[test]
fn global_engine_is_cached_and_procedural_face_is_stable() {
    let first = SemanticLandmarkEngine::global().unwrap();
    let second = SemanticLandmarkEngine::global().unwrap();
    assert!(std::ptr::eq(first, second));

    let prediction = first
        .detect_landmarks_rgb_f32(
            &procedural_face_crop(),
            MODEL_INPUT_WIDTH,
            MODEL_INPUT_HEIGHT,
        )
        .unwrap();
    assert_eq!(prediction.landmarks.len(), SEMANTIC_LANDMARK_COUNT);
    assert!(prediction.presence_probability > 0.99);
    assert!(
        prediction
            .landmarks
            .iter()
            .flat_map(|point| [point.x, point.y, point.z])
            .all(f32::is_finite)
    );

    let expected = [
        (0, [0.506_337_17, 0.643_057_8, -0.091_887_69]),
        (33, [0.308_699, 0.407_689_45, 0.053_336_01]),
        (263, [0.709_239, 0.410_606_95, 0.063_012_61]),
        (468, [0.375_789_52, 0.401_351_75, 0.029_988_23]),
        (477, [0.659_386_8, 0.430_177_66, 0.035_527_77]),
    ];
    for (index, expected) in expected {
        let point = prediction.landmarks[index];
        let actual = [point.x, point.y, point.z];
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-4);
        }
    }
    assert!((prediction.presence_logit - 16.885_065).abs() < 1.0e-3);
    assert!((prediction.tongue_out_probability - 0.136_279_37).abs() < 1.0e-4);
}
