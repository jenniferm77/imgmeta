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

- JPEG (dimensions from the SOF0/SOF2/... marker)
- PNG (dimensions from the IHDR chunk)

Anything else returns `MetadataError::UnknownFormat`.

## Status

Early. Dimensions and format detection work and are tested. EXIF
orientation/timestamp/GPS extraction, GIF and WebP support, and streaming
input (for reading over a network without buffering the whole header)
are not implemented yet.

## License

MIT, see `LICENSE`.
