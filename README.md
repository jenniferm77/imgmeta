# imgmeta

A Rust library that reads basic metadata (format, pixel width, pixel
height) straight out of an image file's header, without decoding the
image and without pulling in any dependencies.

Most of the time you don't need a full image decoder just to answer "how
big is this picture" or "what format is it". Both JPEG and PNG keep that
information in a small, fixed structure near the start of the file, so
`imgmeta` reads a few dozen to a few thousand bytes and stops — it never
touches pixel data.

## Why

I wanted this for a batch job that sorts a folder of a few hundred
thousand photos by resolution before doing anything else with them.
Pulling in `image` (and its dependency tree of decoders for formats we
don't even use) just to read a width and height felt like the wrong
tradeoff, and the header formats aren't complicated once you write the
marker-walking code once.

## Usage

```rust
use imgmeta::read_info;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read("photo.jpg")?;
    let info = read_info(&data)?;

    println!("{:?}, {}x{}", info.format, info.width, info.height);
    Ok(())
}
```

`read_info` takes a byte slice, not a path, so it works the same way
whether the bytes came from disk, a network response, or memory you
already had. You don't need to pass the whole file — for a JPEG you need
everything up through the first frame header (usually well under the
first few KB unless there's a huge embedded thumbnail ahead of it), and
for a PNG you only need the first 33 bytes.

```rust
use imgmeta::{Format, MetadataError};

match imgmeta::read_info(&data) {
    Ok(info) if info.format == Format::Png => { /* ... */ }
    Ok(info) => { /* JPEG */ }
    Err(MetadataError::UnknownFormat) => eprintln!("not a JPEG or PNG"),
    Err(e) => eprintln!("couldn't read header: {e}"),
}
```

## Supported formats

- JPEG (dimensions from the SOF0/SOF2/... marker; orientation, timestamp,
  and GPS coordinates from the first APP1 Exif segment, if present)
- PNG (dimensions from the IHDR chunk)

Anything else returns `MetadataError::UnknownFormat`.

For JPEG, `ImageInfo::exif` is `Some` when the file has an APP1 segment
starting with the Exif TIFF header and it yields at least an orientation, a
timestamp, or GPS coordinates. `orientation` is the raw 1-8 Exif value (left
undecoded, since turning that into an actual rotation/flip depends on what
the caller is doing with the pixels). `timestamp` prefers `DateTimeOriginal`
(when the photo was taken) and falls back to `DateTime` (when the file was
saved), formatted as Exif stores it, `"YYYY:MM:DD HH:MM:SS"`. `gps`, when
the file's GPS IFD has both a latitude and longitude, gives decimal degrees
(`GpsCoords`), positive north and east, with sign already applied from the
ref tags so you don't need to interpret "N"/"S"/"E"/"W" yourself.

## Status

Early. Dimensions, format detection, and JPEG orientation/timestamp/GPS
work and are tested. GIF and WebP support, and streaming input (for
reading over a network without buffering the whole header) are not
implemented yet.

## License

MIT, see `LICENSE`.
