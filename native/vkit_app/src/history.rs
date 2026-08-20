use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct History<T> {
    behind: VecDeque<T>,
    ahead: VecDeque<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> History<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            behind: VecDeque::new(),
            ahead: VecDeque::new(),
        }
    }

    pub fn record(&mut self, here: T) {
        self.ahead.clear();
        self.behind.push_back(here);
    }

    pub fn trim(&mut self, max_steps: usize, max_bytes: usize, cost: impl Fn(&T) -> usize) {
        let mut held: usize = self.behind.iter().map(&cost).sum();
        while self.behind.len() > 1
            && (self.behind.len() > max_steps || held > max_bytes)
            && let Some(dropped) = self.behind.pop_front()
        {
            held = held.saturating_sub(cost(&dropped));
        }
    }

    pub fn undo(&mut self, here: T) -> Option<T> {
        let step = self.behind.pop_back()?;
        self.ahead.push_back(here);
        Some(step)
    }

    pub fn redo(&mut self, here: T) -> Option<T> {
        let step = self.ahead.pop_back()?;
        self.behind.push_back(here);
        Some(step)
    }

    pub fn clear_forward(&mut self) {
        self.ahead.clear();
    }

    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        (self.behind.len(), self.ahead.len())
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.behind.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.ahead.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_back_and_forward_returns_to_where_it_started() {
        let mut history = History::new();
        history.record(1);
        history.record(2);
        assert_eq!(history.position(), (2, 0));

        assert_eq!(history.undo(3), Some(2));
        assert_eq!(history.position(), (1, 1));
        assert_eq!(history.undo(2), Some(1));
        assert_eq!(history.position(), (0, 2));
        assert_eq!(history.undo(1), None, "nothing behind");

        assert_eq!(history.redo(1), Some(2));
        assert_eq!(history.redo(2), Some(3));
        assert_eq!(history.position(), (2, 0));
        assert_eq!(history.redo(3), None, "nothing ahead");
    }

    #[test]
    fn recording_a_step_drops_the_road_ahead() {
        let mut history = History::new();
        history.record(1);
        history.undo(2);
        assert_eq!(history.position(), (0, 1));

        history.record(9);
        assert_eq!(history.position(), (1, 0), "the branch is gone");
        assert_eq!(history.redo(0), None);
    }

    #[test]
    fn a_step_count_forgets_the_oldest_first() {
        let mut history = History::new();
        for step in 1..=5 {
            history.record(step);
            history.trim(3, usize::MAX, |_| 0);
        }
        assert_eq!(history.position(), (3, 0));
        assert_eq!(history.undo(0), Some(5));
        assert_eq!(history.undo(0), Some(4));
        assert_eq!(history.undo(0), Some(3), "1 and 2 were forgotten");
    }

    #[test]
    fn a_byte_budget_forgets_before_a_generous_count_does() {
        let mut history = History::new();
        for step in [100_usize, 100, 100] {
            history.record(step);
            history.trim(512, 250, |cost| *cost);
        }
        assert_eq!(history.position(), (2, 0), "250 holds two of three");

        let mut heavy = History::new();
        heavy.record(10_000_usize);
        heavy.trim(512, 1, |cost| *cost);
        assert_eq!(heavy.position(), (1, 0), "the last step is never dropped");
    }

    #[test]
    fn giving_up_the_road_ahead_leaves_the_road_behind() {
        let mut history = History::new();
        history.record(1);
        history.record(2);
        history.undo(3);
        history.clear_forward();
        assert_eq!(history.position(), (1, 0));
        assert!(history.can_undo() && !history.can_redo());
    }
}
