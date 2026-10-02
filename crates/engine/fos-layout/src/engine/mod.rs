//! The layout engine: DOM + computed styles → box tree → fragment tree
//!
//! ```text
//! build_box_tree (box_tree.rs)   CSS box generation, anonymous boxes,
//!                                white space processing
//! layout_root    (block.rs)      block formatting, margin collapsing,
//!                                replaced elements, intrinsic sizes
//!                (inline.rs)     line breaking and alignment
//! FragmentTree   (fragment.rs)   positioned boxes and text for painting,
//!                                hit testing and script geometry
//! ```

pub mod block;
pub mod flex;
pub mod box_tree;
pub mod fonts;
pub mod fragment;
pub mod inline;

pub use block::LayoutCtx;
pub use box_tree::{build_box_tree, BoxKind, LayoutBox, Styler};
pub use fonts::{FontContext, FontMetrics, Glyph, ResolvedFont, ShapedWord};
pub use fragment::{BoxFragment, BoxFragmentKind, Fragment, FragmentTree, Rect, ReplacedPaint, TextFragment};

use fos_dom::{DomTree, NodeId};

/// Lay out the document rooted at element `root` in a viewport
pub fn layout_document<S: Styler>(tree: &DomTree, root: NodeId, styler: &mut S, fonts: &mut FontContext, viewport: (f32, f32)) -> FragmentTree {
    let Some(boxes) = build_box_tree(tree, root, styler) else {
        return FragmentTree { root: None, document_height: viewport.1, document_width: viewport.0 };
    };
    let mut ctx = LayoutCtx { fonts, viewport };
    let (frag, bottom) = block::layout_root(&mut ctx, &boxes);
    let document_height = bottom.max(frag.ink.bottom()).max(viewport.1);
    let document_width = frag.ink.right().max(viewport.0);
    FragmentTree { root: Some(frag), document_height, document_width }
}

#[cfg(test)]
mod tests;
