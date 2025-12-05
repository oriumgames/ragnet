//! Binary utilities for reading and writing RakNet data types.

use bytes::{Buf, BufMut};

/// A 24-bit unsigned integer, stored as u32 for convenience.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uint24(pub u32);

impl Uint24 {
    /// Creates a new Uint24 from a u32 value.
    /// The value is masked to 24 bits.
    pub fn new(value: u32) -> Self {
        Uint24(value & 0x00FFFFFF)
    }

    /// Increments the value and returns the old value.
    pub fn inc(&mut self) -> Uint24 {
        let old = *self;
        self.0 = (self.0 + 1) & 0x00FFFFFF;
        old
    }

    /// Returns the inner u32 value.
    pub fn value(self) -> u32 {
        self.0
    }
}

impl From<u32> for Uint24 {
    fn from(value: u32) -> Self {
        Uint24::new(value)
    }
}

impl From<Uint24> for u32 {
    fn from(value: Uint24) -> Self {
        value.0
    }
}

impl std::ops::Add for Uint24 {
    type Output = Uint24;

    fn add(self, rhs: Self) -> Self::Output {
        Uint24::new(self.0 + rhs.0)
    }
}

impl std::ops::Add<u32> for Uint24 {
    type Output = Uint24;

    fn add(self, rhs: u32) -> Self::Output {
        Uint24::new(self.0 + rhs)
    }
}

impl std::ops::Sub for Uint24 {
    type Output = Uint24;

    fn sub(self, rhs: Self) -> Self::Output {
        Uint24::new(self.0.wrapping_sub(rhs.0))
    }
}

/// Reads a uint24 (little-endian) from the buffer.
pub fn read_uint24<B: Buf>(buf: &mut B) -> crate::Result<Uint24> {
    if buf.remaining() < 3 {
        return Err(crate::Error::UnexpectedEof);
    }
    let b0 = buf.get_u8() as u32;
    let b1 = buf.get_u8() as u32;
    let b2 = buf.get_u8() as u32;
    Ok(Uint24(b0 | (b1 << 8) | (b2 << 16)))
}

/// Loads a uint24 (little-endian) from a byte slice.
pub fn load_uint24(b: &[u8]) -> Uint24 {
    Uint24((b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16))
}

/// Writes a uint24 (little-endian) to the buffer.
pub fn write_uint24<B: BufMut>(buf: &mut B, v: Uint24) {
    buf.put_u8(v.0 as u8);
    buf.put_u8((v.0 >> 8) as u8);
    buf.put_u8((v.0 >> 16) as u8);
}

/// Writes a u16 (big-endian) to the buffer.
pub fn write_uint16<B: BufMut>(buf: &mut B, v: u16) {
    buf.put_u8((v >> 8) as u8);
    buf.put_u8(v as u8);
}

/// Writes a u32 (big-endian) to the buffer.
pub fn write_uint32<B: BufMut>(buf: &mut B, v: u32) {
    buf.put_u8((v >> 24) as u8);
    buf.put_u8((v >> 16) as u8);
    buf.put_u8((v >> 8) as u8);
    buf.put_u8(v as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uint24_inc() {
        let mut v = Uint24::new(0);
        assert_eq!(v.inc().value(), 0);
        assert_eq!(v.value(), 1);
        assert_eq!(v.inc().value(), 1);
        assert_eq!(v.value(), 2);
    }

    #[test]
    fn test_uint24_overflow() {
        let mut v = Uint24::new(0x00FFFFFF);
        v.inc();
        assert_eq!(v.value(), 0);
    }

    #[test]
    fn test_load_uint24() {
        let bytes = [0x01, 0x02, 0x03];
        let v = load_uint24(&bytes);
        assert_eq!(v.value(), 0x030201);
    }
}
