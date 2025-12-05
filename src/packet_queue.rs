//! Packet queue for reliable ordered packet delivery.

use std::collections::HashMap;

use crate::binary::Uint24;

/// An ordered queue for reliable ordered packets.
#[derive(Debug)]
pub struct PacketQueue {
    pub lowest: Uint24,
    pub highest: Uint24,
    queue: HashMap<u32, Vec<u8>>,
}

impl PacketQueue {
    /// Creates a new packet queue.
    pub fn new() -> Self {
        Self {
            lowest: Uint24::new(0),
            highest: Uint24::new(0),
            queue: HashMap::new(),
        }
    }

    /// Puts a packet at the given index. Returns false if the index was already occupied.
    pub fn put(&mut self, index: Uint24, packet: Vec<u8>) -> bool {
        if index.value() < self.lowest.value() {
            return false;
        }
        if self.queue.contains_key(&index.value()) {
            return false;
        }
        if index.value() >= self.highest.value() {
            self.highest = Uint24::new(index.value() + 1);
        }
        self.queue.insert(index.value(), packet);
        true
    }

    /// Fetches all consecutive packets starting from the lowest index.
    pub fn fetch(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        let mut index = self.lowest.value();

        while index < self.highest.value() {
            if let Some(packet) = self.queue.remove(&index) {
                packets.push(packet);
                index += 1;
            } else {
                break;
            }
        }

        self.lowest = Uint24::new(index);
        packets
    }

    /// Returns the window size.
    pub fn window_size(&self) -> u32 {
        self.highest.value() - self.lowest.value()
    }
}

impl Default for PacketQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_fetch() {
        let mut queue = PacketQueue::new();
        
        assert!(queue.put(Uint24::new(0), vec![1, 2, 3]));
        assert!(queue.put(Uint24::new(1), vec![4, 5, 6]));
        
        let packets = queue.fetch();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0], vec![1, 2, 3]);
        assert_eq!(packets[1], vec![4, 5, 6]);
    }

    #[test]
    fn test_out_of_order() {
        let mut queue = PacketQueue::new();
        
        assert!(queue.put(Uint24::new(1), vec![4, 5, 6])); // Out of order
        
        let packets = queue.fetch();
        assert_eq!(packets.len(), 0); // Can't fetch yet
        
        assert!(queue.put(Uint24::new(0), vec![1, 2, 3]));
        
        let packets = queue.fetch();
        assert_eq!(packets.len(), 2);
    }

    #[test]
    fn test_duplicate() {
        let mut queue = PacketQueue::new();
        
        assert!(queue.put(Uint24::new(0), vec![1, 2, 3]));
        assert!(!queue.put(Uint24::new(0), vec![4, 5, 6])); // Duplicate
    }
}
