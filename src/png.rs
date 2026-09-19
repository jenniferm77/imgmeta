//! PNG is much simpler than JPEG here: the file signature is always
//! followed immediately by an IHDR chunk, and IHDR always starts with
//! width and height as 4-byte big-endian integers. No walking required.

use crate::{Format, ImageInfo, MetadataError};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub(crate) fn is_png(data: &[u8]) -> bool {
    data.len() >= SIGNATURE.len() && data[..SIGNATURE.len()] == SIGNATURE
}

pub(crate) fn read_info(data: &[u8]) -> Result<ImageInfo, MetadataError> {
    // Chunk layout: 4-byte length, 4-byte type, `length` bytes of data,
    // 4-byte CRC. IHDR's data is: width(4) height(4) bit_depth(1)
    // color_type(1) compression(1) filter(1) interlace(1) = 13 bytes.
    let chunk_start = SIGNATURE.len();
    if data.len() < chunk_start + 8 + 8 {
        return Err(MetadataError::Truncated);
    }

    let chunk_len = u32::from_be_bytes(data[chunk_start..chunk_start + 4].try_into().unwrap());
    let chunk_type = &data[chunk_start + 4..chunk_start + 8];
    if chunk_type != b"IHDR" {
        return Err(MetadataError::Malformed("first chunk is not IHDR"));
    }
    if chunk_len != 13 {
        return Err(MetadataError::Malformed("IHDR has unexpected length"));
    }

    let ihdr = chunk_start + 8;
    if data.len() < ihdr + 13 {
        return Err(MetadataError::Truncated);
    }
    let width = u32::from_be_bytes(data[ihdr..ihdr + 4].try_into().unwrap());
    let height = u32::from_be_bytes(data[ihdr + 4..ihdr + 8].try_into().unwrap());

    Ok(ImageInfo {
        format: Format::Png,
        width,
        height,
        exif: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]); // depth, color type, etc.
        bytes.extend_from_slice(&[0u8; 4]); // fake CRC, unchecked by us
        bytes
    }

    #[test]
    fn reads_dimensions_from_ihdr() {
        let data = minimal_png(1920, 1080);
        let info = read_info(&data).unwrap();
        assert_eq!(info.format, Format::Png);
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
    }

    #[test]
    fn rejects_missing_ihdr() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IDAT");
        bytes.extend_from_slice(&[0u8; 13]);
        assert!(matches!(read_info(&bytes), Err(MetadataError::Malformed(_))));
    }
}
