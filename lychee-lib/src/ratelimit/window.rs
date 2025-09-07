use std::collections::VecDeque;

/// A rolling window data structure that automatically maintains a maximum size
/// by removing oldest elements when the capacity is exceeded.
#[derive(Debug, Clone)]
pub struct Window<T> {
    data: VecDeque<T>,
    capacity: usize,
}

impl<T> Window<T> {
    /// Create a new window with the given capacity
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push an element to the window, removing the oldest if at capacity
    pub fn push(&mut self, item: T) {
        if self.data.len() >= self.capacity {
            self.data.pop_front();
        }
        self.data.push_back(item);
    }

    /// Get the number of elements currently in the window
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the window is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get an iterator over the elements in the window
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    /// Convert to a vector (for compatibility with existing code)
    #[must_use]
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.data.iter().cloned().collect()
    }
}

impl<T> Default for Window<T> {
    fn default() -> Self {
        Self::new(100) // Default capacity of 100 items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_capacity() {
        let mut window = Window::new(3);

        // Fill up the window
        window.push(1);
        window.push(2);
        window.push(3);
        assert_eq!(window.len(), 3);

        // Add one more, should remove the oldest
        window.push(4);
        assert_eq!(window.len(), 3);

        let values: Vec<_> = window.iter().copied().collect();
        assert_eq!(values, vec![2, 3, 4]);
    }

    #[test]
    fn test_window_empty() {
        let window: Window<i32> = Window::new(5);
        assert!(window.is_empty());
        assert_eq!(window.len(), 0);
    }

    #[test]
    fn test_window_to_vec() {
        let mut window = Window::new(3);
        window.push(1);
        window.push(2);

        let vec = window.to_vec();
        assert_eq!(vec, vec![1, 2]);
    }
}
