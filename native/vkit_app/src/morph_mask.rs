#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphMask {
    carved: std::collections::BTreeMap<u32, f32>,
}

impl MorphMask {
    pub fn coverage(&self, vertex: u32) -> f32 {
        self.carved.get(&vertex).copied().unwrap_or(1.0)
    }

    #[cfg_attr(not(test), allow(dead_code, reason = "mask inspection, test-asserted"))]
    pub fn is_untouched(&self) -> bool {
        self.carved.is_empty()
    }

    pub fn paint(&mut self, vertex: u32, target: f32, amount: f32) {
        if !target.is_finite() || !amount.is_finite() {
            return;
        }
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            return;
        }
        let current = self.coverage(vertex);
        let next = (current + (target.clamp(0.0, 1.0) - current) * amount).clamp(0.0, 1.0);
        if (next - 1.0).abs() < 1.0e-4 {
            self.carved.remove(&vertex);
        } else {
            self.carved.insert(vertex, next);
        }
    }

    #[cfg_attr(not(test), allow(dead_code, reason = "mask inspection, test-asserted"))]
    pub fn carved_count(&self) -> usize {
        self.carved.len()
    }
}

pub fn resolve_weights(coverage_top_first: &[f32]) -> Vec<f32> {
    let mut weights = vec![0.0; coverage_top_first.len()];
    let mut remaining = 1.0_f32;
    for (index, coverage) in coverage_top_first.iter().enumerate() {
        if remaining <= 0.0 {
            break;
        }
        let take = (coverage.clamp(0.0, 1.0) * remaining).clamp(0.0, remaining);
        weights[index] = take;
        remaining -= take;
    }
    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_mask_claims_everything_and_costs_nothing() {
        let mask = MorphMask::default();
        assert!(mask.is_untouched());
        assert_eq!(mask.coverage(0), 1.0);
        assert_eq!(mask.coverage(9_999), 1.0);
    }

    #[test]
    fn painting_back_to_full_forgets_the_vertex() {
        let mut mask = MorphMask::default();
        mask.paint(7, 0.0, 1.0);
        assert_eq!(mask.coverage(7), 0.0);
        assert_eq!(mask.carved_count(), 1);
        mask.paint(7, 1.0, 1.0);
        assert_eq!(mask.coverage(7), 1.0);
        assert!(
            mask.is_untouched(),
            "a restored vertex should not be carried"
        );
    }

    #[test]
    fn no_stack_of_layers_can_sum_to_more_than_one() {
        for coverage in [
            vec![1.0, 1.0, 1.0],
            vec![1.0, 0.5, 0.25],
            vec![0.5, 0.5, 0.5, 0.5],
            vec![0.0, 0.0, 1.0],
            vec![0.3],
            vec![0.9, 0.9],
        ] {
            let weights = resolve_weights(&coverage);
            let total: f32 = weights.iter().sum();
            assert!(
                total <= 1.0 + 1.0e-5,
                "{coverage:?} resolved to {weights:?}, summing to {total}",
            );
            assert!(
                weights.iter().all(|weight| (0.0..=1.0).contains(weight)),
                "{coverage:?} resolved to {weights:?}",
            );
        }
    }

    #[test]
    fn the_top_layer_wins_where_it_claims_and_yields_where_it_does_not() {
        let weights = resolve_weights(&[1.0, 1.0]);
        assert_eq!(weights, vec![1.0, 0.0]);

        let weights = resolve_weights(&[0.0, 1.0]);
        assert_eq!(weights, vec![0.0, 1.0]);

        let weights = resolve_weights(&[0.25, 1.0]);
        assert!((weights[0] - 0.25).abs() < 1.0e-5);
        assert!((weights[1] - 0.75).abs() < 1.0e-5);
    }

    #[test]
    fn carving_through_every_layer_arrives_at_the_base_face() {
        let weights = resolve_weights(&[0.0, 0.0, 0.0]);
        assert_eq!(weights, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_single_layer_still_yields_to_its_mask() {
        assert_eq!(resolve_weights(&[1.0]), vec![1.0]);
        assert_eq!(resolve_weights(&[0.0]), vec![0.0]);
        let half = resolve_weights(&[0.5]);
        assert!((half[0] - 0.5).abs() < 1.0e-5, "got {half:?}");
    }
}

#[cfg(test)]
mod dab_tests {
    use super::*;

    #[test]
    fn a_dab_carves_less_the_further_from_its_centre_a_vertex_sits() {
        let mut mask = MorphMask::default();
        for (vertex, weight) in [(0_u32, 1.0_f32), (1, 0.5), (2, 0.05)] {
            mask.paint(vertex, 0.0, 0.3 * weight);
        }
        assert!(mask.coverage(0) < mask.coverage(1), "centre carves hardest");
        assert!(
            mask.coverage(1) < mask.coverage(2),
            "and the rim barely at all"
        );
        assert!(mask.coverage(2) > 0.98, "the rim is almost untouched");
    }

    #[test]
    fn a_single_pass_at_full_strength_leaves_room_to_go_further() {
        let mut mask = MorphMask::default();
        mask.paint(0, 0.0, 0.12);
        assert!(
            mask.coverage(0) > 0.8,
            "one dab took {} of the coverage",
            1.0 - mask.coverage(0),
        );
    }
}
