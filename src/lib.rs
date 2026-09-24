//! Reads basic image metadata (format, pixel dimensions, and for JPEG a
//! subset of Exif) directly from the byte header of an image, without
//! decoding pixels and without any external dependencies.
//!
//! Currently supported: JPEG, PNG. Both formats keep dimensions in a fixed
//! spot near the start of the file, so this never needs to read more than a
//! few kilobytes even for a multi-megabyte photo.

mod exif;
mod jpeg;
mod png;

pub use exif::{ExifData, GpsCoords};

/// Image container format, as identified by the file's own magic bytes
/// (not by file extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
}

/// The subset of metadata we currently know how to extract.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    /// Exif metadata, when present. Currently only ever `Some` for JPEG;
    /// the PNG header region this crate reads doesn't carry Exif data.
    pub exif: Option<ExifData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    /// The byte stream doesn't start with a magic number we recognize.
    UnknownFormat,
    /// Recognized as a supported format, but the header ended before we
    /// found the data we needed (truncated file, or not actually an image).
    Truncated,
    /// Recognized and complete enough to parse, but the header is
    /// internally inconsistent (e.g. no SOF marker in a JPEG).
    Malformed(&'static str),
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataError::UnknownFormat => write!(f, "unrecognized image format"),
            MetadataError::Truncated => write!(f, "header ended before metadata was found"),
            MetadataError::Malformed(why) => write!(f, "malformed header: {why}"),
        }
    }
}

impl std::error::Error for MetadataError {}

/// Inspects the magic bytes at the start of `data` and dispatches to the
/// matching format-specific parser.
///
/// `data` only needs to contain the header of the image; for both supported
/// formats that means everything up through (for JPEG) the first frame
/// marker, or (for PNG) the first chunk. Passing an entire file is fine too.
pub fn read_info(data: &[u8]) -> Result<ImageInfo, MetadataError> {
    if png::is_png(data) {
        png::read_info(data)
    } else if jpeg::is_jpeg(data) {
        jpeg::read_info(data)
    } else {
        Err(MetadataError::UnknownFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        let data = [0u8, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(read_info(&data), Err(MetadataError::UnknownFormat));
    }

    #[test]
    fn rejects_truncated_signature() {
        let data = [0xFFu8, 0xD8];
        assert_eq!(read_info(&data), Err(MetadataError::Truncated));
    }
}
