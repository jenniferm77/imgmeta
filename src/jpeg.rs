//! JPEG stores dimensions in the "start of frame" (SOFn) marker segment,
//! which can appear anywhere before the compressed scan data starts. We
//! have to walk the marker chain from the top of the file to find it,
//! skipping every other segment (APPn/EXIF, quantization tables, huffman
//! tables, ...) by its declared length.

use crate::{Format, ImageInfo, MetadataError};

pub(crate) fn is_jpeg(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8
}

pub(crate) fn read_info(data: &[u8]) -> Result<ImageInfo, MetadataError> {
    // Markers with no payload: TEM and the eight restart markers. Every
    // other marker is followed by a 2-byte big-endian length that includes
    // the length field itself.
    fn has_no_payload(marker: u8) -> bool {
        marker == 0x01 || (0xD0..=0xD7).contains(&marker)
    }

    // Start-of-frame markers that carry dimensions. 0xC4 (DHT), 0xC8 (JPG,
    // reserved), and 0xCC (DAC) look like they belong in this range but
    // don't carry a frame header, so they're excluded.
    fn is_sof(marker: u8) -> bool {
        matches!(marker,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF)
    }

    let mut pos = 2; // past the SOI marker checked by is_jpeg
    loop {
        if pos + 1 >= data.len() {
            return Err(MetadataError::Truncated);
        }
        if data[pos] != 0xFF {
            return Err(MetadataError::Malformed("expected marker byte"));
        }
        let marker = data[pos + 1];
        pos += 2;

        if marker == 0xD9 {
            // EOI reached without finding a frame header.
            return Err(MetadataError::Malformed("no SOF marker found"));
        }
        if has_no_payload(marker) {
            continue;
        }

        if pos + 1 >= data.len() {
            return Err(MetadataError::Truncated);
        }
        let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        if seg_len < 2 {
            return Err(MetadataError::Malformed("segment length too small"));
        }

        if is_sof(marker) {
            // Layout after the length field: 1 byte sample precision,
            // 2 bytes height, 2 bytes width.
            let header_start = pos + 2;
            if header_start + 5 > data.len() {
                return Err(MetadataError::Truncated);
            }
            let height = u16::from_be_bytes([data[header_start + 1], data[header_start + 2]]);
            let width = u16::from_be_bytes([data[header_start + 3], data[header_start + 4]]);
            return Ok(ImageInfo {
                format: Format::Jpeg,
                width: width as u32,
                height: height as u32,
            });
        }

        // SOS marks the start of entropy-coded scan data; dimensions must
        // appear before it, so if we hit it first the file has no SOF.
        if marker == 0xDA {
            return Err(MetadataError::Malformed("scan data reached before SOF"));
        }

        pos += seg_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8]; // SOI
        // SOF0 segment: length=17, precision=8, height, width, 1 component.
        bytes.extend_from_slice(&[0xFF, 0xC0]);
        bytes.extend_from_slice(&17u16.to_be_bytes());
        bytes.push(8);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.push(1); // one component
        bytes.extend_from_slice(&[1, 0x11, 0]); // id, sampling, quant table
        bytes.extend_from_slice(&[0xFF, 0xD9]); // EOI
        bytes
    }

    #[test]
    fn reads_dimensions_from_sof0() {
        let data = minimal_jpeg(640, 480);
        let info = read_info(&data).unwrap();
        assert_eq!(info.format, Format::Jpeg);
        assert_eq!(info.width, 640);
        assert_eq!(info.height, 480);
    }

    #[test]
    fn skips_app0_segment_first() {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE0]); // APP0
        bytes.extend_from_slice(&16u16.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 14]);
        bytes.extend_from_slice(&minimal_jpeg(100, 50)[2..]);
        let info = read_info(&bytes).unwrap();
        assert_eq!((info.width, info.height), (100, 50));
    }
}
