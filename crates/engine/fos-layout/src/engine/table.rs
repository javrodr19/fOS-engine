//! Tables (CSS 2.1 §17): the cell grid with column and row spans,
//! automatic and fixed column widths, row heights, vertical alignment in
//! cells, border spacing and captions

use fos_css::style::{BorderCollapse, LpAuto, Style, TableLayout, VerticalAlign};
use fos_dom::NodeId;

use super::block::{content_size, intrinsic_outer, layout_sized, Forced, LayoutCtx, Sizing};
use super::box_tree::{TableBox, TableCell};
use super::fragment::{BoxFragment, BoxFragmentKind, Fragment, Rect};

/// Cells placed in the grid: (row, column, cell)
struct Grid<'a> {
    cols: usize,
    cells: Vec<(usize, usize, &'a TableCell)>,
}

fn grid(t: &TableBox) -> Grid<'_> {
    // occupied[r] = columns taken in row r by cells from rows above
    let mut occupied: Vec<Vec<bool>> = vec![Vec::new(); t.rows.len()];
    let mut cells = Vec::new();
    let mut cols = 0;
    for (r, row) in t.rows.iter().enumerate() {
        let mut c = 0;
        for cell in &row.cells {
            while occupied[r].get(c).copied().unwrap_or(false) {
                c += 1;
            }
            let (cs, rs) = (cell.colspan as usize, (cell.rowspan as usize).min(t.rows.len() - r));
            for rr in r..r + rs {
                let o = &mut occupied[rr];
                if o.len() < c + cs {
                    o.resize(c + cs, false);
                }
                for x in c..c + cs {
                    o[x] = true;
                }
            }
            cells.push((r, c, cell));
            c += cs;
            cols = cols.max(c);
        }
    }
    Grid { cols, cells }
}

/// Horizontal and vertical spacing between cells. Collapsed borders are
/// shared: neighbouring cells (and the outer cells and the table) overlap
/// by the cells' border width, so a border is drawn once
fn spacing(style: &Style, t: &TableBox) -> (f32, f32) {
    match style.inherited.border_collapse {
        BorderCollapse::Separate => style.inherited.border_spacing,
        BorderCollapse::Collapse => {
            let (mut h, mut v) = (0.0f32, 0.0f32);
            for cell in t.rows.iter().flat_map(|r| &r.cells) {
                let w = &cell.b.style.border.width;
                h = h.max(w[1].min(w[3]));
                v = v.max(w[0].min(w[2]));
            }
            (-h, -v)
        }
    }
}

/// Per column min- and max-content widths (cell border boxes)
fn column_widths(ctx: &mut LayoutCtx, g: &Grid) -> (Vec<f32>, Vec<f32>) {
    let mut min = vec![0.0f32; g.cols];
    let mut max = vec![0.0f32; g.cols];
    let mut spanning = Vec::new();
    for &(_, c, cell) in &g.cells {
        let (cmin, cmax) = intrinsic_outer(ctx, &cell.b);
        // A fixed width is a floor for both
        let fixed = match cell.b.style.box_.width {
            LpAuto::Lp(l) if !l.has_percent() => {
                let b = &cell.b.style;
                let bp = b.border.width[1] + b.border.width[3] + b.box_.padding[1].resolve(0.0) + b.box_.padding[3].resolve(0.0);
                Some(content_size(b, l.px, bp) + bp)
            }
            _ => None,
        };
        let (cmin, cmax) = match fixed {
            Some(w) => (cmin.max(w), cmin.max(w)),
            None => (cmin, cmax.max(cmin)),
        };
        if cell.colspan == 1 {
            min[c] = min[c].max(cmin);
            max[c] = max[c].max(cmax);
        } else {
            spanning.push((c, cell.colspan as usize, cmin, cmax));
        }
    }
    // Spanning cells widen their columns evenly where they need more
    for (c, span, cmin, cmax) in spanning {
        let end = (c + span).min(g.cols);
        let n = (end - c) as f32;
        let (have_min, have_max): (f32, f32) = (min[c..end].iter().sum(), max[c..end].iter().sum());
        if cmin > have_min {
            for m in &mut min[c..end] {
                *m += (cmin - have_min) / n;
            }
        }
        if cmax > have_max {
            for m in &mut max[c..end] {
                *m += (cmax - have_max) / n;
            }
        }
        for i in c..end {
            max[i] = max[i].max(min[i]);
        }
    }
    (min, max)
}

/// The table's min- and max-content widths (inside its border and
/// padding)
pub fn intrinsic_table(ctx: &mut LayoutCtx, style: &Style, t: &TableBox) -> (f32, f32) {
    let g = grid(t);
    let (min, max) = column_widths(ctx, &g);
    let hs = spacing(style, t).0 * (g.cols + 1) as f32;
    let (mut tmin, tmax) = (min.iter().sum::<f32>() + hs, max.iter().sum::<f32>() + hs);
    for c in &t.captions {
        tmin = tmin.max(intrinsic_outer(ctx, c).0);
    }
    (tmin, tmax.max(tmin))
}

pub struct TableLayoutResult {
    pub frags: Vec<Fragment>,
    pub height: f32,
    /// The width used (never below the table's minimum)
    pub width: f32,
}

/// Lay out a table's contents in a content box `width` wide
pub fn layout_table(ctx: &mut LayoutCtx, style: &Style, t: &TableBox, width: f32, auto_width: bool) -> TableLayoutResult {
    let g = grid(t);
    let (hs, vs) = spacing(style, t);
    let (min, max) = column_widths(ctx, &g);
    let spacing_w = hs * (g.cols + 1) as f32;
    let sum_min: f32 = min.iter().sum();
    let sum_max: f32 = max.iter().sum();
    let width = width.max(sum_min + spacing_w);
    let avail = width - spacing_w;

    // Column widths
    let cols: Vec<f32> = if style.box_.table_layout == TableLayout::Fixed && !auto_width {
        // From the first row's cells; the rest share what is left
        let mut w: Vec<Option<f32>> = vec![None; g.cols];
        for &(r, c, cell) in &g.cells {
            if r == 0 && cell.colspan == 1 {
                if let LpAuto::Lp(l) = cell.b.style.box_.width {
                    w[c] = Some(l.resolve(avail));
                }
            }
        }
        let fixed: f32 = w.iter().flatten().sum();
        let n_auto = w.iter().filter(|x| x.is_none()).count().max(1) as f32;
        let share = ((avail - fixed) / n_auto).max(0.0);
        w.into_iter().map(|x| x.unwrap_or(share)).collect()
    } else if avail >= sum_max {
        // Extra space goes to columns in proportion to their max widths
        let extra = if auto_width { 0.0 } else { avail - sum_max };
        if sum_max > 0.0 {
            max.iter().map(|m| m + extra * m / sum_max).collect()
        } else {
            vec![avail / g.cols.max(1) as f32; g.cols]
        }
    } else if sum_max > sum_min {
        let f = (avail - sum_min) / (sum_max - sum_min);
        min.iter().zip(&max).map(|(a, b)| a + (b - a) * f).collect()
    } else {
        min.clone()
    };
    let used_width = cols.iter().sum::<f32>() + spacing_w;
    let col_x: Vec<f32> = cols
        .iter()
        .scan(hs, |x, w| {
            let at = *x;
            *x += w + hs;
            Some(at)
        })
        .collect();

    let mut frags = Vec::new();
    let mut y = 0.0f32;
    // Captions above the grid
    for c in &t.captions {
        let mut laid = layout_sized(ctx, c, used_width, None, Sizing::Stretch, false, Forced::default());
        let top = laid.mt.size();
        laid.frag.translate(0.0, y + top);
        y += top + laid.frag.border_box.h + laid.mb.size();
        frags.push(Fragment::Box(laid.frag));
    }
    let grid_top = y;

    // Cells at their column widths
    let span_width = |c: usize, span: usize| -> f32 {
        let end = (c + span).min(g.cols);
        cols[c..end].iter().sum::<f32>() + hs * (end - c).saturating_sub(1) as f32
    };
    let mut laid_cells: Vec<(usize, usize, usize, BoxFragment)> = Vec::with_capacity(g.cells.len());
    let mut row_h: Vec<f32> = t.rows.iter().map(|r| r.style.box_.height.resolve(0.0).unwrap_or(0.0)).collect();
    for &(r, c, cell) in &g.cells {
        let w = span_width(c, cell.colspan as usize);
        let laid = layout_sized(ctx, &cell.b, w, None, Sizing::Shrink, false, Forced { width: Some(w), height: None });
        let rs = (cell.rowspan as usize).min(t.rows.len() - r);
        if rs == 1 {
            row_h[r] = row_h[r].max(laid.frag.border_box.h);
        }
        laid_cells.push((r, c, rs, laid.frag));
    }
    // Row-spanning cells make their last row taller when needed
    for (r, _, rs, f) in &laid_cells {
        if *rs > 1 {
            let have: f32 = row_h[*r..r + rs].iter().sum::<f32>() + vs * (rs - 1) as f32;
            if f.border_box.h > have {
                row_h[r + rs - 1] += f.border_box.h - have;
            }
        }
    }
    let mut row_y = Vec::with_capacity(t.rows.len());
    let mut ry = grid_top + vs;
    for h in &row_h {
        row_y.push(ry);
        ry += h + vs;
    }
    let grid_bottom = if t.rows.is_empty() { grid_top } else { ry };

    // Rows (for their backgrounds) holding their cells
    let mut rows: Vec<BoxFragment> = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| BoxFragment {
            node: row.node,
            kind: BoxFragmentKind::Block,
            style: row.style.clone(),
            border_box: Rect::new(hs, row_y[i], (used_width - 2.0 * hs).max(0.0), row_h[i]),
            border: [0.0; 4],
            padding: [0.0; 4],
            children: Vec::new(),
            ink: Rect::default(),
            marker: None,
            replaced: None,
            scroll_extent: None,
        })
        .collect();
    for (r, c, rs, mut f) in laid_cells {
        let h: f32 = row_h[r..r + rs].iter().sum::<f32>() + vs * (rs - 1) as f32;
        // The cell fills its rows; its content aligns vertically
        let free = h - f.border_box.h;
        let shift = match f.style.box_.vertical_align {
            VerticalAlign::Middle => free / 2.0,
            VerticalAlign::Bottom | VerticalAlign::TextBottom => free,
            _ => 0.0,
        };
        if free > 0.0 {
            if shift > 0.0 {
                for ch in &mut f.children {
                    ch.translate(0.0, shift);
                }
            }
            f.border_box.h = h;
        }
        f.translate(col_x[c] - f.border_box.x, row_y[r] - f.border_box.y);
        f.update_ink();
        rows[r].children.push(Fragment::Box(f));
    }
    for mut row in rows {
        row.update_ink();
        if row.node == NodeId::NONE && !row.style.background.is_visible() {
            // Anonymous rows add nothing but their cells
            frags.extend(row.children);
        } else {
            frags.push(Fragment::Box(row));
        }
    }
    TableLayoutResult { frags, height: grid_bottom, width: used_width }
}
