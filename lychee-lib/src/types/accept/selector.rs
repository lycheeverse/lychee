use crate::types::accept::AcceptRange;

/// An [`AcceptSelector`] determines if a returned HTTP status code should be
/// accepted and thus counted as a valid (not broken) link.
#[derive(Debug)]
pub struct AcceptSelector {
    ranges: Vec<AcceptRange>,
}

impl AcceptSelector {
    /// Creates a new empty [`AcceptSelector`].
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Adds a range of accepted HTTP status codes to this [`AcceptSelector`].
    /// This method merges the new and existing ranges if they overlap.
    pub fn add_range(&mut self, range: AcceptRange) -> &mut Self {
        // Merge with previous range if possible
        if let Some(last) = self.ranges.last_mut() {
            // Merge when there is an overlap between the last end value and the
            // to be inserted new range start value.
            if last.end() >= range.start() {
                last.update_end(*range.end());
                return self;
            }

            // Merge when there is an overlap between the last start value and
            // the to be inserted new range end value. Only do this, if the new
            // start value is smaller than the last start value.
            if last.start() <= range.end() && range.start() <= last.start() {
                last.update_start(*range.start());
                return self;
            }
        }

        // If neither is the case, the ranges have no overlap at all. Just add
        // to the list of ranges.
        self.ranges.push(range);
        self
    }

    /// Returns whether this [`AcceptSelector`] contains `value`.
    pub fn contains(&self, value: usize) -> bool {
self.ranges.iter().any(|range| range.contains(value))
    }

    pub(crate) fn len(&self) -> usize {
        self.ranges.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_non_overlapping_ranges() {
        let range1 = AcceptRange::new(0, 10);
        let range2 = AcceptRange::new(20, 30);

        let mut selector = AcceptSelector::new();
        selector.add_range(range1).add_range(range2);

        assert!(selector.contains(0));
        assert!(selector.contains(10));
        assert!(selector.contains(20));
        assert!(selector.contains(30));

        assert!(!selector.contains(15));
        assert!(!selector.contains(35));

        assert_eq!(selector.len(), 2);
    }

    #[test]
    fn test_overlapping_start_ranges() {
        let range1 = AcceptRange::new(8, 20);
        let range2 = AcceptRange::new(0, 10);

        let mut selector = AcceptSelector::new();
        selector.add_range(range1).add_range(range2);

        assert!(selector.contains(0));
        assert!(selector.contains(10));
        assert!(selector.contains(20));

        assert!(!selector.contains(35));

        assert_eq!(selector.len(), 1);
    }

    #[test]
    fn test_overlapping_end_ranges() {
        let range1 = AcceptRange::new(0, 10);
        let range2 = AcceptRange::new(8, 20);

        let mut selector = AcceptSelector::new();
        selector.add_range(range1).add_range(range2);

        assert!(selector.contains(0));
        assert!(selector.contains(10));
        assert!(selector.contains(20));

        assert!(!selector.contains(35));

        assert_eq!(selector.len(), 1);
    }
}
