use crate::hair_project::HairStrand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Paint {
    Raise,
    Lower,
}

const STEP: f32 = 0.08;

#[must_use]
pub fn seed(physics: &vkit_core::vam::HairPhysicsSettings, strand: &HairStrand) -> Vec<f32> {
    let points = strand.points_cm.len();
    (0..points)
        .map(|index| vkit_core::vam::rolloff_rigidity(physics, index, points))
        .collect()
}

#[must_use]
pub fn paint(
    physics: &vkit_core::vam::HairPhysicsSettings,
    strand: &HairStrand,
    weights: &[f32],
    paint: Paint,
    strength: f32,
) -> Option<Vec<f32>> {
    let step = STEP * strength.clamp(0.0, 1.0);
    if step <= 0.0 {
        return None;
    }
    let direction = match paint {
        Paint::Raise => 1.0,
        Paint::Lower => -1.0,
    };
    let mut values = if strand.is_painted() {
        strand.rigidity.clone()
    } else {
        seed(physics, strand)
    };
    values.resize(strand.points_cm.len(), 0.0);

    let mut moved = false;
    for (index, value) in values.iter_mut().enumerate() {
        if index == 0 {
            continue;
        }
        let weight = weights.get(index).copied().unwrap_or(0.0);
        if weight <= 1.0e-4 {
            continue;
        }
        let next = (*value + direction * step * weight).clamp(0.0, 1.0);
        if (next - *value).abs() > 1.0e-5 {
            *value = next;
            moved = true;
        }
    }
    moved.then_some(values)
}

#[must_use]
pub fn ink(value: f32) -> egui::Color32 {
    const LOOSE: [f32; 3] = [228.0, 62.0, 52.0];
    const HALF: [f32; 3] = [240.0, 196.0, 48.0];
    const HELD: [f32; 3] = [72.0, 200.0, 96.0];

    let value = value.clamp(0.0, 1.0);
    let (from, to, blend) = if value < 0.5 {
        (LOOSE, HALF, value * 2.0)
    } else {
        (HALF, HELD, (value - 0.5) * 2.0)
    };
    let channel = |index: usize| (from[index] + (to[index] - from[index]) * blend) as u8;
    egui::Color32::from_rgb(channel(0), channel(1), channel(2))
}

const TAPER_FROM: f32 = 0.75;

pub const STROKE: f32 = 4.0;

#[must_use]
pub fn half_width(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let keep = if t <= TAPER_FROM {
        1.0
    } else {
        ((1.0 - t) / (1.0 - TAPER_FROM)).max(0.0)
    };
    STROKE * 0.5 * keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn physics() -> vkit_core::vam::HairPhysicsSettings {
        vkit_core::vam::HairPhysicsSettings {
            root_rigidity: 0.6,
            main_rigidity: 0.4,
            tip_rigidity: 0.0,
            rigidity_rolloff_power: 2.0,
            ..vkit_core::vam::HairPhysicsSettings::default()
        }
    }

    fn strand(points: usize) -> HairStrand {
        HairStrand::new((0..points).map(|i| [0.0, i as f32, 0.0]).collect())
    }

    #[test]
    fn the_seed_is_exactly_the_curve_the_game_would_have_used() {
        let strand = strand(12);
        for (index, value) in seed(&physics(), &strand).into_iter().enumerate() {
            let expected = vkit_core::vam::rolloff_rigidity(&physics(), index, 12);
            assert!(
                (value - expected).abs() < 1.0e-6,
                "point {index} seeded {value}, curve says {expected}",
            );
        }
    }

    #[test]
    fn a_first_stroke_departs_from_the_curve_rather_than_from_nothing() {
        let strand = strand(12);
        let weights = vec![1.0; 12];
        let painted = paint(&physics(), &strand, &weights, Paint::Raise, 1.0).expect("a stroke");
        let curve = seed(&physics(), &strand);
        for index in 1..12 {
            let moved = painted[index] - curve[index];
            assert!(
                moved > 0.0 && moved <= STEP + 1.0e-6,
                "point {index} jumped by {moved}",
            );
        }
    }

    #[test]
    fn the_root_is_never_painted() {
        let strand = strand(12);
        let painted = paint(&physics(), &strand, &[1.0; 12], Paint::Raise, 1.0).expect("a stroke");
        assert!((painted[0] - 1.0).abs() < 1.0e-6, "the anchor moved");
    }

    #[test]
    fn a_weld_takes_more_than_one_pass_but_is_reachable() {
        let mut strand = strand(12);
        let weights = vec![1.0; 12];
        let mut passes = 0;
        while strand.rigidity.get(6).copied().unwrap_or(0.0) < 1.0 {
            let Some(values) = paint(&physics(), &strand, &weights, Paint::Raise, 1.0) else {
                break;
            };
            strand.rigidity = values;
            passes += 1;
            assert!(passes < 200, "a weld never arrives");
        }
        assert!(passes > 5, "one pass welded the strand: {passes}");
        assert_eq!(strand.rigidity[6], 1.0);
    }

    #[test]
    fn a_stroke_that_moves_nothing_leaves_the_strand_unpainted() {
        let strand = strand(12);
        assert!(paint(&physics(), &strand, &[0.0; 12], Paint::Raise, 1.0).is_none());
        assert!(paint(&physics(), &strand, &[1.0; 12], Paint::Raise, 0.0).is_none());
        let floored = HairStrand {
            rigidity: vec![0.0; 12],
            ..strand.clone()
        };
        assert!(paint(&physics(), &floored, &[1.0; 12], Paint::Lower, 1.0).is_none());
    }

    #[test]
    fn the_brush_falloff_reaches_the_values() {
        let strand = strand(12);
        let mut weights = vec![0.0; 12];
        weights[4] = 1.0;
        weights[5] = 0.25;
        let painted = paint(&physics(), &strand, &weights, Paint::Raise, 1.0).expect("a stroke");
        let curve = seed(&physics(), &strand);
        let moved = |index: usize| painted[index] - curve[index];
        assert!(moved(3).abs() < 1.0e-6, "a point outside the brush moved");
        assert!(
            (moved(4) - moved(5) * 4.0).abs() < 1.0e-4,
            "the falloff was flattened: {} against {}",
            moved(4),
            moved(5),
        );
    }

    #[test]
    fn the_heatmap_runs_red_through_yellow_to_green() {
        let (loose, half, stiff) = (ink(0.0), ink(0.5), ink(1.0));
        assert!(loose.r() > 200 && loose.g() < 90, "{loose:?} is not red");
        assert!(stiff.g() > 180 && stiff.r() < 100, "{stiff:?} is not green");
        assert!(
            half.r() > 200 && half.g() > 170 && half.b() < 90,
            "{half:?} is not yellow",
        );
        let lit = |ink: egui::Color32| f32::from(ink.r()) + f32::from(ink.g());
        let direct = (lit(loose) + lit(stiff)) * 0.5;
        let bent = lit(half);
        assert!(bent > direct * 1.15, "{bent} against {direct}");
    }

    #[test]
    fn the_stroke_is_full_width_most_of_the_way_and_then_comes_to_a_point() {
        assert!((half_width(0.0) - STROKE * 0.5).abs() < 1.0e-6);
        assert!(
            (half_width(TAPER_FROM) - STROKE * 0.5).abs() < 1.0e-6,
            "the taper started early",
        );
        assert!(half_width(1.0) < 1.0e-6, "the tip is blunt");
        assert!(
            half_width(0.5) > half_width(0.9),
            "the width has to fall, not rise",
        );
        let mut last = f32::MAX;
        for step in 0..=100 {
            let now = half_width(step as f32 / 100.0);
            assert!(now <= last + 1.0e-6, "the width grew at {step}");
            last = now;
        }
    }
}
