//! Image decoders for untrusted bytes
//!
//! PNG, JPEG, GIF and WebP go through widely deployed, continuously fuzzed
//! decoders (`png`, `zune-jpeg` with SIMD IDCT and color conversion, `gif`,
//! `image-webp`), with the image size checked before any pixel buffer is
//! allocated, so a small file cannot claim gigabytes. AVIF has its own
//! decoder here (`avif`, with SIMD helpers in `simd`), not yet hardened
//! for page bytes.

pub mod avif;
mod simd;

pub use avif::{is_avif, AvifDecoder, AvifError, AvifImage};

use std::io::Cursor;
use std::num::NonZeroU64;

use super::{DecodedImage, ImageFormat};

/// The largest image decoded (pixels): 16384 × 16384, as browsers cap
/// image sizes
pub const MAX_PIXELS: u64 = 1 << 28;

/// Unified decoder error
#[derive(Debug, Clone)]
pub enum DecodeError {
    Png(String),
    Jpeg(String),
    Gif(String),
    Webp(String),
    Avif(AvifError),
    /// The image is larger than `MAX_PIXELS` (or has no pixels)
    TooLarge { width: u64, height: u64 },
    UnsupportedFormat,
    InvalidData,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(e) => write!(f, "PNG: {e}"),
            Self::Jpeg(e) => write!(f, "JPEG: {e}"),
            Self::Gif(e) => write!(f, "GIF: {e}"),
            Self::Webp(e) => write!(f, "WebP: {e}"),
            Self::Avif(e) => write!(f, "AVIF: {e}"),
            Self::TooLarge { width, height } => write!(f, "Image size {width}x{height} is not supported"),
            Self::UnsupportedFormat => write!(f, "Unsupported format"),
            Self::InvalidData => write!(f, "Invalid image data"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<AvifError> for DecodeError {
    fn from(e: AvifError) -> Self {
        Self::Avif(e)
    }
}

/// Reject sizes past `MAX_PIXELS` before allocating for them
fn check_size(width: u64, height: u64) -> Result<(), DecodeError> {
    if width == 0 || height == 0 || width.saturating_mul(height) > MAX_PIXELS {
        return Err(DecodeError::TooLarge { width, height });
    }
    Ok(())
}

/// Decode image bytes to straight (unpremultiplied) RGBA pixels
pub fn decode(data: &[u8]) -> Result<DecodedImage, DecodeError> {
    let format = ImageFormat::from_bytes(data);
    decode_format(data, format)
}

/// Decode with known format
pub fn decode_format(data: &[u8], format: ImageFormat) -> Result<DecodedImage, DecodeError> {
    let (pixels, width, height) = match format {
        ImageFormat::Png => decode_png(data)?,
        ImageFormat::Jpeg => decode_jpeg(data)?,
        ImageFormat::Gif => decode_gif(data)?,
        ImageFormat::WebP => decode_webp(data)?,
        // The AV1 decoder has not been fuzzed: page bytes stay away from it
        ImageFormat::Avif | ImageFormat::Unknown => return Err(DecodeError::UnsupportedFormat),
    };
    Ok(DecodedImage { pixels, width, height, format })
}

fn decode_png(data: &[u8]) -> Result<(Vec<u8>, u32, u32), DecodeError> {
    let err = |e: png::DecodingError| DecodeError::Png(e.to_string());
    let limits = png::Limits { bytes: (MAX_PIXELS * 4) as usize };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(data), limits);
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(err)?;
    let (width, height) = (reader.info().width, reader.info().height);
    check_size(width as u64, height as u64)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(err)?;
    buf.truncate(frame.buffer_size());
    let pixels = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(DecodeError::InvalidData),
    };
    Ok((pixels, width, height))
}

fn decode_jpeg(data: &[u8]) -> Result<(Vec<u8>, u32, u32), DecodeError> {
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let err = |e: zune_jpeg::errors::DecodeErrors| DecodeError::Jpeg(format!("{e:?}"));
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA).set_max_width(65535).set_max_height(65535);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().map_err(err)?;
    let info = decoder.info().ok_or(DecodeError::InvalidData)?;
    check_size(info.width as u64, info.height as u64)?;
    // Only YCbCr and grayscale convert to RGBA reliably (zune-jpeg 0.5
    // converts RGB not at all and CMYK with the wrong stride): decode the
    // others to RGB and add alpha after
    let rgb = !matches!(decoder.input_colorspace(), Some(ColorSpace::YCbCr | ColorSpace::Luma));
    if rgb {
        decoder.set_options(options.jpeg_set_out_colorspace(ColorSpace::RGB));
    }
    let pixels = decoder.decode().map_err(err)?;
    let (width, height) = decoder.dimensions().ok_or(DecodeError::InvalidData)?;
    let pixels = if rgb { pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect() } else { pixels };
    if pixels.len() != width * height * 4 {
        return Err(DecodeError::InvalidData);
    }
    Ok((pixels, width as u32, height as u32))
}

/// The first frame, placed on the GIF's logical screen
fn decode_gif(data: &[u8]) -> Result<(Vec<u8>, u32, u32), DecodeError> {
    let err = |e: gif::DecodingError| DecodeError::Gif(e.to_string());
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(NonZeroU64::new(MAX_PIXELS * 4).unwrap()));
    let mut decoder = options.read_info(Cursor::new(data)).map_err(err)?;
    let (width, height) = (decoder.width() as u32, decoder.height() as u32);
    check_size(width as u64, height as u64)?;
    let frame = decoder.read_next_frame().map_err(err)?.ok_or(DecodeError::InvalidData)?;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let (fw, fh) = (frame.width as usize, frame.height as usize);
    let (left, top) = (frame.left as usize, frame.top as usize);
    for row in 0..fh {
        let y = top + row;
        if y >= height as usize {
            break;
        }
        let visible = fw.min((width as usize).saturating_sub(left));
        if visible == 0 {
            break;
        }
        let src = &frame.buffer[row * fw * 4..][..visible * 4];
        let at = (y * width as usize + left) * 4;
        pixels[at..at + visible * 4].copy_from_slice(src);
    }
    Ok((pixels, width, height))
}

fn decode_webp(data: &[u8]) -> Result<(Vec<u8>, u32, u32), DecodeError> {
    let err = |e: image_webp::DecodingError| DecodeError::Webp(e.to_string());
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(data)).map_err(err)?;
    let (width, height) = decoder.dimensions();
    check_size(width as u64, height as u64)?;
    decoder.set_memory_limit((MAX_PIXELS * 4) as usize);
    let size = decoder.output_buffer_size().ok_or(DecodeError::InvalidData)?;
    let mut buf = vec![0; size];
    decoder.read_image(&mut buf).map_err(err)?;
    let pixels = if decoder.has_alpha() { buf } else { buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect() };
    Ok((pixels, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(color);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn png_color_types_become_rgba() {
        let rgb = decode(&png_bytes(2, 1, png::ColorType::Rgb, &[255, 0, 0, 0, 255, 0])).unwrap();
        assert_eq!((rgb.width, rgb.height, rgb.pixels.as_slice()), (2, 1, &[255, 0, 0, 255, 0, 255, 0, 255][..]));
        let gray = decode(&png_bytes(1, 1, png::ColorType::GrayscaleAlpha, &[9, 128])).unwrap();
        assert_eq!(gray.pixels, [9, 9, 9, 128]);
        assert!(matches!(decode(b"\x89PNG\r\n\x1a\nnot really"), Err(DecodeError::Png(_))));
    }

    #[test]
    fn oversized_and_unknown_images_are_rejected() {
        // A 1x1 GIF frame on a 65535 x 65535 screen: refused before the
        // screen's 17 GB are allocated
        let gif = |w: u8, h: u8| {
            let mut g = b"GIF89a".to_vec();
            g.extend_from_slice(&[w, w, h, h, 0, 0, 0]);
            g.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0x80, 255, 0, 0, 0, 0, 255, 2, 2, 0x44, 0x01, 0, 0x3b]);
            g
        };
        assert!(matches!(decode(&gif(0xff, 0xff)), Err(DecodeError::TooLarge { .. })));
        let small = decode(&gif(1, 1)).unwrap();
        assert_eq!((small.width, small.height), (257, 257));
        assert_eq!(&small.pixels[..4], &[255, 0, 0, 255]);
        assert!(matches!(decode(b"definitely not an image"), Err(DecodeError::UnsupportedFormat)));
        assert!(matches!(decode(&[0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0]), Err(DecodeError::Jpeg(_))));
    }
}
