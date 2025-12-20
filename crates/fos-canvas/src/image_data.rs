//! ImageData
//!
//! Pixel data manipulation for Canvas 2D.

/// ImageData - raw pixel data
#[derive(Debug, Clone)]
pub struct ImageData {
    data: Vec<u8>,
    width: u32,
    height: u32,
    color_space: ColorSpace,
}

/// Color space
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorSpace {
    #[default]
    Srgb,
    DisplayP3,
}

impl ImageData {
    /// Create new ImageData with specified dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let data = vec![0u8; (width * height * 4) as usize];
        Self {
            data,
            width,
            height,
            color_space: ColorSpace::default(),
        }
    }
    
    /// Create from existing data
    pub fn from_data(data: Vec<u8>, width: u32, height: u32) -> Result<Self, ImageDataError> {
        let expected = (width * height * 4) as usize;
        if data.len() != expected {
            return Err(ImageDataError::InvalidDataLength {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            width,
            height,
            color_space: ColorSpace::default(),
        })
    }
    
    /// Get width
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get height
    pub fn height(&self) -> u32 {
        self.height
    }
    
    /// Get data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Get mutable data
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
    
    /// Get pixel at (x, y)
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        Some((self.data[idx], self.data[idx + 1], self.data[idx + 2], self.data[idx + 3]))
    }
    
    /// Set pixel at (x, y)
    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
        if x < self.width && y < self.height {
            let idx = ((y * self.width + x) * 4) as usize;
            self.data[idx] = r;
            self.data[idx + 1] = g;
            self.data[idx + 2] = b;
            self.data[idx + 3] = a;
        }
    }
    
    /// Color space
    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }
}

/// ImageData error
#[derive(Debug, Clone)]
pub enum ImageDataError {
    InvalidDataLength { expected: usize, actual: usize },
}

impl std::fmt::Display for ImageDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDataLength { expected, actual } => {
                write!(f, "Invalid data length: expected {}, got {}", expected, actual)
            }
        }
    }
}

impl std::error::Error for ImageDataError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_image_data() {
        let mut img = ImageData::new(10, 10);
        assert_eq!(img.width(), 10);
        assert_eq!(img.height(), 10);
        
        img.set_pixel(5, 5, 255, 0, 0, 255);
        assert_eq!(img.get_pixel(5, 5), Some((255, 0, 0, 255)));
    }
}
