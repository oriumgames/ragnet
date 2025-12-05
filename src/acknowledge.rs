//! Acknowledgement packets (ACK/NACK) for RakNet reliability.

use crate::binary::{load_uint24, Uint24};
use crate::error::{Error, Result};

/// Indicates a range of packets in acknowledgement.
const PACKET_RANGE: u8 = 0;

/// Indicates a single packet in acknowledgement.
const PACKET_SINGLE: u8 = 1;

/// Maximum number of packets allowed in an acknowledgement.
const MAX_ACKNOWLEDGEMENT_PACKETS: usize = 8192;

/// An acknowledgement packet that may be either ACK or NACK.
#[derive(Debug, Default)]
pub struct Acknowledgement {
    pub packets: Vec<Uint24>,
}

impl Acknowledgement {
    /// Creates a new empty acknowledgement.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an acknowledgement with the given packets.
    pub fn with_packets(packets: Vec<Uint24>) -> Self {
        Self { packets }
    }

    /// Writes the acknowledgement to a Vec and returns (bytes, packets_written).
    pub fn write_to_vec(&self, mtu: u16) -> (Vec<u8>, usize) {
        let mut buf = Vec::with_capacity(mtu as usize);
        let n = self.write_to_buf(&mut buf, mtu);
        (buf, n)
    }

    /// Writes the acknowledgement to an existing buffer and returns packets_written.
    /// The buffer should be cleared before calling this method.
    /// This method appends to the buffer starting from its current position.
    pub fn write_to_buf(&self, buf: &mut Vec<u8>, mtu: u16) -> usize {
        let start_len = buf.len();

        if self.packets.is_empty() {
            buf.push(0);
            buf.push(0);
            return 0;
        }

        let mut packets = self.packets.clone();
        packets.sort();

        let mut records: u16 = 0;
        let mut n = 0;

        // Reserve space for record count
        let record_count_pos = buf.len();
        buf.push(0);
        buf.push(0);

        let mut first_packet_in_range: Uint24;
        let mut last_packet_in_range: Uint24;

        let mut iter = packets.iter();

        if let Some(&first) = iter.next() {
            first_packet_in_range = first;
            last_packet_in_range = first;
            n = 1;

            for &pk in iter {
                if buf.len() - start_len >= (mtu - 7) as usize {
                    break;
                }
                n += 1;

                if pk.value() == last_packet_in_range.value() + 1 {
                    last_packet_in_range = pk;
                    continue;
                }

                Self::write_record_to_vec(
                    buf,
                    first_packet_in_range,
                    last_packet_in_range,
                    &mut records,
                );
                first_packet_in_range = pk;
                last_packet_in_range = pk;
            }

            Self::write_record_to_vec(
                buf,
                first_packet_in_range,
                last_packet_in_range,
                &mut records,
            );
        }

        // Write record count at the reserved position
        buf[record_count_pos] = (records >> 8) as u8;
        buf[record_count_pos + 1] = records as u8;

        n
    }

    fn write_record_to_vec(buf: &mut Vec<u8>, first: Uint24, last: Uint24, count: &mut u16) {
        if first == last {
            buf.push(PACKET_SINGLE);
            buf.push(first.value() as u8);
            buf.push((first.value() >> 8) as u8);
            buf.push((first.value() >> 16) as u8);
        } else {
            buf.push(PACKET_RANGE);
            buf.push(first.value() as u8);
            buf.push((first.value() >> 8) as u8);
            buf.push((first.value() >> 16) as u8);
            buf.push(last.value() as u8);
            buf.push((last.value() >> 8) as u8);
            buf.push((last.value() >> 16) as u8);
        }
        *count += 1;
    }

    /// Reads an acknowledgement from the byte slice.
    pub fn read(&mut self, b: &[u8]) -> Result<()> {
        if b.len() < 2 {
            return Err(Error::UnexpectedEof);
        }

        let record_count = u16::from_be_bytes([b[0], b[1]]);
        let mut offset = 2;

        for _ in 0..record_count {
            if b.len() - offset < 4 {
                return Err(Error::UnexpectedEof);
            }

            match b[offset] {
                PACKET_RANGE => {
                    if b.len() - offset < 7 {
                        return Err(Error::UnexpectedEof);
                    }
                    let start = load_uint24(&b[offset + 1..]);
                    let end = load_uint24(&b[offset + 4..]);

                    if self.packets.len() as u32 + end.value() - start.value()
                        > MAX_ACKNOWLEDGEMENT_PACKETS as u32
                    {
                        return Err(Error::MaxAcknowledgement);
                    }

                    for pk in start.value()..=end.value() {
                        self.packets.push(Uint24::new(pk));
                    }
                    offset += 7;
                }
                PACKET_SINGLE => {
                    if self.packets.len() + 1 > MAX_ACKNOWLEDGEMENT_PACKETS {
                        return Err(Error::MaxAcknowledgement);
                    }
                    self.packets.push(load_uint24(&b[offset + 1..]));
                    offset += 4;
                }
                _ => {
                    // Unknown record type, skip based on expected structure
                    offset += 4;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acknowledgement_single() {
        let ack = Acknowledgement::with_packets(vec![Uint24::new(5)]);
        let (buf, n) = ack.write_to_vec(1492);
        assert_eq!(n, 1);

        let mut read_ack = Acknowledgement::new();
        read_ack.read(&buf).unwrap();
        assert_eq!(read_ack.packets.len(), 1);
        assert_eq!(read_ack.packets[0].value(), 5);
    }

    #[test]
    fn test_acknowledgement_range() {
        let ack =
            Acknowledgement::with_packets(vec![Uint24::new(1), Uint24::new(2), Uint24::new(3)]);
        let (buf, n) = ack.write_to_vec(1492);
        assert_eq!(n, 3);

        let mut read_ack = Acknowledgement::new();
        read_ack.read(&buf).unwrap();
        assert_eq!(read_ack.packets.len(), 3);
    }
}
