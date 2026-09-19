//! Exif metadata lives inside a JPEG APP1 segment as a self-contained TIFF
//! file: a byte-order marker, then a chain of IFDs (image file directories)
//! made of fixed 12-byte tag entries. We only care about a couple of tags
//! (orientation, and the two timestamp tags), so this walks just enough of
//! the structure to find them and ignores everything else (thumbnails, GPS
//! IFD, maker notes, ...).

const TAG_ORIENTATION: u16 = 0x0112;
const TAG_DATETIME: u16 = 0x0132;
const TAG_EXIF_IFD_POINTER: u16 = 0x8769;
const TAG_DATETIME_ORIGINAL: u16 = 0x9003;

const TYPE_ASCII: u16 = 2;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;

/// Parsed subset of Exif metadata found in a JPEG APP1 segment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExifData {
    /// Raw orientation value, 1-8 per the Exif spec (1 = normal, 6 = rotated
    /// 90deg CW, etc). Left undecoded so callers can apply whichever
    /// interpretation (rotate/flip) they need.
    pub orientation: Option<u16>,
    /// `DateTimeOriginal` (when the picture was taken) if present, falling
    /// back to `DateTime` (when the file was last saved). Formatted exactly
    /// as Exif stores it: `"YYYY:MM:DD HH:MM:SS"`.
    pub timestamp: Option<String>,
}

struct Reader<'a> {
    data: &'a [u8],
    little_endian: bool,
}

impl Reader<'_> {
    fn u16_at(&self, offset: usize) -> Option<u16> {
        let bytes = self.data.get(offset..offset + 2)?;
        Some(if self.little_endian {
            u16::from_le_bytes(bytes.try_into().unwrap())
        } else {
            u16::from_be_bytes(bytes.try_into().unwrap())
        })
    }

    fn u32_at(&self, offset: usize) -> Option<u32> {
        let bytes = self.data.get(offset..offset + 4)?;
        Some(if self.little_endian {
            u32::from_le_bytes(bytes.try_into().unwrap())
        } else {
            u32::from_be_bytes(bytes.try_into().unwrap())
        })
    }
}

struct IfdEntry {
    tag: u16,
    value_type: u16,
    count: u32,
    /// Offset (into the TIFF data) of the entry's 4-byte value field. For
    /// values that fit in 4 bytes this holds the value itself; otherwise it
    /// holds an offset to where the value actually lives.
    value_field: usize,
}

impl IfdEntry {
    fn as_short(&self, reader: &Reader) -> Option<u16> {
        if self.value_type != TYPE_SHORT || self.count < 1 {
            return None;
        }
        reader.u16_at(self.value_field)
    }

    fn as_long(&self, reader: &Reader) -> Option<u32> {
        if self.value_type != TYPE_LONG || self.count < 1 {
            return None;
        }
        reader.u32_at(self.value_field)
    }

    fn as_ascii(&self, reader: &Reader) -> Option<String> {
        if self.value_type != TYPE_ASCII || self.count == 0 {
            return None;
        }
        let len = self.count as usize;
        let data_offset = if len <= 4 {
            self.value_field
        } else {
            reader.u32_at(self.value_field)? as usize
        };
        let bytes = reader.data.get(data_offset..data_offset + len)?;
        // Exif ASCII values are NUL-terminated; trust the terminator over
        // `count` in case a writer padded the field.
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        std::str::from_utf8(&bytes[..end]).ok().map(str::to_string)
    }
}

fn read_ifd(reader: &Reader, offset: usize) -> Option<Vec<IfdEntry>> {
    let count = reader.u16_at(offset)? as usize;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let entry_offset = offset + 2 + i * 12;
        entries.push(IfdEntry {
            tag: reader.u16_at(entry_offset)?,
            value_type: reader.u16_at(entry_offset + 2)?,
            count: reader.u32_at(entry_offset + 4)?,
            value_field: entry_offset + 8,
        });
    }
    Some(entries)
}

/// `tiff_data` is the APP1 segment's payload with the 6-byte `"Exif\0\0"`
/// prefix already stripped, so it starts at the TIFF byte-order marker.
///
/// Returns `None` for anything malformed or lacking both tags we look for;
/// callers treat that the same as "no Exif metadata" rather than an error,
/// since a broken thumbnail IFD or maker note elsewhere in the block
/// shouldn't stop us from reporting the pixel dimensions we came for.
pub(crate) fn parse(tiff_data: &[u8]) -> Option<ExifData> {
    if tiff_data.len() < 8 {
        return None;
    }
    let little_endian = match &tiff_data[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let reader = Reader { data: tiff_data, little_endian };
    if reader.u16_at(2)? != 42 {
        return None;
    }
    let ifd0_offset = reader.u32_at(4)? as usize;
    let ifd0 = read_ifd(&reader, ifd0_offset)?;

    let mut result = ExifData::default();
    let mut exif_sub_ifd_offset = None;
    for entry in &ifd0 {
        match entry.tag {
            TAG_ORIENTATION => result.orientation = entry.as_short(&reader),
            TAG_DATETIME => result.timestamp = entry.as_ascii(&reader),
            TAG_EXIF_IFD_POINTER => exif_sub_ifd_offset = entry.as_long(&reader),
            _ => {}
        }
    }

    if let Some(offset) = exif_sub_ifd_offset {
        if let Some(sub_ifd) = read_ifd(&reader, offset as usize) {
            for entry in &sub_ifd {
                if entry.tag == TAG_DATETIME_ORIGINAL {
                    // Prefer the capture time over the file's save time.
                    if let Some(original) = entry.as_ascii(&reader) {
                        result.timestamp = Some(original);
                    }
                }
            }
        }
    }

    if result.orientation.is_none() && result.timestamp.is_none() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_or_bad_input() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(b"not tiff"), None);
        assert_eq!(parse(&[0u8; 4]), None);
    }

    #[test]
    fn parses_orientation_only() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"II");
        buf.extend_from_slice(&42u16.to_le_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset

        buf.extend_from_slice(&1u16.to_le_bytes()); // one entry
        buf.extend_from_slice(&TAG_ORIENTATION.to_le_bytes());
        buf.extend_from_slice(&TYPE_SHORT.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&[6, 0, 0, 0]); // value 6, inline
        buf.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

        let result = parse(&buf).unwrap();
        assert_eq!(result.orientation, Some(6));
        assert_eq!(result.timestamp, None);
    }

    #[test]
    fn prefers_datetime_original_from_sub_ifd_over_datetime() {
        let header_len = 8usize;
        let ifd0_len = 2 + 12 * 2 + 4; // count + 2 entries + next-IFD offset
        let sub_ifd_offset = header_len + ifd0_len;
        let sub_ifd_len = 2 + 12 * 1 + 4;
        let string_offset = sub_ifd_offset + sub_ifd_len;
        let datetime = b"2021:05:04 10:20:30\0";
        assert_eq!(datetime.len(), 20);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"MM");
        buf.extend_from_slice(&42u16.to_be_bytes());
        buf.extend_from_slice(&(header_len as u32).to_be_bytes());

        // IFD0: DateTime (inline, short so it's overridden) + Exif pointer.
        buf.extend_from_slice(&2u16.to_be_bytes());
        buf.extend_from_slice(&TAG_DATETIME.to_be_bytes());
        buf.extend_from_slice(&TYPE_ASCII.to_be_bytes());
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(b"197\0"); // 4 bytes, fits inline
        buf.extend_from_slice(&TAG_EXIF_IFD_POINTER.to_be_bytes());
        buf.extend_from_slice(&TYPE_LONG.to_be_bytes());
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&(sub_ifd_offset as u32).to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(buf.len(), header_len + ifd0_len);

        // Sub IFD: DateTimeOriginal, stored out-of-line since it's >4 bytes.
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(&TAG_DATETIME_ORIGINAL.to_be_bytes());
        buf.extend_from_slice(&TYPE_ASCII.to_be_bytes());
        buf.extend_from_slice(&(datetime.len() as u32).to_be_bytes());
        buf.extend_from_slice(&(string_offset as u32).to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(buf.len(), sub_ifd_offset + sub_ifd_len);

        buf.extend_from_slice(datetime);

        let result = parse(&buf).unwrap();
        assert_eq!(result.timestamp.as_deref(), Some("2021:05:04 10:20:30"));
    }

    #[test]
    fn truncated_ifd_yields_none_instead_of_panicking() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"II");
        buf.extend_from_slice(&42u16.to_le_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes());
        buf.extend_from_slice(&5u16.to_le_bytes()); // claims 5 entries, none follow
        assert_eq!(parse(&buf), None);
    }
}
