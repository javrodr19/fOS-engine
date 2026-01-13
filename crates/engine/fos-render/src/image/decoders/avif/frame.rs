//! AV1 Frame Buffer
//!
//! Frame storage with Y/U/V planes and block access utilities.

use super::AvifError;

/// Decoded frame buffer
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub subsampling_x: u8,
    pub subsampling_y: u8,
    pub monochrome: bool,
    pub planes: Vec<Plane>,
}

/// Single plane buffer
#[derive(Debug, Clone)]
pub struct Plane {
    pub data: Vec<i16>,  // 16-bit for 10/12-bit support
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

impl Frame {
    pub fn new(
        width: u32,
        height: u32,
        bit_depth: u8,
        subsampling_x: u8,
        subsampling_y: u8,
        monochrome: bool,
    ) -> Self {
        let mut planes = Vec::new();
        
        // Y plane (luma)
        let y_plane = Plane::new(width as usize, height as usize);
        planes.push(y_plane);
        
        // U and V planes (chroma) if not monochrome
        if !monochrome {
            let chroma_w = width as usize >> subsampling_x;
            let chroma_h = height as usize >> subsampling_y;
            planes.push(Plane::new(chroma_w, chroma_h));
            planes.push(Plane::new(chroma_w, chroma_h));
        }
        
        Self {
            width,
            height,
            bit_depth,
            subsampling_x,
            subsampling_y,
            monochrome,
            planes,
        }
    }
    
    /// Number of planes
    pub fn num_planes(&self) -> usize {
        if self.monochrome { 1 } else { 3 }
    }
    
    /// Get plane dimensions
    pub fn plane_dimensions(&self, plane: usize) -> (usize, usize) {
        if plane >= self.planes.len() {
            return (0, 0);
        }
        (self.planes[plane].width, self.planes[plane].height)
    }
    
    /// Get pixel value
    pub fn get_pixel(&self, plane: usize, x: usize, y: usize) -> i16 {
        if plane >= self.planes.len() {
            return 0;
        }
        self.planes[plane].get(x, y)
    }
    
    /// Set pixel value
    pub fn set_pixel(&mut self, plane: usize, x: usize, y: usize, value: i16) {
        if plane < self.planes.len() {
            self.planes[plane].set(x, y, value);
        }
    }
    
    /// Get reference pixels for intra prediction
    pub fn get_reference_pixels(
        &self,
        col: u32,
        row: u32,
        width: u32,
        height: u32,
        plane: usize,
    ) -> (Vec<i16>, Vec<i16>, i16) {
        let col = col as usize;
        let row = row as usize;
        let width = width as usize;
        let height = height as usize;
        
        // Default value for unavailable pixels
        let default = 1 << (self.bit_depth - 1);
        
        // Top reference (including top-right)
        let mut top = Vec::with_capacity(width * 2);
        if row > 0 {
            for x in col..col + width * 2 {
                let (pw, _) = self.plane_dimensions(plane);
                if x < pw {
                    top.push(self.get_pixel(plane, x, row - 1));
                } else {
                    top.push(if !top.is_empty() { *top.last().unwrap() } else { default as i16 });
                }
            }
        } else {
            top.resize(width * 2, default as i16);
        }
        
        // Left reference (including bottom-left)
        let mut left = Vec::with_capacity(height * 2);
        if col > 0 {
            for y in row..row + height * 2 {
                let (_, ph) = self.plane_dimensions(plane);
                if y < ph {
                    left.push(self.get_pixel(plane, col - 1, y));
                } else {
                    left.push(if !left.is_empty() { *left.last().unwrap() } else { default as i16 });
                }
            }
        } else {
            left.resize(height * 2, default as i16);
        }
        
        // Top-left
        let top_left = if row > 0 && col > 0 {
            self.get_pixel(plane, col - 1, row - 1)
        } else {
            default as i16
        };
        
        (top, left, top_left)
    }
    
    /// Write block to frame buffer
    pub fn write_block(
        &mut self,
        col: u32,
        row: u32,
        width: u32,
        height: u32,
        data: &[i16],
        plane: usize,
    ) -> Result<(), AvifError> {
        if plane >= self.planes.len() {
            return Err(AvifError::DecodingError("Invalid plane".into()));
        }
        
        let max_value = (1 << self.bit_depth) - 1;
        
        for y in 0..height as usize {
            for x in 0..width as usize {
                let idx = y * width as usize + x;
                if idx < data.len() {
                    let val = data[idx].clamp(0, max_value);
                    self.set_pixel(plane, col as usize + x, row as usize + y, val);
                }
            }
        }
        
        Ok(())
    }
    
    /// Compute average luma for CfL prediction
    pub fn compute_luma_average(&self, col: u32, row: u32, width: u32, height: u32) -> i32 {
        let mut sum: i64 = 0;
        let count = (width * height) as i64;
        
        for y in row..row + height {
            for x in col..col + width {
                sum += self.get_pixel(0, x as usize, y as usize) as i64;
            }
        }
        
        if count > 0 {
            (sum / count) as i32
        } else {
            1 << (self.bit_depth - 1)
        }
    }
}

impl Plane {
    pub fn new(width: usize, height: usize) -> Self {
        let stride = width;
        let data = vec![0i16; stride * height];
        
        Self {
            data,
            width,
            height,
            stride,
        }
    }
    
    pub fn get(&self, x: usize, y: usize) -> i16 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data[y * self.stride + x]
    }
    
    pub fn set(&mut self, x: usize, y: usize, value: i16) {
        if x < self.width && y < self.height {
            self.data[y * self.stride + x] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_frame_creation() {
        let frame = Frame::new(1920, 1080, 8, 1, 1, false);
        
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.num_planes(), 3);
        
        // Luma plane
        assert_eq!(frame.plane_dimensions(0), (1920, 1080));
        // Chroma planes (4:2:0)
        assert_eq!(frame.plane_dimensions(1), (960, 540));
        assert_eq!(frame.plane_dimensions(2), (960, 540));
    }
    
    #[test]
    fn test_monochrome_frame() {
        let frame = Frame::new(100, 100, 8, 0, 0, true);
        
        assert_eq!(frame.num_planes(), 1);
    }
    
    #[test]
    fn test_pixel_access() {
        let mut frame = Frame::new(10, 10, 8, 1, 1, false);
        
        frame.set_pixel(0, 5, 5, 128);
        assert_eq!(frame.get_pixel(0, 5, 5), 128);
        
        // Out of bounds should return 0
        assert_eq!(frame.get_pixel(0, 100, 100), 0);
    }
    
    #[test]
    fn test_write_block() {
        let mut frame = Frame::new(16, 16, 8, 1, 1, false);
        let data: Vec<i16> = (0..16).collect();
        
        frame.write_block(0, 0, 4, 4, &data, 0).unwrap();
        
        assert_eq!(frame.get_pixel(0, 0, 0), 0);
        assert_eq!(frame.get_pixel(0, 1, 0), 1);
        assert_eq!(frame.get_pixel(0, 0, 1), 4);
    }
    
    #[test]
    fn test_luma_average() {
        let mut frame = Frame::new(4, 4, 8, 1, 1, false);
        
        // Fill with value 100
        for y in 0..4 {
            for x in 0..4 {
                frame.set_pixel(0, x, y, 100);
            }
        }
        
        assert_eq!(frame.compute_luma_average(0, 0, 4, 4), 100);
    }
}
