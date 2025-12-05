//! Packet encapsulation for RakNet datagrams.

use bytes::{BufMut};

use crate::binary::{load_uint24, write_uint24, write_uint16, write_uint32, Uint24};
use crate::error::{Error, Result};

/// Bit flag set for every valid datagram.
pub const BIT_FLAG_DATAGRAM: u8 = 0x80;

/// Bit flag set for every ACK packet.
pub const BIT_FLAG_ACK: u8 = 0x40;

/// Bit flag set for every NACK packet.
pub const BIT_FLAG_NACK: u8 = 0x20;

/// Bit flag set for every datagram with packet data.
pub const BIT_FLAG_NEEDS_B_AND_AS: u8 = 0x04;

/// Reliability: packet could arrive out of order, be duplicated, or not arrive at all.
pub const RELIABILITY_UNRELIABLE: u8 = 0;

/// Reliability: packet could be duplicated or not arrive, but ensures right order.
pub const RELIABILITY_UNRELIABLE_SEQUENCED: u8 = 1;

/// Reliability: packet could not arrive or arrive out of order, but not duplicated.
pub const RELIABILITY_RELIABLE: u8 = 2;

/// Reliability: every packet arrives, in right order, not duplicated.
pub const RELIABILITY_RELIABLE_ORDERED: u8 = 3;

/// Reliability: packet could not arrive, but ensures right order and not duplicated.
pub const RELIABILITY_RELIABLE_SEQUENCED: u8 = 4;

/// Split flag in packet header.
const SPLIT_FLAG: u8 = 0x10;

/// Additional size for packet headers.
/// Datagram header + Datagram sequence number + Packet header +
/// Packet content length + Packet message index + Packet order index + Packet order channel
pub const PACKET_ADDITIONAL_SIZE: u16 = 1 + 3 + 1 + 2 + 3 + 3 + 1;

/// Additional size for split packets.
/// Packet split count + Packet split ID + Packet split index
pub const SPLIT_ADDITIONAL_SIZE: u16 = 4 + 2 + 4;

/// A packet encapsulation around data sent in RakNet.
#[derive(Debug, Clone)]
pub struct Packet {
    pub reliability: u8,
    pub message_index: Uint24,
    pub sequence_index: Uint24,
    pub order_index: Uint24,
    pub content: Vec<u8>,
    pub split: bool,
    pub split_count: u32,
    pub split_index: u32,
    pub split_id: u16,
}

impl Default for Packet {
    fn default() -> Self {
        Self {
            reliability: RELIABILITY_RELIABLE_ORDERED,
            message_index: Uint24::default(),
            sequence_index: Uint24::default(),
            order_index: Uint24::default(),
            content: Vec::new(),
            split: false,
            split_count: 0,
            split_index: 0,
            split_id: 0,
        }
    }
}

impl Packet {
    /// Creates a new packet with default reliability (ReliableOrdered).
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes the packet to the buffer.
    pub fn write<B: BufMut>(&self, buf: &mut B) {
        let mut header = self.reliability << 5;
        if self.split {
            header |= SPLIT_FLAG;
        }

        buf.put_u8(header);
        write_uint16(buf, (self.content.len() as u16) << 3);

        if self.is_reliable() {
            write_uint24(buf, self.message_index);
        }
        if self.is_sequenced() {
            write_uint24(buf, self.sequence_index);
        }
        if self.is_sequenced_or_ordered() {
            write_uint24(buf, self.order_index);
            buf.put_u8(0); // Order channel
        }
        if self.split {
            write_uint32(buf, self.split_count);
            write_uint16(buf, self.split_id);
            write_uint32(buf, self.split_index);
        }
        buf.put_slice(&self.content);
    }

    /// Reads a packet from the byte slice and returns the number of bytes consumed.
    pub fn read(&mut self, b: &[u8]) -> Result<usize> {
        if b.len() < 3 {
            return Err(Error::UnexpectedEof);
        }

        let header = b[0];
        self.split = (header & SPLIT_FLAG) != 0;
        self.reliability = (header & 224) >> 5;

        let n = (u16::from_be_bytes([b[1], b[2]]) >> 3) as usize;
        if n == 0 {
            return Err(Error::InvalidPacketLength("cannot be 0".to_string()));
        }

        let mut offset = 3;

        if self.is_reliable() {
            if b.len() - offset < 3 {
                return Err(Error::UnexpectedEof);
            }
            self.message_index = load_uint24(&b[offset..]);
            offset += 3;
        }

        if self.is_sequenced() {
            if b.len() - offset < 3 {
                return Err(Error::UnexpectedEof);
            }
            self.sequence_index = load_uint24(&b[offset..]);
            offset += 3;
        }

        if self.is_sequenced_or_ordered() {
            if b.len() - offset < 4 {
                return Err(Error::UnexpectedEof);
            }
            self.order_index = load_uint24(&b[offset..]);
            // Order channel (1 byte)
            offset += 4;
        }

        if self.split {
            if b.len() - offset < 10 {
                return Err(Error::UnexpectedEof);
            }
            self.split_count = u32::from_be_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]]);
            self.split_id = u16::from_be_bytes([b[offset + 4], b[offset + 5]]);
            self.split_index = u32::from_be_bytes([b[offset + 6], b[offset + 7], b[offset + 8], b[offset + 9]]);
            offset += 10;
        }

        if b.len() - offset < n {
            return Err(Error::UnexpectedEof);
        }

        self.content = b[offset..offset + n].to_vec();
        Ok(offset + n)
    }

    /// Returns true if the packet reliability includes reliable delivery.
    pub fn is_reliable(&self) -> bool {
        matches!(
            self.reliability,
            RELIABILITY_RELIABLE | RELIABILITY_RELIABLE_ORDERED | RELIABILITY_RELIABLE_SEQUENCED
        )
    }

    /// Returns true if the packet reliability includes sequencing or ordering.
    pub fn is_sequenced_or_ordered(&self) -> bool {
        matches!(
            self.reliability,
            RELIABILITY_UNRELIABLE_SEQUENCED | RELIABILITY_RELIABLE_ORDERED | RELIABILITY_RELIABLE_SEQUENCED
        )
    }

    /// Returns true if the packet reliability includes sequencing.
    pub fn is_sequenced(&self) -> bool {
        matches!(
            self.reliability,
            RELIABILITY_UNRELIABLE_SEQUENCED | RELIABILITY_RELIABLE_SEQUENCED
        )
    }

    /// Resets the packet for reuse.
    pub fn reset(&mut self) {
        self.reliability = RELIABILITY_RELIABLE_ORDERED;
        self.message_index = Uint24::default();
        self.sequence_index = Uint24::default();
        self.order_index = Uint24::default();
        self.content.clear();
        self.split = false;
        self.split_count = 0;
        self.split_index = 0;
        self.split_id = 0;
    }
}

/// Splits a content buffer into smaller buffers that don't exceed the MTU size.
pub fn split(b: &[u8], mtu: u16) -> Vec<&[u8]> {
    let n = b.len();
    let mut max_size = (mtu - PACKET_ADDITIONAL_SIZE) as usize;

    if n > max_size {
        // If the content size is bigger than the maximum size, the packet will
        // get split, using additional bytes for split metadata.
        max_size = (mtu - PACKET_ADDITIONAL_SIZE - SPLIT_ADDITIONAL_SIZE) as usize;
    }

    // Calculate the number of fragments needed
    let fragment_count = n.div_ceil(max_size);
    let mut fragments = Vec::with_capacity(fragment_count);

    let mut remaining = b;
    for _ in 0..fragment_count - 1 {
        fragments.push(&remaining[..max_size]);
        remaining = &remaining[max_size..];
    }
    fragments.push(remaining);

    fragments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_small_packet() {
        let data = vec![0u8; 100];
        let fragments = split(&data, 1492);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].len(), 100);
    }

    #[test]
    fn test_split_large_packet() {
        let data = vec![0u8; 3000];
        let fragments = split(&data, 1492);
        assert!(fragments.len() > 1);
    }
}
