//! Custom Image Decoders
//!
//! From-scratch image decoders with SIMD acceleration.
//! Supports PNG, JPEG, GIF, and WebP.

mod simd;
mod deflate;
mod png;
mod jpeg;
mod gif;
mod webp;

pub use simd::SimdOps;
pub use deflate::{Inflate, DeflateError};
pub use png::{PngDecoder, PngError};
pub use jpeg::{JpegDecoder, JpegError};
pub use gif::{GifDecoder, GifError, GifFrame};
pub use webp::{WebpDecoder, WebpError};

use super::{DecodedImage, ImageFormat};

/// Unified decoder error
#[derive(Debug, Clone)]
pub enum DecodeError {
    Png(PngError),
    Jpeg(JpegError),
    Gif(GifError),
    Webp(WebpError),
    UnsupportedFormat,
    InvalidData,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(e) => write!(f, "PNG: {}", e),
            Self::Jpeg(e) => write!(f, "JPEG: {}", e),
            Self::Gif(e) => write!(f, "GIF: {}", e),
            Self::Webp(e) => write!(f, "WebP: {}", e),
            Self::UnsupportedFormat => write!(f, "Unsupported format"),
            Self::InvalidData => write!(f, "Invalid image data"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<PngError> for DecodeError {
    fn from(e: PngError) -> Self { Self::Png(e) }
}

impl From<JpegError> for DecodeError {
    fn from(e: JpegError) -> Self { Self::Jpeg(e) }
}

impl From<GifError> for DecodeError {
    fn from(e: GifError) -> Self { Self::Gif(e) }
}

impl From<WebpError> for DecodeError {
    fn from(e: WebpError) -> Self { Self::Webp(e) }
}

/// Decode image bytes to RGBA pixels
pub fn decode(data: &[u8]) -> Result<DecodedImage, DecodeError> {
    let format = ImageFormat::from_bytes(data);
    decode_format(data, format)
}

/// Decode with known format
pub fn decode_format(data: &[u8], format: ImageFormat) -> Result<DecodedImage, DecodeError> {
    match format {
        ImageFormat::Png => {
            let mut decoder = PngDecoder::new();
            let img = decoder.decode(data)?;
            Ok(DecodedImage {
                pixels: img.pixels,
                width: img.width,
                height: img.height,
                format,
            })
        }
        ImageFormat::Jpeg => {
            let mut decoder = JpegDecoder::new();
            let img = decoder.decode(data)?;
            Ok(DecodedImage {
                pixels: img.pixels,
                width: img.width,
                height: img.height,
                format,
            })
        }
        ImageFormat::Gif => {
            let mut decoder = GifDecoder::new();
            let frames = decoder.decode(data)?;
            if let Some(frame) = frames.first() {
                Ok(DecodedImage {
                    pixels: frame.pixels.clone(),
                    width: frame.width,
                    height: frame.height,
                    format,
                })
            } else {
                Err(DecodeError::InvalidData)
            }
        }
        ImageFormat::WebP => {
            let mut decoder = WebpDecoder::new();
            let img = decoder.decode(data)?;
            Ok(DecodedImage {
                pixels: img.pixels,
                width: img.width,
                height: img.height,
                format,
            })
        }
        ImageFormat::Unknown => Err(DecodeError::UnsupportedFormat),
    }
}
