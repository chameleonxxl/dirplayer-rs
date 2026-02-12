/// PFR Header + Logical Font Directory Parser

use super::bit_reader::PfrBitReader;
use super::types::{PfrHeader, LogicalFontRecord};

/// Parse the PFR header from raw data
pub fn parse_pfr_header(data: &[u8]) -> Result<PfrHeader, String> {
    if data.len() < 58 {
        return Err(format!("PFR data too small for header ({} bytes, need >= 58)", data.len()));
    }

    let mut reader = PfrBitReader::new(data);

    // Read magic: "PFR1"(4 bytes)
    let magic = reader.read_bytes(4);
    let magic_str = String::from_utf8_lossy(&magic);
    if magic_str != "PFR1" {
        return Err(format!("Invalid PFR magic: '{}'", magic_str));
    }

    let is_pfr1 = magic_str == "PFR1";

    let mut header = PfrHeader::new();
    header.version = if is_pfr1 { 1 } else { 0 };

    header.signature = reader.read_u16() as u32;
    header.header_sig2 = reader.read_u16();
    header.header_size = reader.read_u16();
    header.log_font_dir_size = reader.read_u16() as u32;
    header.log_font_dir_offset = reader.read_u16() as u32;
    header.log_font_max_size = reader.read_u16();
    header.log_font_section_size = reader.read_u24();
    header.log_font_section_offset = reader.read_u24();
    header.phys_font_max_size = reader.read_u16();
    header.phys_font_section_size = reader.read_u24();
    header.phys_font_section_offset = reader.read_u24();
    header.gps_max_size = reader.read_u16();
    header.gps_section_size = reader.read_u24();
    header.gps_section_offset = reader.read_u24();
    header.max_blue_values = reader.read_u8();
    header.max_x_orus = reader.read_u8();
    header.max_y_orus = reader.read_u8();
    header.phys_font_max_size_high = reader.read_u8();

    let flags_byte = reader.read_u8();
    header.pfr_invert_bitmap = (flags_byte & 0x02) != 0;
    header.pfr_black_pixel = (flags_byte & 0x01) != 0;
    header.flags = flags_byte;

    let _bct_max_size = reader.read_u24();
    let _bct_set_max_size = reader.read_u24();
    let _pft_bct_set_max_size = reader.read_u24();

    header.n_phys_fonts = reader.read_u16();

    let _max_stem_snap_v = reader.read_u8();
    let _max_stem_snap_h = reader.read_u8();

    header.max_chars = reader.read_u16();

    Ok(header)
}

/// Parse the logical font directory
pub fn parse_logical_font_directory(data: &[u8], header: &PfrHeader) -> Result<Vec<LogicalFontRecord>, String> {
    let mut logical_fonts = Vec::new();

    if header.log_font_dir_offset == 0 || header.log_font_dir_size == 0 {
        return Ok(logical_fonts);
    }

    let is_pfr1 = header.version == 1;

    // PFR1 with small LogFontDir: read directly from LogFontSection
    if is_pfr1 && header.log_font_dir_size < 14 {
        let section_offset = header.log_font_section_offset as usize;
        let section_size = header.log_font_section_size as usize;

        if section_size >= 18 && section_offset > 0 && section_offset < data.len() {
            let mut reader = PfrBitReader::from_offset(data, section_offset);

            // Read font matrix: 4 x 24-bit two's complement values
            let mut font_matrix = [0i32; 4];
            for j in 0..4 {
                font_matrix[j] = reader.read_i24();
            }

            // Read flags byte (8 individual bits)
            let _zero_bit = reader.read_bit();
            let extra_items_present = reader.read_bit();
            let two_byte_bold_thickness = reader.read_bit();
            let bold_flag = reader.read_bit();
            let two_byte_stroke_thickness = reader.read_bit();
            let stroke_flag = reader.read_bit();
            let line_join_type = reader.read_bits(2);

            // Skip stroke/bold data based on flags
            if stroke_flag {
                if two_byte_stroke_thickness {
                    reader.read_bits(16);
                } else {
                    reader.read_bits(8);
                }
                if line_join_type == 0 { // MITER_LINE_JOIN
                    reader.read_bits(24);
                }
            } else if bold_flag {
                if two_byte_bold_thickness {
                    reader.read_bits(16);
                } else {
                    reader.read_bits(8);
                }
            }

            // Skip extra items if present
            if extra_items_present {
                let n_extra_items = reader.read_bits(8);
                for _ in 0..n_extra_items {
                    let extra_item_size = reader.read_bits(8);
                    let _extra_item_type = reader.read_bits(8);
                    for _ in 0..extra_item_size {
                        reader.read_bits(8);
                    }
                }
            }

            // Read physFontSize (u16) and physFontOffset (u24)
            let phys_font_size = reader.read_bits(16) as u32;
            let phys_font_offset = reader.read_bits(24) as u32;

            let mut phys_font_size_increment: u32 = 0;
            if header.phys_font_max_size_high != 0 {
                phys_font_size_increment = reader.read_bits(8) as u32;
            }

            let full_phys_font_size = phys_font_size + phys_font_size_increment * 65536;

            logical_fonts.push(LogicalFontRecord {
                font_matrix,
                size: full_phys_font_size,
                offset: phys_font_offset,
                style_flags: 0,
            });
        }

        return Ok(logical_fonts);
    }

    // large LogFontDir: read from LogFontDirOffset
    let dir_offset = header.log_font_dir_offset as usize;
    if dir_offset >= data.len() {
        return Ok(logical_fonts);
    }

    let mut reader = PfrBitReader::from_offset(data, dir_offset);
    let n_log_fonts = reader.read_u16() as usize;

    for _ in 0..n_log_fonts.min(16) {
        let mut font_matrix = [0i32; 4];
        for j in 0..4 {
            font_matrix[j] = reader.read_i24();
        }

        // Read flags byte (8 individual bits)
        let _zero_bit = reader.read_bit();
        let extra_items_present = reader.read_bit();
        let two_byte_bold_thickness = reader.read_bit();
        let bold_flag = reader.read_bit();
        let two_byte_stroke_thickness = reader.read_bit();
        let stroke_flag = reader.read_bit();
        let line_join_type = reader.read_bits(2);

        if stroke_flag {
            if two_byte_stroke_thickness {
                reader.read_bits(16);
            } else {
                reader.read_bits(8);
            }
            if line_join_type == 0 {
                reader.read_bits(24);
            }
        } else if bold_flag {
            if two_byte_bold_thickness {
                reader.read_bits(16);
            } else {
                reader.read_bits(8);
            }
        }

        if extra_items_present {
            let n_extra_items = reader.read_bits(8);
            for _ in 0..n_extra_items {
                let extra_item_size = reader.read_bits(8);
                let _extra_item_type = reader.read_bits(8);
                for _ in 0..extra_item_size {
                    reader.read_bits(8);
                }
            }
        }

        let phys_font_size = reader.read_bits(16) as u32;
        let phys_font_offset = reader.read_bits(24) as u32;

        let mut phys_font_size_increment: u32 = 0;
        if header.phys_font_max_size_high != 0 {
            phys_font_size_increment = reader.read_bits(8) as u32;
        }

        let full_phys_font_size = phys_font_size + phys_font_size_increment * 65536;

        logical_fonts.push(LogicalFontRecord {
            font_matrix,
            size: full_phys_font_size,
            offset: phys_font_offset,
            style_flags: 0,
        });
    }

    Ok(logical_fonts)
}
