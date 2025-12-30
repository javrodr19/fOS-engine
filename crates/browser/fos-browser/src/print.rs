//! Print functionality
//!
//! Print page to PDF or printer.

use std::path::PathBuf;

/// Print settings
#[derive(Debug, Clone)]
pub struct PrintSettings {
    pub paper_size: PaperSize,
    pub orientation: Orientation,
    pub margins: Margins,
    pub scale: f32,
    pub background_graphics: bool,
    pub headers_footers: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper_size: PaperSize::A4,
            orientation: Orientation::Portrait,
            margins: Margins::default(),
            scale: 1.0,
            background_graphics: true,
            headers_footers: true,
        }
    }
}

/// Paper size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    A4,
    Letter,
    Legal,
    A3,
    A5,
    Custom { width_mm: u32, height_mm: u32 },
}

impl PaperSize {
    /// Get dimensions in millimeters
    pub fn dimensions_mm(&self) -> (u32, u32) {
        match self {
            Self::A4 => (210, 297),
            Self::Letter => (216, 279),
            Self::Legal => (216, 356),
            Self::A3 => (297, 420),
            Self::A5 => (148, 210),
            Self::Custom { width_mm, height_mm } => (*width_mm, *height_mm),
        }
    }
    
    /// Get dimensions in points (72 dpi)
    pub fn dimensions_pt(&self) -> (f32, f32) {
        let (w, h) = self.dimensions_mm();
        (w as f32 * 2.834, h as f32 * 2.834)
    }
}

/// Page orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// Page margins
#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Default for Margins {
    fn default() -> Self {
        Self {
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
            left: 10.0,
        }
    }
}

/// Print manager
#[derive(Debug, Default)]
pub struct PrintManager {
    settings: PrintSettings,
}

impl PrintManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get current settings
    pub fn settings(&self) -> &PrintSettings {
        &self.settings
    }
    
    /// Update settings
    pub fn set_settings(&mut self, settings: PrintSettings) {
        self.settings = settings;
    }
    
    /// Print to PDF file
    pub fn print_to_pdf(&self, html: &str, url: &str, output: PathBuf) -> Result<(), PrintError> {
        // For now, just write a simple PDF header
        // A real implementation would use a PDF library
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(&output).map_err(|e| PrintError::IoError(e.to_string()))?;
        
        // Minimal PDF structure
        let content = format!(
            "%PDF-1.4\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n\
            4 0 obj\n<< /Length 44 >>\nstream\nBT /F1 12 Tf 72 720 Td (Page from: {}) Tj ET\nendstream\nendobj\n\
            xref\n0 5\n\
            0000000000 65535 f \n\
            0000000009 00000 n \n\
            0000000058 00000 n \n\
            0000000115 00000 n \n\
            0000000214 00000 n \n\
            trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n307\n%%EOF",
            url
        );
        
        file.write_all(content.as_bytes()).map_err(|e| PrintError::IoError(e.to_string()))?;
        
        log::info!("PDF saved to {:?}", output);
        Ok(())
    }
    
    /// Print using system print dialog (Linux)
    #[cfg(target_os = "linux")]
    pub fn print_system(&self, html: &str, url: &str) -> Result<(), PrintError> {
        use std::process::Command;
        use std::io::Write;
        
        // Create temp file
        let temp_path = std::env::temp_dir().join("fos_print.html");
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| PrintError::IoError(e.to_string()))?;
        file.write_all(html.as_bytes())
            .map_err(|e| PrintError::IoError(e.to_string()))?;
        
        // Try to use xdg-open or firefox to print
        let result = Command::new("xdg-open")
            .arg(&temp_path)
            .spawn();
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(PrintError::PrintFailed(e.to_string())),
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    pub fn print_system(&self, _html: &str, _url: &str) -> Result<(), PrintError> {
        Err(PrintError::NotSupported)
    }
}

/// Print errors
#[derive(Debug)]
pub enum PrintError {
    IoError(String),
    PrintFailed(String),
    NotSupported,
}

impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::PrintFailed(e) => write!(f, "Print failed: {}", e),
            Self::NotSupported => write!(f, "Printing not supported on this platform"),
        }
    }
}

impl std::error::Error for PrintError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_paper_size() {
        let (w, h) = PaperSize::A4.dimensions_mm();
        assert_eq!(w, 210);
        assert_eq!(h, 297);
    }
}
