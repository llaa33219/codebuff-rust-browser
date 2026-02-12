//! TrueType / OpenType table parsing.
//!
//! Parses the sfnt table directory and individual tables: `head`, `hhea`, `maxp`,
//! `cmap` (format 4), `loca`, `hmtx`, `kern`, `name`, `OS/2`.

use common::{Cursor, Endian, ParseError};

// ─────────────────────────────────────────────────────────────────────────────
// TableTag
// ─────────────────────────────────────────────────────────────────────────────

/// A 4-byte table tag identifying a TrueType/OpenType table.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableTag(pub [u8; 4]);

impl TableTag {
    pub const HEAD: Self = Self(*b"head");
    pub const CMAP: Self = Self(*b"cmap");
    pub const GLYF: Self = Self(*b"glyf");
    pub const LOCA: Self = Self(*b"loca");
    pub const HHEA: Self = Self(*b"hhea");
    pub const HMTX: Self = Self(*b"hmtx");
    pub const MAXP: Self = Self(*b"maxp");
    pub const KERN: Self = Self(*b"kern");
    pub const NAME: Self = Self(*b"name");
    pub const OS2: Self = Self(*b"OS/2");
    pub const POST: Self = Self(*b"post");
}

impl core::fmt::Debug for TableTag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = core::str::from_utf8(&self.0).unwrap_or("????");
        write!(f, "TableTag('{s}')")
    }
}

impl core::fmt::Display for TableTag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = core::str::from_utf8(&self.0).unwrap_or("????");
        write!(f, "{s}")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TableRecord
// ─────────────────────────────────────────────────────────────────────────────

/// A single entry in the sfnt table directory.
#[derive(Clone, Copy, Debug)]
pub struct TableRecord {
    pub tag: TableTag,
    pub checksum: u32,
    pub offset: u32,
    pub length: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// FontFile
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed font file with access to raw table data.
pub struct FontFile<'a> {
    pub data: &'a [u8],
    pub num_tables: u16,
    pub tables: Vec<TableRecord>,
}

impl<'a> FontFile<'a> {
    /// Parse the sfnt table directory from font file data.
    pub fn parse(data: &'a [u8]) -> Result<Self, ParseError> {
        let mut c = Cursor::new(data, Endian::Big);

        let sfnt_version = c.u32()?;
        // Accept TrueType (0x00010000) or OpenType ('OTTO' = 0x4F54544F)
        if sfnt_version != 0x00010000 && sfnt_version != 0x4F54544F {
            return Err(ParseError::InvalidValue("not a TrueType/OpenType font"));
        }

        let num_tables = c.u16()?;
        let _search_range = c.u16()?;
        let _entry_selector = c.u16()?;
        let _range_shift = c.u16()?;

        let mut tables = Vec::with_capacity(num_tables as usize);
        for _ in 0..num_tables {
            let tag_bytes = c.bytes(4)?;
            let tag = TableTag([tag_bytes[0], tag_bytes[1], tag_bytes[2], tag_bytes[3]]);
            let checksum = c.u32()?;
            let offset = c.u32()?;
            let length = c.u32()?;
            tables.push(TableRecord { tag, checksum, offset, length });
        }

        Ok(FontFile { data, num_tables, tables })
    }

    /// Find a table by tag and return its raw data slice.
    pub fn table_data(&self, tag: TableTag) -> Option<&'a [u8]> {
        self.tables.iter()
            .find(|t| t.tag == tag)
            .and_then(|t| {
                let start = t.offset as usize;
                let end = start + t.length as usize;
                self.data.get(start..end)
            })
    }

    /// Find a table record by tag.
    pub fn find_table(&self, tag: TableTag) -> Option<&TableRecord> {
        self.tables.iter().find(|t| t.tag == tag)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HeadTable
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed `head` table.
#[derive(Clone, Debug)]
pub struct HeadTable {
    pub units_per_em: u16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub index_to_loc_format: i16, // 0 = short (u16), 1 = long (u32)
    pub mac_style: u16,
    pub flags: u16,
}

impl HeadTable {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut c = Cursor::new(data, Endian::Big);
        let _major_version = c.u16()?;
        let _minor_version = c.u16()?;
        let _font_revision = c.u32()?; // Fixed
        let _checksum_adjust = c.u32()?;
        let _magic = c.u32()?;
        let flags = c.u16()?;
        let units_per_em = c.u16()?;
        c.skip(16)?; // created + modified (LONGDATETIME × 2)
        let x_min = c.i16()?;
        let y_min = c.i16()?;
        let x_max = c.i16()?;
        let y_max = c.i16()?;
        let mac_style = c.u16()?;
        let _lowest_rec_ppem = c.u16()?;
        let _font_direction_hint = c.i16()?;
        let index_to_loc_format = c.i16()?;
        let _glyph_data_format = c.i16()?;

        Ok(HeadTable {
            units_per_em,
            x_min, y_min, x_max, y_max,
            index_to_loc_format,
            mac_style,
            flags,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HheaTable
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed `hhea` (horizontal header) table.
#[derive(Clone, Debug)]
pub struct HheaTable {
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub num_h_metrics: u16,
}

impl HheaTable {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut c = Cursor::new(data, Endian::Big);
        let _major = c.u16()?;
        let _minor = c.u16()?;
        let ascender = c.i16()?;
        let descender = c.i16()?;
        let line_gap = c.i16()?;
        let advance_width_max = c.u16()?;
        c.skip(22)?; // min/max extents, caret fields, reserved
        let num_h_metrics = c.u16()?;

        Ok(HheaTable {
            ascender, descender, line_gap,
            advance_width_max, num_h_metrics,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MaxpTable
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed `maxp` table.
#[derive(Clone, Debug)]
pub struct MaxpTable {
    pub num_glyphs: u16,
}

impl MaxpTable {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut c = Cursor::new(data, Endian::Big);
        let _version = c.u32()?;
        let num_glyphs = c.u16()?;
        Ok(MaxpTable { num_glyphs })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CmapFormat4
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed `cmap` format 4 subtable (BMP character-to-glyph mapping).
#[derive(Clone, Debug)]
pub struct CmapFormat4 {
    pub seg_count: u16,
    pub end_code: Vec<u16>,
    pub start_code: Vec<u16>,
    pub id_delta: Vec<i16>,
    pub id_range_offset: Vec<u16>,
    pub glyph_id_array: Vec<u16>,
}

impl CmapFormat4 {
    /// Parse a format 4 cmap subtable from its data (starting after the format field).
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut c = Cursor::new(data, Endian::Big);
        let _format = c.u16()?; // should be 4
        let length = c.u16()? as usize;
        let _language = c.u16()?;
        let seg_count_x2 = c.u16()?;
        let seg_count = seg_count_x2 / 2;
        let _search_range = c.u16()?;
        let _entry_selector = c.u16()?;
        let _range_shift = c.u16()?;

        let mut end_code = Vec::with_capacity(seg_count as usize);
        for _ in 0..seg_count {
            end_code.push(c.u16()?);
        }

        let _reserved_pad = c.u16()?;

        let mut start_code = Vec::with_capacity(seg_count as usize);
        for _ in 0..seg_count {
            start_code.push(c.u16()?);
        }

        let mut id_delta = Vec::with_capacity(seg_count as usize);
        for _ in 0..seg_count {
            id_delta.push(c.i16()?);
        }

        // Save position for id_range_offset references
        let id_range_offset_pos = c.position();
        let mut id_range_offset = Vec::with_capacity(seg_count as usize);
        for _ in 0..seg_count {
            id_range_offset.push(c.u16()?);
        }

        // Remaining bytes are the glyph ID array
        let remaining = length.saturating_sub(c.position());
        let glyph_count = remaining / 2;
        let mut glyph_id_array = Vec::with_capacity(glyph_count);
        for _ in 0..glyph_count {
            if c.remaining() >= 2 {
                glyph_id_array.push(c.u16()?);
            }
        }

        let _ = id_range_offset_pos;

        Ok(CmapFormat4 {
            seg_count,
            end_code,
            start_code,
            id_delta,
            id_range_offset,
            glyph_id_array,
        })
    }

    /// Look up a glyph ID for a Unicode codepoint (BMP only).
    pub fn lookup(&self, codepoint: u16) -> u16 {
        // Binary search for the segment
        let mut lo = 0usize;
        let mut hi = self.seg_count as usize;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.end_code[mid] < codepoint {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo >= self.seg_count as usize {
            return 0; // .notdef
        }

        let i = lo;
        if self.start_code[i] > codepoint {
            return 0; // .notdef
        }

        if self.id_range_offset[i] == 0 {
            // Simple case: glyph_id = (codepoint + id_delta) mod 65536
            let gid = (codepoint as i32 + self.id_delta[i] as i32) as u16;
            return gid;
        }

        // Complex case: use id_range_offset to index into glyph_id_array
        let offset = self.id_range_offset[i] as usize;
        let idx = offset / 2 + (codepoint as usize - self.start_code[i] as usize);
        // idx is relative to the id_range_offset entry position, but we need to
        // subtract seg_count-i to get into glyph_id_array
        let array_idx = idx.saturating_sub(self.seg_count as usize - i);

        if array_idx < self.glyph_id_array.len() {
            let gid = self.glyph_id_array[array_idx];
            if gid != 0 {
                return (gid as i32 + self.id_delta[i] as i32) as u16;
            }
        }

        0 // .notdef
    }
}

/// Find and parse the best cmap subtable (prefer format 4 for BMP, platform 3 encoding 1).
pub fn parse_cmap(data: &[u8]) -> Result<CmapFormat4, ParseError> {
    let mut c = Cursor::new(data, Endian::Big);
    let _version = c.u16()?;
    let num_tables = c.u16()?;

    let mut best_offset: Option<u32> = None;

    for _ in 0..num_tables {
        let platform_id = c.u16()?;
        let encoding_id = c.u16()?;
        let offset = c.u32()?;

        // Prefer Microsoft Unicode BMP (3, 1)
        if platform_id == 3 && encoding_id == 1 {
            best_offset = Some(offset);
            break;
        }
        // Accept Unicode (0, 3) as fallback
        if platform_id == 0 && encoding_id == 3 && best_offset.is_none() {
            best_offset = Some(offset);
        }
        // Accept any Unicode platform
        if platform_id == 0 && best_offset.is_none() {
            best_offset = Some(offset);
        }
    }

    let offset = best_offset.ok_or(ParseError::InvalidValue("no suitable cmap subtable found"))? as usize;
    let subtable_data = data.get(offset..).ok_or(ParseError::UnexpectedEof)?;

    // Check format
    let format = u16::from_be_bytes([subtable_data[0], subtable_data[1]]);
    if format != 4 {
        return Err(ParseError::InvalidValue("only cmap format 4 is supported"));
    }

    CmapFormat4::parse(subtable_data)
}

// ─────────────────────────────────────────────────────────────────────────────
// Loca table
// ─────────────────────────────────────────────────────────────────────────────

/// Get the byte offset of a glyph within the `glyf` table.
///
/// `index_to_loc_format`: 0 = short (offsets are u16 × 2), 1 = long (offsets are u32).
pub fn get_glyph_offset(loca_data: &[u8], glyph_id: u16, index_to_loc_format: i16) -> Result<(u32, u32), ParseError> {
    let mut c = Cursor::new(loca_data, Endian::Big);
    let (offset, next_offset) = if index_to_loc_format == 0 {
        // Short format: u16 values, multiply by 2
        c.skip(glyph_id as usize * 2)?;
        let off = c.u16()? as u32 * 2;
        let next = c.u16()? as u32 * 2;
        (off, next)
    } else {
        // Long format: u32 values
        c.skip(glyph_id as usize * 4)?;
        let off = c.u32()?;
        let next = c.u32()?;
        (off, next)
    };

    Ok((offset, next_offset))
}

// ─────────────────────────────────────────────────────────────────────────────
// Hmtx table
// ─────────────────────────────────────────────────────────────────────────────

/// Horizontal metrics for a glyph.
#[derive(Clone, Copy, Debug)]
pub struct HMetric {
    pub advance_width: u16,
    pub left_side_bearing: i16,
}

/// Get horizontal metrics for a glyph.
pub fn get_hmetric(hmtx_data: &[u8], glyph_id: u16, num_h_metrics: u16) -> Result<HMetric, ParseError> {
    let mut c = Cursor::new(hmtx_data, Endian::Big);

    if glyph_id < num_h_metrics {
        c.skip(glyph_id as usize * 4)?;
        let advance_width = c.u16()?;
        let left_side_bearing = c.i16()?;
        Ok(HMetric { advance_width, left_side_bearing })
    } else {
        // Use last advance width, read LSB from extended array
        c.skip((num_h_metrics as usize - 1) * 4)?;
        let advance_width = c.u16()?;
        let _last_lsb = c.i16()?;
        // LSBs for glyphs beyond num_h_metrics
        let extra_index = (glyph_id - num_h_metrics) as usize;
        let lsb_offset = num_h_metrics as usize * 4 + extra_index * 2;
        let mut c2 = Cursor::new(hmtx_data, Endian::Big);
        c2.skip(lsb_offset)?;
        let left_side_bearing = c2.i16()?;
        Ok(HMetric { advance_width, left_side_bearing })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_tag_constants() {
        assert_eq!(TableTag::HEAD.0, *b"head");
        assert_eq!(TableTag::CMAP.0, *b"cmap");
        assert_eq!(TableTag::GLYF.0, *b"glyf");
        assert_eq!(TableTag::LOCA.0, *b"loca");
        assert_eq!(TableTag::HHEA.0, *b"hhea");
        assert_eq!(TableTag::HMTX.0, *b"hmtx");
        assert_eq!(TableTag::MAXP.0, *b"maxp");
    }

    #[test]
    fn table_tag_display() {
        assert_eq!(format!("{}", TableTag::HEAD), "head");
        assert_eq!(format!("{:?}", TableTag::HEAD), "TableTag('head')");
    }

    #[test]
    fn table_tag_equality() {
        assert_eq!(TableTag(*b"head"), TableTag::HEAD);
        assert_ne!(TableTag(*b"cmap"), TableTag::HEAD);
    }

    #[test]
    fn parse_font_file_bad_magic() {
        let data = [0u8; 12];
        let result = FontFile::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_font_file_truetype_header() {
        // Minimal valid sfnt header with 0 tables
        let mut data = Vec::new();
        data.extend_from_slice(&0x00010000u32.to_be_bytes()); // sfVersion
        data.extend_from_slice(&0u16.to_be_bytes()); // numTables = 0
        data.extend_from_slice(&0u16.to_be_bytes()); // searchRange
        data.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
        data.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

        let ff = FontFile::parse(&data).unwrap();
        assert_eq!(ff.num_tables, 0);
        assert!(ff.tables.is_empty());
    }

    #[test]
    fn head_table_parse() {
        // Construct a minimal head table (54 bytes)
        let mut data = vec![0u8; 54];
        // version 1.0
        data[0] = 0; data[1] = 1; data[2] = 0; data[3] = 0;
        // units_per_em at offset 18
        data[18] = 0x03; data[19] = 0xE8; // 1000
        // index_to_loc_format at offset 50
        data[50] = 0; data[51] = 1; // long format

        let head = HeadTable::parse(&data).unwrap();
        assert_eq!(head.units_per_em, 1000);
        assert_eq!(head.index_to_loc_format, 1);
    }

    #[test]
    fn maxp_table_parse() {
        let mut data = vec![0u8; 6];
        data[0] = 0; data[1] = 1; data[2] = 0; data[3] = 0; // version
        data[4] = 0x01; data[5] = 0x00; // 256 glyphs

        let maxp = MaxpTable::parse(&data).unwrap();
        assert_eq!(maxp.num_glyphs, 256);
    }

    #[test]
    fn loca_short_format() {
        // Short format: values are u16, multiply by 2
        let mut data = Vec::new();
        for val in [0u16, 50, 120, 200] {
            data.extend_from_slice(&val.to_be_bytes());
        }

        let (off, next) = get_glyph_offset(&data, 1, 0).unwrap();
        assert_eq!(off, 100);  // 50 * 2
        assert_eq!(next, 240); // 120 * 2
    }

    #[test]
    fn loca_long_format() {
        let mut data = Vec::new();
        for val in [0u32, 100, 240, 400] {
            data.extend_from_slice(&val.to_be_bytes());
        }

        let (off, next) = get_glyph_offset(&data, 1, 1).unwrap();
        assert_eq!(off, 100);
        assert_eq!(next, 240);
    }
}
