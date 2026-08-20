pub(crate) struct UnityRandom {
    x: u32,
    y: u32,
    z: u32,
    w: u32,
}

impl UnityRandom {
    pub(crate) fn new(seed: i32) -> Self {
        let x = seed as u32;
        let y = x.wrapping_mul(1_812_433_253).wrapping_add(1);
        let z = y.wrapping_mul(1_812_433_253).wrapping_add(1);
        let w = z.wrapping_mul(1_812_433_253).wrapping_add(1);
        Self { x, y, z, w }
    }

    fn next(&mut self) -> u32 {
        let t = self.x ^ (self.x << 11);
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        self.w = self.w ^ (self.w >> 19) ^ t ^ (t >> 8);
        self.w
    }

    pub(crate) fn value(&mut self) -> f32 {
        (self.next() & 0x7f_ffff) as f32 / 8_388_607.0
    }
}

pub(crate) fn strand_randoms(strands: usize) -> Vec<[f32; 3]> {
    let mut random = UnityRandom::new(5);
    (0..strands)
        .map(|_| {
            let x = random.value();
            let y = random.value();
            let z = random.value();
            [x, y, z]
        })
        .collect()
}

pub(crate) fn barycentric_table(strands: usize) -> Vec<[f32; 3]> {
    let mut random = UnityRandom::new(6);
    (0..strands * BARYCENTRICS_PER_STRAND)
        .map(|_| {
            let mut x = random.value();
            let mut y = random.value();
            if x + y > 1.0 {
                x = 1.0 - x;
                y = 1.0 - y;
            }
            [x, y, 1.0 - (x + y)]
        })
        .collect()
}

pub(crate) const BARYCENTRICS_PER_STRAND: usize = 64;

pub(crate) fn child_slot(child: usize, children: usize) -> usize {
    (child * BARYCENTRICS_PER_STRAND / children.max(1)).min(BARYCENTRICS_PER_STRAND - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequence_is_pinned() {
        let mut random = UnityRandom::new(5);
        let first: Vec<f32> = (0..4).map(|_| random.value()).collect();
        let again: Vec<f32> = {
            let mut random = UnityRandom::new(5);
            (0..4).map(|_| random.value()).collect()
        };
        assert_eq!(first, again, "same seed, same sequence");
        let mut other = UnityRandom::new(6);
        let other_first: Vec<f32> = (0..4).map(|_| other.value()).collect();
        assert_ne!(first, other_first, "different seeds diverge");
        for value in first.iter().chain(other_first.iter()) {
            assert!((0.0..=1.0).contains(value), "values stay in [0, 1]");
        }
    }

    #[test]
    fn barycentrics_are_barycentric() {
        for entry in barycentric_table(3) {
            let sum: f32 = entry.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1.0e-6,
                "components sum to one: {entry:?}"
            );
            assert!(
                entry.iter().all(|component| *component >= 0.0),
                "no component negative: {entry:?}",
            );
        }
        assert_eq!(barycentric_table(3).len(), 3 * BARYCENTRICS_PER_STRAND);
    }

    #[test]
    fn slots_stride_through_the_table() {
        assert_eq!(child_slot(0, 16), 0);
        assert_eq!(child_slot(1, 16), 4);
        assert_eq!(child_slot(15, 16), 60);
        assert_eq!(child_slot(0, 64), 0);
        assert_eq!(child_slot(63, 64), 63);
        assert_eq!(child_slot(1, 8), child_slot(8, 64));
    }
}
