use egui::{Pos2, Rect, pos2};

const DEPTH_FIELD_CELL: f32 = 3.0;

const DEPTH_FIELD_FLOOR_CM: f32 = 0.35;

const DEPTH_FIELD_SLACK_CELLS: f32 = 6.0;

pub(crate) struct SurfaceDepth {
    rect: Rect,

    slack: f32,
    cols: usize,
    rows: usize,
    nearest: Vec<f32>,
}

impl SurfaceDepth {
    pub(crate) fn new(rect: Rect, world_per_point: f32) -> Self {
        let cols = ((rect.width() / DEPTH_FIELD_CELL).ceil() as usize).max(1);
        let rows = ((rect.height() / DEPTH_FIELD_CELL).ceil() as usize).max(1);
        Self {
            rect,
            slack: (world_per_point.max(0.0) * DEPTH_FIELD_CELL * DEPTH_FIELD_SLACK_CELLS)
                .max(DEPTH_FIELD_FLOOR_CM),
            cols,
            rows,
            nearest: vec![f32::INFINITY; cols * rows],
        }
    }

    pub(crate) fn coords(&self, screen: Pos2) -> Option<(usize, usize)> {
        let x = ((screen.x - self.rect.left()) / DEPTH_FIELD_CELL).floor();
        let y = ((screen.y - self.rect.top()) / DEPTH_FIELD_CELL).floor();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        (x < self.cols && y < self.rows).then_some((x, y))
    }

    pub(crate) fn mark(&mut self, screen: Pos2, distance: f32) {
        if let Some((x, y)) = self.coords(screen) {
            let cell = &mut self.nearest[y * self.cols + x];
            if distance < *cell {
                *cell = distance;
            }
        }
    }

    pub(crate) fn fill_triangle(&mut self, a: (Pos2, f32), b: (Pos2, f32), c: (Pos2, f32)) {
        let area = (b.0.x - a.0.x) * (c.0.y - a.0.y) - (c.0.x - a.0.x) * (b.0.y - a.0.y);
        if area.abs() < 1.0e-6 {
            self.draw_span(a, b);
            self.draw_span(b, c);
            self.draw_span(c, a);
            return;
        }
        let left = a.0.x.min(b.0.x).min(c.0.x).max(self.rect.left());
        let right = a.0.x.max(b.0.x).max(c.0.x).min(self.rect.right());
        let top = a.0.y.min(b.0.y).min(c.0.y).max(self.rect.top());
        let bottom = a.0.y.max(b.0.y).max(c.0.y).min(self.rect.bottom());
        if left > right || top > bottom {
            return;
        }
        let Some((first_col, first_row)) = self.coords(pos2(left, top)) else {
            return;
        };
        let last_col = self
            .coords(pos2(right, bottom))
            .map_or(first_col, |(col, _)| col);
        let last_row = self
            .coords(pos2(right, bottom))
            .map_or(first_row, |(_, row)| row);
        for row in first_row..=last_row.min(self.rows - 1) {
            for col in first_col..=last_col.min(self.cols - 1) {
                let centre = pos2(
                    self.rect.left() + (col as f32 + 0.5) * DEPTH_FIELD_CELL,
                    self.rect.top() + (row as f32 + 0.5) * DEPTH_FIELD_CELL,
                );
                let w0 = ((b.0.x - centre.x) * (c.0.y - centre.y)
                    - (c.0.x - centre.x) * (b.0.y - centre.y))
                    / area;
                let w1 = ((c.0.x - centre.x) * (a.0.y - centre.y)
                    - (a.0.x - centre.x) * (c.0.y - centre.y))
                    / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let depth = w0 * a.1 + w1 * b.1 + w2 * c.1;
                let cell = &mut self.nearest[row * self.cols + col];
                if depth < *cell {
                    *cell = depth;
                }
            }
        }
    }

    pub(crate) fn draw_span(&mut self, from: (Pos2, f32), to: (Pos2, f32)) {
        let steps = ((from.0 - to.0).length() / DEPTH_FIELD_CELL)
            .ceil()
            .max(1.0);
        let steps = (steps as usize).min(512);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            self.mark(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        }
    }

    #[cfg(test)]
    pub(crate) fn probe(rect: Rect, world_per_point: f32) -> Self {
        Self::new(rect, world_per_point)
    }

    #[cfg(test)]
    pub(crate) fn probe_mark(&mut self, screen: Pos2, distance: f32) {
        self.mark(screen, distance);
    }

    #[cfg(test)]
    pub(crate) fn probe_fill(&mut self, a: (Pos2, f32), b: (Pos2, f32), c: (Pos2, f32)) {
        self.fill_triangle(a, b, c);
    }

    #[must_use]
    pub(crate) fn hides(&self, screen: Pos2, distance: f32) -> bool {
        let Some((x, y)) = self.coords(screen) else {
            return false;
        };
        let mut nearest = f32::INFINITY;
        for row in y.saturating_sub(1)..=(y + 1).min(self.rows - 1) {
            for col in x.saturating_sub(1)..=(x + 1).min(self.cols - 1) {
                nearest = nearest.min(self.nearest[row * self.cols + col]);
            }
        }
        distance > nearest + self.slack
    }
}
