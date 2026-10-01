//! Counters such as section numbers (SILE's `counters` package).

/// A counter with one value per level, shown as `1.2.3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultilevelCounter {
    pub values: Vec<i64>,
}

impl Default for MultilevelCounter {
    fn default() -> Self {
        Self { values: vec![0] }
    }
}

impl MultilevelCounter {
    /// Step the counter at `level` (counting from 1; the deepest level when
    /// `None`). Going deeper opens the new levels at 0 and the stepped one
    /// at 1; going shallower drops the deeper levels when `reset`.
    pub fn increment(&mut self, level: Option<usize>, reset: bool) {
        let level = level.unwrap_or(self.values.len()).max(1);
        if level > self.values.len() {
            self.values.resize(level - 1, 0);
            self.values.push(1);
        } else {
            self.values[level - 1] += 1;
            if reset {
                self.values.truncate(level);
            }
        }
    }

    /// Set the value at `level`, opening or dropping levels as `increment`
    /// does.
    pub fn set(&mut self, level: usize, value: i64) {
        let level = level.max(1);
        self.values.resize(level, 0);
        self.values[level - 1] = value;
    }

    /// The levels up to `level` (all of them when `None`), joined by dots.
    pub fn format(&self, level: Option<usize>) -> String {
        let max = level.unwrap_or(self.values.len()).min(self.values.len());
        self.values[..max].iter().map(i64::to_string).collect::<Vec<_>>().join(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_open_and_close() {
        let mut c = MultilevelCounter::default();
        assert_eq!(c.format(None), "0");
        c.increment(Some(3), true);
        assert_eq!(c.format(Some(3)), "0.0.1");
        c.increment(Some(3), true);
        assert_eq!(c.format(None), "0.0.2");
        c.increment(Some(2), true);
        assert_eq!(c.format(None), "0.1");
        c.increment(Some(3), false);
        c.increment(Some(1), false);
        assert_eq!(c.format(None), "1.1.1");
    }
}
