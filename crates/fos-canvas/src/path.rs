//! Path2D
//!
//! Path construction for Canvas 2D.

/// 2D Path
#[derive(Debug, Clone, Default)]
pub struct Path2D {
    commands: Vec<PathCommand>,
    current_x: f64,
    current_y: f64,
    start_x: f64,
    start_y: f64,
}

/// Path command
#[derive(Debug, Clone, Copy)]
pub enum PathCommand {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    QuadraticCurveTo { cpx: f64, cpy: f64, x: f64, y: f64 },
    BezierCurveTo { cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64 },
    Arc { x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64, counterclockwise: bool },
    ArcTo { x1: f64, y1: f64, x2: f64, y2: f64, radius: f64 },
    Ellipse { x: f64, y: f64, rx: f64, ry: f64, rotation: f64, start_angle: f64, end_angle: f64, counterclockwise: bool },
    Rect { x: f64, y: f64, width: f64, height: f64 },
    ClosePath,
}

impl Path2D {
    /// Create new empty path
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create path from another path
    pub fn from_path(path: &Path2D) -> Self {
        path.clone()
    }
    
    /// Move to point
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.commands.push(PathCommand::MoveTo(x, y));
        self.current_x = x;
        self.current_y = y;
        self.start_x = x;
        self.start_y = y;
    }
    
    /// Line to point
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.commands.push(PathCommand::LineTo(x, y));
        self.current_x = x;
        self.current_y = y;
    }
    
    /// Quadratic curve
    pub fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
        self.commands.push(PathCommand::QuadraticCurveTo { cpx, cpy, x, y });
        self.current_x = x;
        self.current_y = y;
    }
    
    /// Bezier curve
    pub fn bezier_curve_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        self.commands.push(PathCommand::BezierCurveTo { cp1x, cp1y, cp2x, cp2y, x, y });
        self.current_x = x;
        self.current_y = y;
    }
    
    /// Arc
    pub fn arc(&mut self, x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64, counterclockwise: bool) {
        self.commands.push(PathCommand::Arc { x, y, radius, start_angle, end_angle, counterclockwise });
        // Update current position to end of arc
        self.current_x = x + radius * end_angle.cos();
        self.current_y = y + radius * end_angle.sin();
    }
    
    /// Arc to
    pub fn arc_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) {
        self.commands.push(PathCommand::ArcTo { x1, y1, x2, y2, radius });
    }
    
    /// Ellipse
    pub fn ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64, rotation: f64, start_angle: f64, end_angle: f64, counterclockwise: bool) {
        self.commands.push(PathCommand::Ellipse { x, y, rx, ry, rotation, start_angle, end_angle, counterclockwise });
    }
    
    /// Rectangle
    pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.commands.push(PathCommand::Rect { x, y, width, height });
    }
    
    /// Round rectangle
    pub fn round_rect(&mut self, x: f64, y: f64, width: f64, height: f64, radii: f64) {
        // Construct rounded rect from arcs and lines
        self.move_to(x + radii, y);
        self.line_to(x + width - radii, y);
        self.arc_to(x + width, y, x + width, y + radii, radii);
        self.line_to(x + width, y + height - radii);
        self.arc_to(x + width, y + height, x + width - radii, y + height, radii);
        self.line_to(x + radii, y + height);
        self.arc_to(x, y + height, x, y + height - radii, radii);
        self.line_to(x, y + radii);
        self.arc_to(x, y, x + radii, y, radii);
        self.close_path();
    }
    
    /// Close path
    pub fn close_path(&mut self) {
        self.commands.push(PathCommand::ClosePath);
        self.current_x = self.start_x;
        self.current_y = self.start_y;
    }
    
    /// Add another path
    pub fn add_path(&mut self, path: &Path2D) {
        self.commands.extend(path.commands.iter().cloned());
    }
    
    /// Get commands
    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_path_basic() {
        let mut path = Path2D::new();
        path.move_to(10.0, 10.0);
        path.line_to(100.0, 10.0);
        path.line_to(100.0, 100.0);
        path.close_path();
        
        assert_eq!(path.commands().len(), 4);
    }
    
    #[test]
    fn test_path_rect() {
        let mut path = Path2D::new();
        path.rect(0.0, 0.0, 50.0, 50.0);
        
        assert!(!path.is_empty());
    }
}
