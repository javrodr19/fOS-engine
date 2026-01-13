//! AV1 Loop Filters
//!
//! Deblocking filter, CDEF, and loop restoration for AV1 decoding.

use super::frame::Frame;
use super::obu::{LoopFilterParams, CdefParams, RestorationParams, RestorationType};

/// Apply deblocking filter to frame
pub fn deblock_filter(frame: &mut Frame, params: &LoopFilterParams) {
    if !params.enabled {
        return;
    }
    
    let width = frame.width as usize;
    let height = frame.height as usize;
    
    // Vertical edges (filter horizontally)
    for plane in 0..frame.num_planes() {
        let (plane_w, plane_h) = frame.plane_dimensions(plane);
        let level = params.level[if plane == 0 { 0 } else { 2 }];
        
        if level == 0 {
            continue;
        }
        
        // Filter vertical edges every 4 pixels
        for y in 0..plane_h {
            for x in (4..plane_w).step_by(4) {
                filter_vertical_edge(frame, plane, x, y, level, params.sharpness);
            }
        }
        
        // Filter horizontal edges every 4 pixels
        for y in (4..plane_h).step_by(4) {
            for x in 0..plane_w {
                filter_horizontal_edge(frame, plane, x, y, level, params.sharpness);
            }
        }
    }
}

fn filter_vertical_edge(
    frame: &mut Frame,
    plane: usize,
    x: usize,
    y: usize,
    level: u8,
    sharpness: u8,
) {
    let (w, h) = frame.plane_dimensions(plane);
    if x < 2 || x + 2 > w {
        return;
    }
    
    // Get pixels around edge
    let p1 = frame.get_pixel(plane, x - 2, y);
    let p0 = frame.get_pixel(plane, x - 1, y);
    let q0 = frame.get_pixel(plane, x, y);
    let q1 = frame.get_pixel(plane, x + 1, y);
    
    // Calculate filter decision
    let (filter, _hev) = filter_decision(p1, p0, q0, q1, level, sharpness);
    
    if filter {
        // Apply 4-tap filter
        let f = ((p1 as i32 - q1 as i32) + 3 * (q0 as i32 - p0 as i32) + 4) >> 3;
        let f_clamped = f.clamp(-128, 127);
        
        frame.set_pixel(plane, x - 1, y, (p0 as i32 + f_clamped) as i16);
        frame.set_pixel(plane, x, y, (q0 as i32 - f_clamped) as i16);
    }
}

fn filter_horizontal_edge(
    frame: &mut Frame,
    plane: usize,
    x: usize,
    y: usize,
    level: u8,
    sharpness: u8,
) {
    let (w, h) = frame.plane_dimensions(plane);
    if y < 2 || y + 2 > h {
        return;
    }
    
    let p1 = frame.get_pixel(plane, x, y - 2);
    let p0 = frame.get_pixel(plane, x, y - 1);
    let q0 = frame.get_pixel(plane, x, y);
    let q1 = frame.get_pixel(plane, x, y + 1);
    
    let (filter, _hev) = filter_decision(p1, p0, q0, q1, level, sharpness);
    
    if filter {
        let f = ((p1 as i32 - q1 as i32) + 3 * (q0 as i32 - p0 as i32) + 4) >> 3;
        let f_clamped = f.clamp(-128, 127);
        
        frame.set_pixel(plane, x, y - 1, (p0 as i32 + f_clamped) as i16);
        frame.set_pixel(plane, x, y, (q0 as i32 - f_clamped) as i16);
    }
}

fn filter_decision(
    p1: i16,
    p0: i16,
    q0: i16,
    q1: i16,
    level: u8,
    sharpness: u8,
) -> (bool, bool) {
    let thresh = level as i32;
    let inner_thresh = if sharpness == 0 {
        thresh
    } else {
        (thresh >> sharpness.min(4)).max(1)
    };
    
    let dp1p0 = (p1 as i32 - p0 as i32).abs();
    let dq1q0 = (q1 as i32 - q0 as i32).abs();
    let dp0q0 = (p0 as i32 - q0 as i32).abs();
    
    let filter = dp1p0 <= inner_thresh && 
                 dq1q0 <= inner_thresh &&
                 dp0q0 * 2 + ((dp1p0 + dq1q0) >> 1) <= thresh;
    
    let hev = dp1p0 > inner_thresh >> 1 || dq1q0 > inner_thresh >> 1;
    
    (filter, hev)
}

/// Apply CDEF (Constrained Directional Enhancement Filter)
pub fn cdef_filter(frame: &mut Frame, params: &CdefParams) {
    if !params.enabled {
        return;
    }
    
    let (width, height) = (frame.width as usize, frame.height as usize);
    
    // Process in 8x8 blocks
    for plane in 0..frame.num_planes() {
        let (plane_w, plane_h) = frame.plane_dimensions(plane);
        let (pri_strength, sec_strength) = if plane == 0 {
            (params.y_pri_strength[0], params.y_sec_strength[0])
        } else {
            (params.uv_pri_strength[0], params.uv_sec_strength[0])
        };
        
        if pri_strength == 0 && sec_strength == 0 {
            continue;
        }
        
        for by in (0..plane_h).step_by(8) {
            for bx in (0..plane_w).step_by(8) {
                cdef_block(
                    frame,
                    plane,
                    bx,
                    by,
                    8.min(plane_w - bx),
                    8.min(plane_h - by),
                    pri_strength,
                    sec_strength,
                    params.damping,
                );
            }
        }
    }
}

fn cdef_block(
    frame: &mut Frame,
    plane: usize,
    bx: usize,
    by: usize,
    block_w: usize,
    block_h: usize,
    pri_strength: u8,
    sec_strength: u8,
    damping: u8,
) {
    if pri_strength == 0 && sec_strength == 0 {
        return;
    }
    
    // Find best direction for the block
    let direction = find_cdef_direction(frame, plane, bx, by, block_w, block_h);
    
    // Apply directional filter
    for y in by..by + block_h {
        for x in bx..bx + block_w {
            let val = frame.get_pixel(plane, x, y);
            
            // Primary filter (along direction)
            let mut sum = 0i32;
            let mut count = 0;
            
            let (dx, dy) = CDEF_DIRECTIONS[direction];
            
            for d in [-2i32, -1, 1, 2] {
                let nx = (x as i32 + dx * d) as usize;
                let ny = (y as i32 + dy * d) as usize;
                
                let (pw, ph) = frame.plane_dimensions(plane);
                if nx < pw && ny < ph {
                    let neighbor = frame.get_pixel(plane, nx, ny);
                    let diff = (neighbor - val).abs() as i32;
                    let strength = pri_strength as i32;
                    
                    // Damped difference
                    let damped = constrain(diff, strength, damping as i32);
                    sum += (neighbor as i32 - val as i32).signum() * damped;
                    count += 1;
                }
            }
            
            if count > 0 {
                let filtered = val as i32 + (sum + count / 2) / count.max(1);
                frame.set_pixel(plane, x, y, filtered as i16);
            }
        }
    }
}

fn find_cdef_direction(
    frame: &Frame,
    plane: usize,
    bx: usize,
    by: usize,
    block_w: usize,
    block_h: usize,
) -> usize {
    // Compute directional strength for each direction and pick best
    let mut best_dir = 0;
    let mut best_cost = i32::MAX;
    
    for dir in 0..8 {
        let (dx, dy) = CDEF_DIRECTIONS[dir];
        let mut cost = 0i32;
        
        for y in by..by + block_h {
            for x in bx..bx + block_w {
                let val = frame.get_pixel(plane, x, y) as i32;
                
                let (pw, ph) = frame.plane_dimensions(plane);
                let nx = (x as i32 + dx) as usize;
                let ny = (y as i32 + dy) as usize;
                
                if nx < pw && ny < ph {
                    let neighbor = frame.get_pixel(plane, nx, ny) as i32;
                    cost += (val - neighbor).abs();
                }
            }
        }
        
        if cost < best_cost {
            best_cost = cost;
            best_dir = dir;
        }
    }
    
    best_dir
}

fn constrain(diff: i32, strength: i32, damping: i32) -> i32 {
    if diff == 0 {
        return 0;
    }
    
    let magnitude = diff.abs();
    let threshold = strength << (damping - 3).max(0);
    
    if magnitude >= threshold {
        0
    } else {
        let damped = magnitude.min(strength - (magnitude >> (damping - 3).max(0)));
        if diff < 0 { -damped } else { damped }
    }
}

// CDEF direction vectors
static CDEF_DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),   // 0°
    (1, 1),   // 45°
    (0, 1),   // 90°
    (-1, 1),  // 135°
    (-1, 0),  // 180°
    (-1, -1), // 225°
    (0, -1),  // 270°
    (1, -1),  // 315°
];

/// Apply loop restoration filter
pub fn loop_restoration(frame: &mut Frame, params: &RestorationParams) {
    if !params.enabled {
        return;
    }
    
    // Apply restoration to each plane
    for plane in 0..frame.num_planes() {
        let rtype = match plane {
            0 => params.type_y,
            1 => params.type_u,
            _ => params.type_v,
        };
        
        match rtype {
            RestorationType::None => {}
            RestorationType::Wiener => {
                wiener_filter(frame, plane, params.unit_size);
            }
            RestorationType::SelfGuided => {
                self_guided_filter(frame, plane, params.unit_size);
            }
            RestorationType::Switchable => {
                // In switchable mode, would decode which filter per unit
                wiener_filter(frame, plane, params.unit_size);
            }
        }
    }
}

fn wiener_filter(frame: &mut Frame, plane: usize, unit_size: u32) {
    let (width, height) = frame.plane_dimensions(plane);
    let unit = unit_size as usize;
    
    // Default Wiener filter taps (7-tap filter)
    let taps = [4i32, -25, 60, 128, 60, -25, 4]; // Sum = 256
    
    // Process in restoration units
    for uy in (0..height).step_by(unit) {
        for ux in (0..width).step_by(unit) {
            let uw = unit.min(width - ux);
            let uh = unit.min(height - uy);
            
            wiener_unit(frame, plane, ux, uy, uw, uh, &taps);
        }
    }
}

fn wiener_unit(
    frame: &mut Frame,
    plane: usize,
    ux: usize,
    uy: usize,
    uw: usize,
    uh: usize,
    taps: &[i32; 7],
) {
    let (width, height) = frame.plane_dimensions(plane);
    
    // Temporary buffer for filtered values
    let mut temp = vec![0i16; uw * uh];
    
    // Horizontal filter
    for y in 0..uh {
        for x in 0..uw {
            let px = ux + x;
            let py = uy + y;
            
            let mut sum = 0i32;
            for (i, &tap) in taps.iter().enumerate() {
                let nx = (px as i32 + i as i32 - 3).clamp(0, width as i32 - 1) as usize;
                sum += frame.get_pixel(plane, nx, py) as i32 * tap;
            }
            temp[y * uw + x] = ((sum + 128) >> 8) as i16;
        }
    }
    
    // Vertical filter (using temp buffer)
    for y in 0..uh {
        for x in 0..uw {
            let px = ux + x;
            let py = uy + y;
            
            let mut sum = 0i32;
            for (i, &tap) in taps.iter().enumerate() {
                let ny = (y as i32 + i as i32 - 3).clamp(0, uh as i32 - 1) as usize;
                sum += temp[ny * uw + x] as i32 * tap;
            }
            frame.set_pixel(plane, px, py, ((sum + 128) >> 8) as i16);
        }
    }
}

fn self_guided_filter(frame: &mut Frame, plane: usize, unit_size: u32) {
    let (width, height) = frame.plane_dimensions(plane);
    let unit = unit_size as usize;
    
    // Self-guided filter parameters
    let radius = 2;
    let eps = 10; // Regularization parameter
    
    for uy in (0..height).step_by(unit) {
        for ux in (0..width).step_by(unit) {
            let uw = unit.min(width - ux);
            let uh = unit.min(height - uy);
            
            self_guided_unit(frame, plane, ux, uy, uw, uh, radius, eps);
        }
    }
}

fn self_guided_unit(
    frame: &mut Frame,
    plane: usize,
    ux: usize,
    uy: usize,
    uw: usize,
    uh: usize,
    radius: usize,
    eps: i32,
) {
    let (width, height) = frame.plane_dimensions(plane);
    
    // Compute local mean and variance in sliding window
    for y in 0..uh {
        for x in 0..uw {
            let px = ux + x;
            let py = uy + y;
            
            let mut sum = 0i64;
            let mut sum_sq = 0i64;
            let mut count = 0i64;
            
            for dy in 0..=radius * 2 {
                for dx in 0..=radius * 2 {
                    let nx = (px as i32 + dx as i32 - radius as i32)
                        .clamp(0, width as i32 - 1) as usize;
                    let ny = (py as i32 + dy as i32 - radius as i32)
                        .clamp(0, height as i32 - 1) as usize;
                    
                    let val = frame.get_pixel(plane, nx, ny) as i64;
                    sum += val;
                    sum_sq += val * val;
                    count += 1;
                }
            }
            
            let mean = sum / count;
            let variance = (sum_sq / count - mean * mean).max(0);
            
            // Self-guided restoration formula
            let current = frame.get_pixel(plane, px, py) as i64;
            let a = (variance * 256) / (variance + eps as i64 + 1);
            let b = mean * (256 - a) / 256;
            
            let restored = (a * current + b * 256 + 128) / 256;
            frame.set_pixel(plane, px, py, restored as i16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_filter_decision() {
        // Smooth edge - should filter
        let (filter, hev) = filter_decision(100, 100, 100, 100, 40, 0);
        assert!(filter);
        assert!(!hev);
        
        // Sharp edge - should not filter
        let (filter, _) = filter_decision(0, 0, 255, 255, 10, 0);
        assert!(!filter);
    }
    
    #[test]
    fn test_constrain() {
        assert_eq!(constrain(0, 10, 5), 0);
        assert_eq!(constrain(5, 10, 5), 5);
    }
    
    #[test]
    fn test_cdef_directions() {
        assert_eq!(CDEF_DIRECTIONS[0], (1, 0));  // Horizontal
        assert_eq!(CDEF_DIRECTIONS[2], (0, 1));  // Vertical
    }
}
