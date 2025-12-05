//! Resend map for tracking packets that need retransmission.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::binary::Uint24;
use crate::packet::Packet;

/// Time window for RTT calculation.
const RTT_CALCULATION_WINDOW: Duration = Duration::from_secs(5);

/// A record of a packet pending acknowledgement.
#[derive(Debug)]
pub struct ResendRecord {
    pub packet: Packet,
    pub timestamp: Instant,
}

/// A map for tracking packets that may need to be resent.
#[derive(Debug)]
pub struct ResendMap {
    pub unacknowledged: HashMap<u32, ResendRecord>,
    delays: HashMap<Instant, Duration>,
}

impl ResendMap {
    /// Creates a new resend map.
    pub fn new() -> Self {
        Self {
            unacknowledged: HashMap::new(),
            delays: HashMap::new(),
        }
    }

    /// Adds a packet to be tracked for potential retransmission.
    pub fn add(&mut self, index: Uint24, packet: Packet) {
        self.unacknowledged.insert(
            index.value(),
            ResendRecord {
                packet,
                timestamp: Instant::now(),
            },
        );
    }

    /// Marks a packet as acknowledged and returns it if found.
    pub fn acknowledge(&mut self, index: Uint24) -> Option<Packet> {
        self.remove(index, 1)
    }

    /// Gets a packet for retransmission and returns it if found.
    pub fn retransmit(&mut self, index: Uint24) -> Option<Packet> {
        self.remove(index, 2)
    }

    /// Removes a packet from tracking and records the delay.
    fn remove(&mut self, index: Uint24, mul: u32) -> Option<Packet> {
        let record = self.unacknowledged.remove(&index.value())?;
        
        let now = Instant::now();
        let delay = now.duration_since(record.timestamp) * mul;
        self.delays.insert(now, delay);
        
        Some(record.packet)
    }

    /// Calculates the average round-trip time based on recent acknowledgements.
    pub fn rtt(&mut self, now: Instant) -> Duration {
        // Remove old records
        self.delays.retain(|t, _| now.duration_since(*t) <= RTT_CALCULATION_WINDOW);

        if self.delays.is_empty() {
            // Default reasonable RTT
            return Duration::from_millis(50);
        }

        let total: Duration = self.delays.values().sum();
        total / self.delays.len() as u32
    }
}

impl Default for ResendMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_acknowledge() {
        let mut map = ResendMap::new();
        
        let packet = Packet::new();
        map.add(Uint24::new(0), packet);
        
        assert!(map.unacknowledged.contains_key(&0));
        
        let ack = map.acknowledge(Uint24::new(0));
        assert!(ack.is_some());
        assert!(!map.unacknowledged.contains_key(&0));
    }

    #[test]
    fn test_rtt_default() {
        let mut map = ResendMap::new();
        let rtt = map.rtt(Instant::now());
        assert_eq!(rtt, Duration::from_millis(50));
    }
}
