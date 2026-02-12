/// PFR Bit Reader - Bit-level reading for PFR1 font data
///
/// Provides bit-level reading operations on a byte slice.
/// PFR1 format requires reading individual bits and multi-bit values
/// that don't align to byte boundaries.

#[derive(Debug)]
pub struct PfrBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buffer: u32,
    bits_left: u32,
    saved_pos: Option<usize>,
    saved_bit_buffer: Option<u32>,
    saved_bits_left: Option<u32>,
}

impl<'a> PfrBitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buffer: 0,
            bits_left: 0,
            saved_pos: None,
            saved_bit_buffer: None,
            saved_bits_left: None,
        }
    }

    pub fn from_offset(data: &'a [u8], offset: usize) -> Self {
        Self {
            data,
            pos: offset,
            bit_buffer: 0,
            bits_left: 0,
            saved_pos: None,
            saved_bit_buffer: None,
            saved_bits_left: None,
        }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos;
        self.bit_buffer = 0;
        self.bits_left = 0;
    }

    pub fn remaining(&self) -> usize {
        if self.pos >= self.data.len() { 0 } else { self.data.len() - self.pos }
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.data.len() && self.bits_left == 0
    }

    pub fn data_slice(&self) -> &'a [u8] {
        self.data
    }

    pub fn save_position(&mut self) {
        self.saved_pos = Some(self.pos);
        self.saved_bit_buffer = Some(self.bit_buffer);
        self.saved_bits_left = Some(self.bits_left);
    }

    pub fn restore_position(&mut self) {
        if let Some(pos) = self.saved_pos {
            self.pos = pos;
            self.bit_buffer = self.saved_bit_buffer.unwrap_or(0);
            self.bits_left = self.saved_bits_left.unwrap_or(0);
        }
    }

    pub fn align_to_byte(&mut self) {
        self.bit_buffer = 0;
        self.bits_left = 0;
    }

    // ========== Byte-level reads ==========

    pub fn read_u8(&mut self) -> u8 {
        self.align_to_byte();
        if self.pos >= self.data.len() {
            return 0;
        }
        let val = self.data[self.pos];
        self.pos += 1;
        val
    }

    pub fn read_i8(&mut self) -> i8 {
        self.read_u8() as i8
    }

    pub fn read_u16(&mut self) -> u16 {
        let hi = self.read_u8() as u16;
        let lo = self.read_u8() as u16;
        (hi << 8) | lo
    }

    pub fn read_i16(&mut self) -> i16 {
        self.read_u16() as i16
    }

    pub fn read_u24(&mut self) -> u32 {
        let b0 = self.read_u8() as u32;
        let b1 = self.read_u8() as u32;
        let b2 = self.read_u8() as u32;
        (b0 << 16) | (b1 << 8) | b2
    }

    pub fn read_i24(&mut self) -> i32 {
        let val = self.read_u24();
        // Sign-extend from 24 bits
        if val & 0x800000 != 0 {
            (val | 0xFF000000) as i32
        } else {
            val as i32
        }
    }

    pub fn read_u32(&mut self) -> u32 {
        let hi = self.read_u16() as u32;
        let lo = self.read_u16() as u32;
        (hi << 16) | lo
    }

    pub fn read_bytes(&mut self, count: usize) -> Vec<u8> {
        self.align_to_byte();
        let end = (self.pos + count).min(self.data.len());
        let bytes = self.data[self.pos..end].to_vec();
        self.pos = end;
        bytes
    }

    pub fn read_string(&mut self, len: usize) -> String {
        let bytes = self.read_bytes(len);
        String::from_utf8_lossy(&bytes).to_string()
    }

    // ========== Bit-level reads ==========

    /// Read N bits as unsigned value (MSB first)
    pub fn read_bits(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }

        let mut result: u32 = 0;
        let mut remaining = count;

        while remaining > 0 {
            if self.bits_left == 0 {
                if self.pos >= self.data.len() {
                    return result;
                }
                self.bit_buffer = self.data[self.pos] as u32;
                self.pos += 1;
                self.bits_left = 8;
            }

            let take = remaining.min(self.bits_left);
            let shift = self.bits_left - take;
            let mask = ((1u32 << take) - 1) << shift;
            let bits = (self.bit_buffer & mask) >> shift;

            result = (result << take) | bits;
            self.bits_left -= take;
            remaining -= take;
        }

        result
    }

    /// Read N bits as signed value (two's complement, MSB first)
    pub fn read_bits_signed(&mut self, count: u32) -> i32 {
        let val = self.read_bits(count);
        // Sign extend
        if count > 0 && val & (1 << (count - 1)) != 0 {
            let mask = !((1u32 << count) - 1);
            (val | mask) as i32
        } else {
            val as i32
        }
    }

    /// Read a single bit
    pub fn read_bit(&mut self) -> bool {
        self.read_bits(1) != 0
    }

    /// Peek at N bits without consuming them
    pub fn peek_bits(&mut self, count: u32) -> u32 {
        let saved_pos = self.pos;
        let saved_buffer = self.bit_buffer;
        let saved_left = self.bits_left;

        let result = self.read_bits(count);

        self.pos = saved_pos;
        self.bit_buffer = saved_buffer;
        self.bits_left = saved_left;

        result
    }

    /// Skip N bytes
    pub fn skip(&mut self, count: usize) {
        self.align_to_byte();
        self.pos += count;
        if self.pos > self.data.len() {
            self.pos = self.data.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_u8() {
        let data = [0x42, 0xFF];
        let mut reader = PfrBitReader::new(&data);
        assert_eq!(reader.read_u8(), 0x42);
        assert_eq!(reader.read_u8(), 0xFF);
    }

    #[test]
    fn test_read_u16() {
        let data = [0x12, 0x34];
        let mut reader = PfrBitReader::new(&data);
        assert_eq!(reader.read_u16(), 0x1234);
    }

    #[test]
    fn test_read_bits() {
        let data = [0b10110100];
        let mut reader = PfrBitReader::new(&data);
        assert_eq!(reader.read_bits(3), 0b101);
        assert_eq!(reader.read_bits(5), 0b10100);
    }

    #[test]
    fn test_read_bits_signed() {
        let data = [0b11110000];
        let mut reader = PfrBitReader::new(&data);
        assert_eq!(reader.read_bits_signed(4), -1); // 0b1111 = -1 in 4-bit signed
    }

    #[test]
    fn test_save_restore() {
        let data = [0x42, 0x43, 0x44];
        let mut reader = PfrBitReader::new(&data);
        reader.read_u8();
        reader.save_position();
        assert_eq!(reader.read_u8(), 0x43);
        reader.restore_position();
        assert_eq!(reader.read_u8(), 0x43);
    }
}
