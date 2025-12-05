//! Datagram window for tracking received datagrams and detecting missing ones.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::binary::Uint24;

/// A queue for tracking incoming datagrams.
#[derive(Debug)]
pub struct DatagramWindow {
    pub lowest: Uint24,
    pub highest: Uint24,
    queue: HashMap<u32, Instant>,
}

impl DatagramWindow {
    /// Creates a new datagram window.
    pub fn new() -> Self {
        Self {
            lowest: Uint24::new(0),
            highest: Uint24::new(0),
            queue: HashMap::new(),
        }
    }

    /// Adds an index to the window. Returns false if already seen.
    pub fn add(&mut self, index: Uint24) -> bool {
        if self.seen(index) {
            return false;
        }
        if index.value() + 1 > self.highest.value() {
            self.highest = Uint24::new(index.value() + 1);
        }
        self.queue.insert(index.value(), Instant::now());
        true
    }

    /// Checks if the index has been seen before.
    pub fn seen(&self, index: Uint24) -> bool {
        if index.value() < self.lowest.value() {
            return true;
        }
        self.queue.contains_key(&index.value())
    }

    /// Shifts the window, removing consecutive indices from the lowest.
    /// Returns the number of indices shifted.
    pub fn shift(&mut self) -> usize {
        let mut n = 0;
        let mut index = self.lowest.value();

        while index < self.highest.value() {
            if !self.queue.contains_key(&index) {
                break;
            }
            self.queue.remove(&index);
            n += 1;
            index += 1;
        }

        self.lowest = Uint24::new(index);
        n
    }

    /// Returns a list of missing indices that have been waiting too long.
    pub fn missing(&mut self, since: Duration) -> Vec<Uint24> {
        let mut indices = Vec::new();
        let mut missing = false;
        let now = Instant::now();

        // Iterate from highest to lowest
        for index in (self.lowest.value()..self.highest.value()).rev() {
            if let Some(&time) = self.queue.get(&index) {
                if now.duration_since(time) >= since {
                    // All packets before this one took too long to arrive
                    missing = true;
                }
                continue;
            }
            if missing {
                indices.push(Uint24::new(index));
                // Mark as "requested" with a zero time
                self.queue.insert(index, Instant::now() - Duration::from_secs(3600));
            }
        }

        self.shift();
        indices
    }

    /// Returns the size of the window.
    pub fn size(&self) -> u32 {
        self.highest.value() - self.lowest.value()
    }
}

impl Default for DatagramWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_seen() {
        let mut win = DatagramWindow::new();
        assert!(win.add(Uint24::new(0)));
        assert!(!win.add(Uint24::new(0))); // Already seen
        assert!(win.add(Uint24::new(1)));
        assert!(win.seen(Uint24::new(0)));
        assert!(win.seen(Uint24::new(1)));
    }

    #[test]
    fn test_shift() {
        let mut win = DatagramWindow::new();
        win.add(Uint24::new(0));
        win.add(Uint24::new(1));
        win.add(Uint24::new(2));
        
        let shifted = win.shift();
        assert_eq!(shifted, 3);
        assert_eq!(win.lowest.value(), 3);
    }

    #[test]
    fn test_shift_with_gap() {
        let mut win = DatagramWindow::new();
        win.add(Uint24::new(0));
        win.add(Uint24::new(2)); // Gap at 1
        
        let shifted = win.shift();
        assert_eq!(shifted, 1);
        assert_eq!(win.lowest.value(), 1);
    }
}
