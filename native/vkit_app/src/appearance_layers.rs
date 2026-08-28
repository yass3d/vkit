use crate::morph_mask::{MorphMask, resolve_weights};

#[derive(Clone, Debug)]
pub struct AppearanceLayer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub deltas: Vec<[f32; 3]>,
    pub mask: MorphMask,
}

#[derive(Debug, Default)]
pub struct AppearanceStack {
    pub layers: Vec<AppearanceLayer>,
    pub selected_id: Option<u64>,
    next_id: u64,
}

impl AppearanceStack {
    pub fn add(&mut self, name: String, deltas: Vec<[f32; 3]>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.layers.push(AppearanceLayer {
            id,
            name,
            visible: true,
            deltas,
            mask: MorphMask::default(),
        });
        self.selected_id = Some(id);
        id
    }

    pub fn remove(&mut self, id: u64) {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return;
        };
        self.layers.remove(index);
        if self.selected_id == Some(id) {
            self.selected_id = self
                .layers
                .get(index.min(self.layers.len().saturating_sub(1)))
                .map(|layer| layer.id);
        }
    }

    pub fn layer(&self, id: u64) -> Option<&AppearanceLayer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn layer_mut(&mut self, id: u64) -> Option<&mut AppearanceLayer> {
        self.layers.iter_mut().find(|layer| layer.id == id)
    }

    pub fn raise(&mut self, id: u64) {
        if let Some(index) = self.layers.iter().position(|layer| layer.id == id)
            && index + 1 < self.layers.len()
        {
            self.layers.swap(index, index + 1);
        }
    }

    pub fn move_to(&mut self, id: u64, insertion_index: usize) {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return;
        };
        let layer = self.layers.remove(index);
        let adjusted = insertion_index
            .saturating_sub(usize::from(index < insertion_index))
            .min(self.layers.len());
        self.layers.insert(adjusted, layer);
    }

    pub fn lower(&mut self, id: u64) {
        if let Some(index) = self.layers.iter().position(|layer| layer.id == id)
            && index > 0
        {
            self.layers.swap(index, index - 1);
        }
    }

    pub fn contributing(&self) -> Vec<&AppearanceLayer> {
        self.layers
            .iter()
            .rev()
            .filter(|layer| layer.visible)
            .collect()
    }
}

pub fn blend_vertex_deltas(
    layer_deltas: &[&[[f32; 3]]],
    masks: &[&MorphMask],
    vertex_count: usize,
) -> Vec<[f32; 3]> {
    let mut blended = vec![[0.0_f32; 3]; vertex_count];
    if layer_deltas.is_empty() {
        return blended;
    }
    let mut coverage = vec![0.0_f32; layer_deltas.len()];
    for (vertex, accumulated) in blended.iter_mut().enumerate() {
        let index = vertex as u32;
        for (slot, mask) in masks.iter().enumerate() {
            coverage[slot] = mask.coverage(index);
        }
        let weights = resolve_weights(&coverage);
        for (slot, deltas) in layer_deltas.iter().enumerate() {
            let weight = weights[slot];
            if weight <= 0.0 {
                continue;
            }
            let Some(delta) = deltas.get(vertex) else {
                continue;
            };
            for axis in 0..3 {
                accumulated[axis] += delta[axis] * weight;
            }
        }
    }
    blended
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack_of(count: usize) -> (AppearanceStack, Vec<u64>) {
        let mut stack = AppearanceStack::default();
        let ids = (0..count)
            .map(|index| stack.add(format!("layer {index}"), vec![[0.0, 0.0, 0.0]; 3]))
            .collect();
        (stack, ids)
    }

    #[test]
    fn a_layer_dropped_on_a_gap_lands_in_it() {
        let (mut stack, ids) = stack_of(3);
        let order = |stack: &AppearanceStack| -> Vec<u64> {
            stack.layers.iter().map(|layer| layer.id).collect()
        };
        assert_eq!(order(&stack), ids);

        stack.move_to(ids[2], 1);
        assert_eq!(order(&stack), vec![ids[0], ids[2], ids[1]]);

        stack.move_to(ids[0], 3);
        assert_eq!(order(&stack), vec![ids[2], ids[1], ids[0]]);
    }

    #[test]
    fn dragging_a_layer_says_the_same_thing_as_the_chevrons() {
        let (mut by_drag, ids) = stack_of(3);
        let (mut by_button, _) = stack_of(3);

        by_button.raise(ids[0]);
        by_drag.move_to(ids[0], 2);

        let order = |stack: &AppearanceStack| -> Vec<u64> {
            stack.layers.iter().map(|layer| layer.id).collect()
        };
        assert_eq!(
            order(&by_drag),
            order(&by_button),
            "one step by drag has to be one step by button",
        );
    }

    #[test]
    fn one_layer_is_used_whole() {
        let deltas = vec![[1.0, 2.0, 3.0]; 4];
        let mask = MorphMask::default();
        let blended = blend_vertex_deltas(&[&deltas], &[&mask], 4);
        assert_eq!(blended, deltas, "a lone appearance should arrive unchanged");
    }

    #[test]
    fn stacking_two_appearances_never_exceeds_either_of_them() {
        let top = vec![[10.0, 0.0, 0.0]; 3];
        let bottom = vec![[10.0, 0.0, 0.0]; 3];
        let clear = MorphMask::default();
        let blended = blend_vertex_deltas(&[&top, &bottom], &[&clear, &clear], 3);
        for vertex in &blended {
            assert!(
                (vertex[0] - 10.0).abs() < 1.0e-5,
                "two noses summed to {vertex:?} instead of staying one nose",
            );
        }
    }

    #[test]
    fn carving_the_top_layer_reveals_the_one_below() {
        let top = vec![[10.0, 0.0, 0.0]; 3];
        let bottom = vec![[-4.0, 0.0, 0.0]; 3];
        let mut carved = MorphMask::default();
        carved.paint(1, 0.0, 1.0);
        let clear = MorphMask::default();

        let blended = blend_vertex_deltas(&[&top, &bottom], &[&carved, &clear], 3);
        assert!((blended[0][0] - 10.0).abs() < 1.0e-5, "untouched stays top");
        assert!(
            (blended[1][0] + 4.0).abs() < 1.0e-5,
            "the carved vertex should be the lower layer, got {:?}",
            blended[1],
        );
        assert!((blended[2][0] - 10.0).abs() < 1.0e-5);
    }

    #[test]
    fn a_feathered_edge_is_a_blend_and_still_never_overshoots() {
        let top = vec![[10.0, 0.0, 0.0]; 1];
        let bottom = vec![[0.0, 0.0, 0.0]; 1];
        let mut half = MorphMask::default();
        half.paint(0, 0.0, 0.5);
        let clear = MorphMask::default();
        let blended = blend_vertex_deltas(&[&top, &bottom], &[&half, &clear], 1);
        assert!(
            (blended[0][0] - 5.0).abs() < 1.0e-4,
            "a half-carved vertex should be halfway, got {:?}",
            blended[0],
        );
    }

    #[test]
    fn hiding_a_layer_hands_its_region_down_rather_than_leaving_a_hole() {
        let mut stack = AppearanceStack::default();
        let lower = stack.add("A".into(), Vec::new());
        let upper = stack.add("B".into(), Vec::new());
        assert_eq!(stack.contributing().len(), 2);
        assert_eq!(stack.contributing()[0].id, upper, "topmost comes first");

        stack.layer_mut(upper).unwrap().visible = false;
        let contributing = stack.contributing();
        assert_eq!(contributing.len(), 1);
        assert_eq!(contributing[0].id, lower);

        let deltas = vec![[7.0, 0.0, 0.0]; 2];
        let mask = MorphMask::default();
        let blended = blend_vertex_deltas(&[&deltas], &[&mask], 2);
        assert_eq!(blended[0][0], 7.0);
    }

    #[test]
    fn a_short_layer_cannot_read_past_the_template() {
        let top: Vec<[f32; 3]> = vec![[1.0, 0.0, 0.0]];
        let bottom = vec![[2.0, 0.0, 0.0]; 3];
        let clear = MorphMask::default();
        let blended = blend_vertex_deltas(&[&top, &bottom], &[&clear, &clear], 3);
        assert_eq!(blended[0][0], 1.0);
        assert_eq!(blended[1][0], 0.0);
        assert_eq!(blended[2][0], 0.0);
    }

    #[test]
    fn raising_and_lowering_move_one_place_and_stop_at_the_ends() {
        let mut stack = AppearanceStack::default();
        let a = stack.add("A".into(), Vec::new());
        let b = stack.add("B".into(), Vec::new());
        stack.lower(b);
        assert_eq!(stack.layers[0].id, b);
        stack.lower(b);
        assert_eq!(stack.layers[0].id, b, "the bottom is the bottom");
        stack.raise(b);
        assert_eq!(stack.layers[1].id, b);
        let _ = a;
    }

    #[test]
    fn removing_the_selected_layer_selects_a_neighbour() {
        let mut stack = AppearanceStack::default();
        let a = stack.add("A".into(), Vec::new());
        let b = stack.add("B".into(), Vec::new());
        assert_eq!(stack.selected_id, Some(b));
        stack.remove(b);
        assert_eq!(stack.selected_id, Some(a));
        stack.remove(a);
        assert_eq!(stack.selected_id, None);
    }
}
