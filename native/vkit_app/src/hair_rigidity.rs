//! Painting rigidity onto strand points, the way the game's style tools do.
//!
//! Rigidity is a point joint: every particle is pulled toward the position it
//! was authored at, skinned by the scalp matrix, at a strength this decides.
//! Unpainted, that strength comes from the `rootRigidity` / `mainRigidity` /
//! `tipRigidity` rolloff — three sliders for the whole item. Painted, it comes
//! from an array with one value per point, and the sliders stop being read.
//!
//! That is what `usePaintedRigidity` switches between, and it is why the paint
//! has to start as a copy of the curve rather than at zero: the moment the
//! toggle flips, every point that has not been touched still has to look
//! exactly as it did.
//!
//! # The value 1.0 is not "very stiff"
//!
//! `CSPointJoints` ends with `if (J.Rigidity >= 1.0 || isFixed == 1) result =
//! target` — an unconditional snap, no lerp, no velocity. A point painted 1.0
//! is welded to its rest pose and will not move for gravity, wind or a hand.
//! That is a real thing to want at a clip or a tie, so the brush can reach it,
//! but it must be reached deliberately: the seed is the curve, and the stroke
//! walks.

use crate::hair_project::HairStrand;

/// Which way a stroke moves the value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Paint {
    /// Toward stiff.
    Raise,
    /// Toward loose.
    Lower,
}

/// How much of the range one full-strength stroke step covers.
///
/// Slow on purpose. The whole point of painting is the shape of the falloff
/// between stiff and loose, and a brush that saturates in one pass paints a
/// step edge instead.
const STEP: f32 = 0.08;

/// What a point starts at before anyone paints on it.
///
/// The curve the game would have used, so flipping `usePaintedRigidity` on an
/// item nobody has painted changes nothing at all.
#[must_use]
pub fn seed(physics: &vkit_core::vam::HairPhysicsSettings, strand: &HairStrand) -> Vec<f32> {
    let points = strand.points_cm.len();
    (0..points)
        .map(|index| vkit_core::vam::rolloff_rigidity(physics, index, points))
        .collect()
}

/// Move one strand's painted rigidity under a brush.
///
/// `weights` is the brush falloff at each point, already worked out by the
/// caller in the same way every other hair brush works it out. Returns `None`
/// when the stroke would change nothing, so an unpainted part is not dragged
/// into carrying an array by a brush that merely passed over it.
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
        // The scalp anchor is not ours to paint. The game writes 1.1 there to
        // trip its own snap, and a value we put in its place would be read
        // instead.
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

/// Red for loose, green for stiff — the heatmap the game's debug kernel draws.
///
/// Not a decoration: painted rigidity is invisible until something shows it,
/// and a brush whose effect you cannot see is a brush nobody can aim.
#[must_use]
pub fn ink(value: f32) -> egui::Color32 {
    let value = value.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (255.0 * (1.0 - value)) as u8,
        (235.0 * value) as u8 + 20,
        40,
    )
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

    /// Flipping the toggle on an unpainted item must not move a hair. The seed
    /// is the curve, point for point.
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

    /// A first stroke starts from the curve, not from zero — otherwise the
    /// first touch drops the whole strand loose before it stiffens anything.
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

    /// The scalp anchor is the game's, not ours.
    #[test]
    fn the_root_is_never_painted() {
        let strand = strand(12);
        let painted = paint(&physics(), &strand, &[1.0; 12], Paint::Raise, 1.0).expect("a stroke");
        assert!((painted[0] - 1.0).abs() < 1.0e-6, "the anchor moved");
    }

    /// Reaching a full weld has to take deliberate work, because 1.0 is not a
    /// stiffness — it is `result = target`, which no force can move.
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

    /// A brush that changes nothing must not start an array. An item with no
    /// paint on it exports no array at all, and that is the difference between
    /// a file the game reads as before and one that welds solid.
    #[test]
    fn a_stroke_that_moves_nothing_leaves_the_strand_unpainted() {
        let strand = strand(12);
        assert!(paint(&physics(), &strand, &[0.0; 12], Paint::Raise, 1.0).is_none());
        assert!(paint(&physics(), &strand, &[1.0; 12], Paint::Raise, 0.0).is_none());
        // Already at the floor, being pushed further down.
        let floored = HairStrand {
            rigidity: vec![0.0; 12],
            ..strand.clone()
        };
        assert!(paint(&physics(), &floored, &[1.0; 12], Paint::Lower, 1.0).is_none());
    }

    /// The falloff is the brush's, and it has to survive into the values.
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

    /// Loose reads red and stiff reads green, so a glance says which is which.
    #[test]
    fn the_heatmap_runs_red_to_green() {
        let (loose, stiff) = (ink(0.0), ink(1.0));
        assert!(loose.r() > 200 && loose.g() < 60, "{loose:?} is not red");
        assert!(stiff.g() > 200 && stiff.r() < 60, "{stiff:?} is not green");
    }
}
