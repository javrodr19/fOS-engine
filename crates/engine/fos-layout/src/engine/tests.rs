use fos_css::style::{Style, StyleContext};
use fos_dom::{DomTree, NodeId};

use super::*;

/// Tag defaults plus `style` attributes
struct TestStyler;

impl Styler for TestStyler {
    fn style(&mut self, tree: &DomTree, node: NodeId, parent: &Style) -> Style {
        let el = tree.get(node).and_then(|n| n.as_element()).expect("element");
        let tag = tree.resolve(el.name.local).to_string();
        let ua = match tag.as_str() {
            "html" | "div" | "p" | "section" => "display: block",
            "body" => "display: block; margin: 8px",
            "ul" | "ol" => "display: block; padding-left: 40px",
            "li" => "display: list-item",
            "head" | "script" => "display: none",
            "ol li" => "",
            _ => "",
        };
        let mut decls = fos_css::parse_declarations(ua);
        decls.extend(fos_css::parse_declarations(tree.get_attribute(node, "style").unwrap_or("")));
        let refs: Vec<&fos_css::Declaration> = decls.iter().collect();
        let mut style = Style::inherit_from(parent);
        style.cascade(&refs, parent, &StyleContext::default(), &mut Default::default());
        style
    }

    /// `::before` takes its declarations from `data-before`
    fn pseudo(&mut self, tree: &DomTree, node: NodeId, pe: fos_dom::PseudoElement, style: &Style) -> Option<Style> {
        if pe != fos_dom::PseudoElement::Before {
            return None;
        }
        let decls = fos_css::parse_declarations(tree.get_attribute(node, "data-before")?);
        let refs: Vec<&fos_css::Declaration> = decls.iter().collect();
        let mut ps = Style::inherit_from(style);
        ps.cascade(&refs, style, &StyleContext::default(), &mut Default::default());
        Some(ps)
    }

    fn natural_size(&mut self, tree: &DomTree, node: NodeId) -> Option<(f32, f32)> {
        let v = tree.get_attribute(node, "data-natural")?;
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }
}

/// A tiny document builder: `el(tree, parent, tag, style)`
fn el(tree: &mut DomTree, parent: NodeId, tag: &str, style: &str) -> NodeId {
    let id = tree.create_element(tag);
    if !style.is_empty() {
        tree.set_attribute(id, "style", style);
    }
    tree.append_child(parent, id);
    id
}

fn text(tree: &mut DomTree, parent: NodeId, s: &str) {
    let t = tree.create_text(s);
    tree.append_child(parent, t);
}

/// html > body; returns (tree, html, body)
fn doc() -> (DomTree, NodeId, NodeId) {
    let mut tree = DomTree::new();
    let root = tree.root();
    let html = el(&mut tree, root, "html", "");
    let body = el(&mut tree, html, "body", "");
    (tree, html, body)
}

fn layout(tree: &DomTree, html: NodeId, width: f32) -> FragmentTree {
    let mut fonts = FontContext::default();
    layout_document(tree, html, &mut TestStyler, &mut fonts, (width, 600.0))
}

fn rect_of(t: &FragmentTree, node: NodeId) -> Rect {
    t.element_rects().into_iter().find(|(n, _)| *n == node).map(|(_, r)| r).unwrap_or_else(|| panic!("no box for {node:?}"))
}

fn rects_of(t: &FragmentTree, node: NodeId) -> Vec<Rect> {
    t.element_rects().into_iter().filter(|(n, _)| *n == node).map(|(_, r)| r).collect()
}

#[test]
fn block_widths_and_auto_margins() {
    let (mut tree, html, body) = doc();
    let full = el(&mut tree, body, "div", "height: 10px; padding: 0 5px; border-left: 3px solid");
    let centered = el(&mut tree, body, "div", "width: 200px; height: 10px; margin: 0 auto");
    let right = el(&mut tree, body, "div", "width: 100px; height: 10px; margin-left: auto");
    let sized = el(&mut tree, body, "div", "width: 50%; height: 10px; box-sizing: border-box; padding: 0 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, body), Rect::new(8.0, 8.0, 784.0, 40.0));
    assert_eq!(rect_of(&t, full), Rect::new(8.0, 8.0, 784.0, 10.0));
    assert_eq!(rect_of(&t, centered).x, 8.0 + (784.0 - 200.0) / 2.0);
    assert_eq!(rect_of(&t, right).x, 8.0 + 784.0 - 100.0);
    assert_eq!(rect_of(&t, sized).w, 392.0);
}

#[test]
fn min_and_max_width() {
    let (mut tree, html, body) = doc();
    let a = el(&mut tree, body, "div", "max-width: 300px; height: 1px");
    let b = el(&mut tree, body, "div", "width: 10px; min-width: 50px; height: 1px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, a).w, 300.0);
    assert_eq!(rect_of(&t, b).w, 50.0);
}

#[test]
fn sibling_margins_collapse() {
    let (mut tree, html, body) = doc();
    let a = el(&mut tree, body, "div", "height: 10px; margin-bottom: 20px");
    let b = el(&mut tree, body, "div", "height: 10px; margin-top: 30px");
    let c = el(&mut tree, body, "div", "height: 10px; margin-top: -10px");
    let t = layout(&tree, html, 800.0);
    let (ra, rb, rc) = (rect_of(&t, a), rect_of(&t, b), rect_of(&t, c));
    assert_eq!(rb.y - ra.bottom(), 30.0);
    assert_eq!(rc.y - rb.bottom(), -10.0);
}

#[test]
fn parent_and_child_margins_collapse() {
    let (mut tree, html, body) = doc();
    let outer = el(&mut tree, body, "div", "margin-top: 10px");
    let inner = el(&mut tree, outer, "div", "margin-top: 25px; height: 10px");
    let bordered = el(&mut tree, body, "div", "border-top: 1px solid");
    let inner2 = el(&mut tree, bordered, "div", "margin-top: 25px; height: 10px");
    let t = layout(&tree, html, 800.0);
    // The body's 8px, the outer's 10px and the inner's 25px are one margin
    assert_eq!(rect_of(&t, outer).y, 25.0);
    assert_eq!(rect_of(&t, inner).y, 25.0);
    assert_eq!(rect_of(&t, outer).h, 10.0);
    // A border keeps the child's margin inside
    let rb = rect_of(&t, bordered);
    assert_eq!(rect_of(&t, inner2).y, rb.y + 1.0 + 25.0);
}

#[test]
fn empty_blocks_collapse_through() {
    let (mut tree, html, body) = doc();
    let a = el(&mut tree, body, "div", "height: 10px; margin-bottom: 10px");
    el(&mut tree, body, "div", "margin: 15px 0");
    let b = el(&mut tree, body, "div", "height: 10px; margin-top: 5px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, b).y - rect_of(&t, a).bottom(), 15.0);
}

#[test]
fn percent_heights_need_a_definite_parent() {
    let (mut tree, html, body) = doc();
    tree.set_attribute(html, "style", "height: 100%");
    tree.set_attribute(body, "style", "height: 50%; margin: 0");
    let child = el(&mut tree, body, "div", "height: 50%");
    let auto = el(&mut tree, body, "div", "");
    let lost = el(&mut tree, auto, "div", "height: 50%");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, html).h, 600.0);
    assert_eq!(rect_of(&t, body).h, 300.0);
    assert_eq!(rect_of(&t, child).h, 150.0);
    assert_eq!(rect_of(&t, lost).h, 0.0);
}

#[test]
fn text_wraps_into_lines() {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "p", "");
    text(&mut tree, p, "The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.");
    let wide = layout(&tree, html, 2000.0);
    let narrow = layout(&tree, html, 150.0);
    let one = rect_of(&wide, p).h;
    let many = rect_of(&narrow, p).h;
    assert!(one > 0.0);
    assert!(many >= one * 4.0, "{many} vs {one}");
    // Every line fits
    narrow.for_each(|f| {
        if let Fragment::Text(t) = f {
            assert!(t.rect.right() <= 8.0 + 134.0 + 0.5, "{:?}", t.rect);
        }
    });
}

#[test]
fn white_space_collapses_and_pre_keeps_lines() {
    let (mut tree, html, body) = doc();
    let a = el(&mut tree, body, "div", "");
    text(&mut tree, a, "  one   \n  two  ");
    let b = el(&mut tree, body, "div", "white-space: pre");
    text(&mut tree, b, "one\ntwo\nthree");
    let t = layout(&tree, html, 800.0);
    let ha = rect_of(&t, a).h;
    assert!((rect_of(&t, b).h - 3.0 * ha).abs() < 0.5, "{} vs {}", rect_of(&t, b).h, ha);
    let mut words = Vec::new();
    t.for_each(|f| {
        if let Fragment::Text(t) = f {
            if t.node == a {
                words.push(t.words.len());
            }
        }
    });
    assert_eq!(words, vec![2]);
}

#[test]
fn br_breaks_lines_and_display_none_is_skipped() {
    let (mut tree, html, body) = doc();
    let one = el(&mut tree, body, "div", "");
    text(&mut tree, one, "a");
    let two = el(&mut tree, body, "div", "");
    text(&mut tree, two, "a");
    el(&mut tree, two, "br", "");
    text(&mut tree, two, "b");
    let hidden = el(&mut tree, two, "div", "display: none; height: 100px");
    let t = layout(&tree, html, 800.0);
    assert!((rect_of(&t, two).h - 2.0 * rect_of(&t, one).h).abs() < 0.5);
    assert!(rects_of(&t, hidden).is_empty());
}

#[test]
fn text_align_and_indent() {
    let (mut tree, html, body) = doc();
    let left = el(&mut tree, body, "div", "");
    text(&mut tree, left, "x");
    let center = el(&mut tree, body, "div", "text-align: center");
    text(&mut tree, center, "x");
    let right = el(&mut tree, body, "div", "text-align: right");
    text(&mut tree, right, "x");
    let indent = el(&mut tree, body, "div", "text-indent: 30px");
    text(&mut tree, indent, "x");
    let t = layout(&tree, html, 800.0);
    let xs: Vec<(NodeId, Rect)> = t.element_rects().into_iter().filter(|(n, _)| [left, center, right, indent].contains(n)).collect();
    let text_rect = |n: NodeId| xs.iter().filter(|(m, _)| *m == n).nth(1).map(|r| r.1).expect("text");
    let (l, c, r) = (text_rect(left), text_rect(center), text_rect(right));
    assert_eq!(l.x, 8.0);
    assert!((c.x + c.w / 2.0 - 400.0).abs() < 1.0, "{c:?}");
    assert!((r.right() - 792.0).abs() < 0.5, "{r:?}");
    assert_eq!(text_rect(indent).x, 38.0);
}

#[test]
fn inline_blocks_shrink_and_sit_on_the_line() {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "div", "");
    text(&mut tree, p, "before ");
    let ib = el(&mut tree, p, "span", "display: inline-block; padding: 2px; border: 1px solid");
    text(&mut tree, ib, "inside");
    text(&mut tree, p, " after");
    let t = layout(&tree, html, 800.0);
    let r = rect_of(&t, ib);
    assert!(r.w < 200.0 && r.w > 6.0, "{r:?}");
    assert!(r.x > 8.0);
    // One line: the container is about as tall as the inline-block
    assert!(rect_of(&t, p).h < r.h * 1.6, "{:?} {r:?}", rect_of(&t, p));
}

#[test]
fn inline_boxes_split_across_lines() {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "div", "width: 120px");
    let span = el(&mut tree, p, "span", "border: 1px solid; padding: 0 4px");
    text(&mut tree, span, "lorem ipsum dolor sit amet consectetur");
    let t = layout(&tree, html, 800.0);
    let parts: Vec<Rect> = t.element_rects().into_iter().filter(|(n, r)| *n == span && r.h > 0.0).map(|(_, r)| r).collect();
    // Part boxes plus text runs; at least three lines' worth of parts
    assert!(parts.len() >= 6, "{parts:?}");
}

#[test]
fn replaced_elements_size_from_natural_and_ratio() {
    let (mut tree, html, body) = doc();
    let img = el(&mut tree, body, "img", "");
    tree.set_attribute(img, "data-natural", "100x50");
    let wide = el(&mut tree, body, "img", "width: 200px");
    tree.set_attribute(wide, "data-natural", "100x50");
    let block = el(&mut tree, body, "img", "display: block; height: 10px");
    tree.set_attribute(block, "data-natural", "100x50");
    let canvas = el(&mut tree, body, "canvas", "");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, img).w, rect_of(&t, img).h), (100.0, 50.0));
    assert_eq!((rect_of(&t, wide).w, rect_of(&t, wide).h), (200.0, 100.0));
    assert_eq!((rect_of(&t, block).w, rect_of(&t, block).h), (20.0, 10.0));
    assert_eq!((rect_of(&t, canvas).w, rect_of(&t, canvas).h), (300.0, 150.0));
    // Images in a line sit on the baseline: the line is at least as tall
    assert!(rect_of(&t, block).y >= rect_of(&t, wide).bottom());
}

#[test]
fn anonymous_blocks_wrap_inline_runs() {
    let (mut tree, html, body) = doc();
    text(&mut tree, body, "first");
    let d = el(&mut tree, body, "div", "height: 20px");
    text(&mut tree, body, "second");
    let t = layout(&tree, html, 800.0);
    let rd = rect_of(&t, d);
    let mut texts = Vec::new();
    t.for_each(|f| {
        if let Fragment::Text(t) = f {
            texts.push(t.rect);
        }
    });
    assert_eq!(texts.len(), 2);
    assert!(texts[0].bottom() <= rd.y + 0.5);
    assert!(texts[1].y >= rd.bottom() - 0.5);
}

#[test]
fn list_items_get_markers() {
    let (mut tree, html, body) = doc();
    let ol = el(&mut tree, body, "ol", "list-style-type: decimal");
    for s in ["a", "b", "c"] {
        let li = el(&mut tree, ol, "li", "");
        text(&mut tree, li, s);
    }
    let t = layout(&tree, html, 800.0);
    let mut markers = Vec::new();
    fn walk(b: &BoxFragment, out: &mut Vec<f32>) {
        if let Some(m) = &b.marker {
            out.push(m.rect.x);
        }
        for c in &b.children {
            if let Fragment::Box(cb) = c {
                walk(cb, out);
            }
        }
    }
    walk(t.root.as_ref().unwrap(), &mut markers);
    assert_eq!(markers.len(), 3);
    // Outside: left of the content
    assert!(markers.iter().all(|&x| x < 48.0));
}

#[test]
fn hit_testing_finds_the_deepest_box() {
    let (mut tree, html, body) = doc();
    let outer = el(&mut tree, body, "div", "height: 100px; padding: 10px");
    let inner = el(&mut tree, outer, "div", "height: 20px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(t.hit_test(50.0, 25.0), Some(inner));
    assert_eq!(t.hit_test(50.0, 90.0), Some(outer));
    assert_eq!(t.hit_test(3.0, 3.0), Some(html));
}

#[test]
fn relative_positioning_offsets_without_moving_siblings() {
    let (mut tree, html, body) = doc();
    let a = el(&mut tree, body, "div", "height: 10px; position: relative; top: 5px; left: 7px");
    let b = el(&mut tree, body, "div", "height: 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, a).x, rect_of(&t, a).y), (15.0, 13.0));
    assert_eq!(rect_of(&t, b).y, 18.0);
}

#[test]
fn justify_fills_lines_but_not_the_last() {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "div", "width: 200px; text-align: justify");
    text(&mut tree, p, "aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv");
    let t = layout(&tree, html, 800.0);
    let mut lines = Vec::new();
    t.for_each(|f| {
        if let Fragment::Text(t) = f {
            lines.push(t.rect);
        }
    });
    assert!(lines.len() >= 2);
    for r in &lines[..lines.len() - 1] {
        assert!((r.right() - 208.0).abs() < 0.5, "{r:?}");
    }
}

#[test]
fn spaces_between_inline_elements_are_kept() {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "div", "");
    text(&mut tree, p, "by ");
    let a = el(&mut tree, p, "a", "");
    text(&mut tree, a, "user");
    text(&mut tree, p, " ");
    let span = el(&mut tree, p, "span", "");
    let b = el(&mut tree, span, "a", "");
    text(&mut tree, b, "age");
    let t = layout(&tree, html, 800.0);
    let mut runs = Vec::new();
    t.for_each(|f| {
        if let Fragment::Text(t) = f {
            runs.push((t.node, t.rect));
        }
    });
    eprintln!("{runs:?}");
    let ra = runs.iter().find(|r| r.0 == a).unwrap().1;
    let rb = runs.iter().find(|r| r.0 == b).unwrap().1;
    assert!(rb.x - ra.right() > 2.0, "{ra:?} {rb:?}");
}

fn boxes(t: &FragmentTree, ns: &[NodeId]) -> Vec<Rect> {
    ns.iter().map(|n| rect_of(t, *n)).collect()
}

#[test]
fn flex_row_grows_and_shrinks() {
    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; width: 600px; height: 50px");
    let a = el(&mut tree, row, "div", "flex: 1; height: 10px");
    let b = el(&mut tree, row, "div", "flex: 2");
    let c = el(&mut tree, row, "div", "width: 100px; flex-shrink: 0");
    let t = layout(&tree, html, 800.0);
    let r = boxes(&t, &[a, b, c]);
    assert_eq!((r[0].x, r[0].w), (8.0, 500.0 / 3.0));
    assert!((r[1].w - 1000.0 / 3.0).abs() < 0.01);
    assert_eq!((r[2].x, r[2].w), (508.0, 100.0));
    // Stretch fills the line unless a height is given
    assert_eq!((r[0].h, r[1].h, r[2].h), (10.0, 50.0, 50.0));

    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; width: 300px");
    let a = el(&mut tree, row, "div", "width: 200px; height: 1px");
    let b = el(&mut tree, row, "div", "width: 200px; height: 1px; flex-shrink: 3");
    let t = layout(&tree, html, 800.0);
    let r = boxes(&t, &[a, b]);
    // 100px overflow shared 1:3 (weighted by base size)
    assert_eq!((r[0].w, r[1].w), (175.0, 125.0));
}

#[test]
fn flex_justify_and_align() {
    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; width: 400px; height: 100px; justify-content: space-between; align-items: center");
    let a = el(&mut tree, row, "div", "width: 50px; height: 20px");
    let b = el(&mut tree, row, "div", "width: 50px; height: 40px");
    let c = el(&mut tree, row, "div", "width: 50px; height: 60px");
    let t = layout(&tree, html, 800.0);
    let r = boxes(&t, &[a, b, c]);
    assert_eq!((r[0].x, r[1].x, r[2].x), (8.0, 183.0, 358.0));
    assert_eq!((r[0].y, r[1].y, r[2].y), (8.0 + 40.0, 8.0 + 30.0, 8.0 + 20.0));

    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; width: 400px; justify-content: center");
    let a = el(&mut tree, row, "div", "width: 100px; height: 10px");
    let right = el(&mut tree, row, "div", "width: 50px; height: 10px; margin-left: auto");
    let t = layout(&tree, html, 800.0);
    // Auto margins win over justify-content
    assert_eq!(rect_of(&t, a).x, 8.0);
    assert_eq!(rect_of(&t, right).x, 358.0);
}

#[test]
fn flex_row_in_a_min_height_container_fills_it() {
    // A single row with an automatic height takes the container's min
    // height (less its padding): items stretch to it and center in it
    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; min-height: 300px; padding: 20px 0 30px; box-sizing: border-box");
    let a = el(&mut tree, row, "div", "width: 50px");
    let b = el(&mut tree, row, "div", "width: 50px; height: 50px; align-self: center");
    let capped = el(&mut tree, body, "div", "display: flex; max-height: 40px");
    let c = el(&mut tree, capped, "div", "width: 50px; height: 100px; align-self: flex-end");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, row).h, 300.0);
    assert_eq!((rect_of(&t, a).y, rect_of(&t, a).h), (28.0, 250.0));
    assert_eq!(rect_of(&t, b).y, 28.0 + 100.0);
    // A max height caps the row: the item overflows it from its end
    assert_eq!(rect_of(&t, c).y, rect_of(&t, capped).y + 40.0 - 100.0);
}

#[test]
fn flex_wraps_columns_and_reverses() {
    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; flex-wrap: wrap; width: 250px; gap: 10px");
    let items: Vec<NodeId> = (0..5).map(|_| el(&mut tree, row, "div", "width: 100px; height: 20px")).collect();
    let t = layout(&tree, html, 800.0);
    let r = boxes(&t, &items);
    assert_eq!((r[0].x, r[1].x, r[2].x), (8.0, 118.0, 8.0));
    assert_eq!((r[0].y, r[2].y, r[4].y), (8.0, 38.0, 68.0));
    assert_eq!(rect_of(&t, row).h, 80.0);

    let (mut tree, html, body) = doc();
    let col = el(&mut tree, body, "div", "display: flex; flex-direction: column; height: 200px");
    let a = el(&mut tree, col, "div", "height: 30px");
    let b = el(&mut tree, col, "div", "flex-grow: 1");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, a).w, rect_of(&t, b).y, rect_of(&t, b).h), (784.0, 38.0, 170.0));

    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; flex-direction: row-reverse; width: 300px");
    let a = el(&mut tree, row, "div", "width: 100px; height: 5px");
    let b = el(&mut tree, row, "div", "width: 100px; height: 5px; order: -1");
    let t = layout(&tree, html, 800.0);
    // order puts b first, which row-reverse places at the right
    assert_eq!((rect_of(&t, b).x, rect_of(&t, a).x), (208.0, 108.0));
}

#[test]
fn flex_items_from_text_and_inline_children() {
    let (mut tree, html, body) = doc();
    let nav = el(&mut tree, body, "div", "display: flex; gap: 20px");
    let a = el(&mut tree, nav, "a", "");
    text(&mut tree, a, "Home");
    let b = el(&mut tree, nav, "a", "");
    text(&mut tree, b, "About us");
    let t = layout(&tree, html, 800.0);
    let (ra, rb) = (rect_of(&t, a), rect_of(&t, b));
    // Side by side, each as wide as its text
    assert_eq!(ra.y, rb.y);
    assert!((rb.x - ra.right() - 20.0).abs() < 0.01, "{ra:?} {rb:?}");
    assert!(ra.w < 100.0);
}

#[test]
fn absolute_boxes_use_the_nearest_positioned_ancestor() {
    let (mut tree, html, body) = doc();
    let rel = el(&mut tree, body, "div", "position: relative; margin-top: 50px; height: 200px; padding: 10px; border: 2px solid");
    let tl = el(&mut tree, rel, "div", "position: absolute; top: 5px; left: 7px; width: 20px; height: 20px");
    let br = el(&mut tree, rel, "div", "position: absolute; bottom: 0; right: 0; width: 30px; height: 10px");
    let stretched = el(&mut tree, rel, "div", "position: absolute; top: 0; bottom: 0; left: 10%; right: 10%");
    let centered = el(&mut tree, rel, "div", "position: absolute; left: 0; right: 0; width: 100px; height: 10px; margin: 0 auto");
    let after = el(&mut tree, body, "div", "height: 10px");
    let t = layout(&tree, html, 800.0);
    let r = rect_of(&t, rel);
    let pad = Rect::new(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0);
    assert_eq!((rect_of(&t, tl).x, rect_of(&t, tl).y), (pad.x + 7.0, pad.y + 5.0));
    assert_eq!((rect_of(&t, br).right(), rect_of(&t, br).bottom()), (pad.right(), pad.bottom()));
    let s = rect_of(&t, stretched);
    assert_eq!((s.y, s.h), (pad.y, pad.h));
    assert!((s.w - pad.w * 0.8).abs() < 0.01);
    let c = rect_of(&t, centered);
    assert!((c.x - (pad.x + (pad.w - 100.0) / 2.0)).abs() < 0.01, "{c:?} {pad:?}");
    // Out of flow: the next block follows the container directly
    assert_eq!(rect_of(&t, after).y, r.bottom());
}

#[test]
fn absolute_boxes_without_insets_stay_at_their_static_position() {
    let (mut tree, html, body) = doc();
    el(&mut tree, body, "div", "height: 30px");
    let abs = el(&mut tree, body, "div", "position: absolute");
    text(&mut tree, abs, "menu");
    let next = el(&mut tree, body, "div", "height: 10px");
    let fixed = el(&mut tree, body, "div", "position: fixed; bottom: 0; left: 0; right: 0; height: 40px");
    let t = layout(&tree, html, 800.0);
    let a = rect_of(&t, abs);
    assert_eq!((a.x, a.y), (8.0, 38.0));
    // Shrink-to-fit
    assert!(a.w < 100.0 && a.w > 0.0);
    assert_eq!(rect_of(&t, next).y, 38.0);
    let f = rect_of(&t, fixed);
    assert_eq!((f.x, f.y, f.w), (0.0, 560.0, 800.0));
}

#[test]
fn balanced_text_evens_out_its_lines() {
    // Without balancing the last line holds one word; balanced, the lines
    // are about as long, and still centered in the full width
    let words = "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii";
    let line_widths = |balance: bool| {
        let (mut tree, html, body) = doc();
        let style = if balance { "width: 200px; font-size: 16px; text-align: center; text-wrap: balance" } else { "width: 200px; font-size: 16px; text-align: center" };
        let p = el(&mut tree, body, "p", style);
        text(&mut tree, p, words);
        let t = layout(&tree, html, 800.0);
        let mut lines: Vec<(f32, f32, f32)> = Vec::new();
        t.for_each(|f| {
            if let Fragment::Text(tf) = f {
                match lines.iter_mut().find(|l| (l.0 - tf.rect.y).abs() < 1.0) {
                    Some(l) => {
                        l.1 = l.1.min(tf.rect.x);
                        l.2 = l.2.max(tf.rect.x + tf.rect.w);
                    }
                    None => lines.push((tf.rect.y, tf.rect.x, tf.rect.x + tf.rect.w)),
                }
            }
        });
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        lines.into_iter().map(|l| (l.1, l.2 - l.1)).collect::<Vec<_>>()
    };
    let plain = line_widths(false);
    let balanced = line_widths(true);
    assert_eq!(plain.len(), balanced.len());
    assert!(plain.len() >= 2, "{plain:?}");
    let spread = |l: &[(f32, f32)]| l.iter().map(|x| x.1).fold(0.0, f32::max) - l.iter().map(|x| x.1).fold(f32::MAX, f32::min);
    assert!(spread(&balanced) < spread(&plain), "{plain:?} {balanced:?}");
    // Centered in the 200px box (which starts at 8px)
    for (x, w) in balanced {
        assert!((x + w / 2.0 - (8.0 + 100.0)).abs() < 1.0, "{x} {w}");
    }
}

#[test]
fn floats_sit_side_by_side_and_text_wraps_around_them() {
    let (mut tree, html, body) = doc();
    let c = el(&mut tree, body, "div", "width: 400px");
    let l = el(&mut tree, c, "div", "float: left; width: 100px; height: 50px");
    let r = el(&mut tree, c, "div", "float: right; width: 80px; height: 30px");
    let l2 = el(&mut tree, c, "div", "float: left; width: 100px; height: 20px");
    let p = el(&mut tree, c, "p", "margin: 0");
    text(&mut tree, p, "text beside the floats");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, l).x, rect_of(&t, l).y), (8.0, 8.0));
    assert_eq!((rect_of(&t, r).x, rect_of(&t, r).y), (328.0, 8.0));
    assert_eq!((rect_of(&t, l2).x, rect_of(&t, l2).y), (108.0, 8.0));
    // The paragraph's first line starts right of both left floats
    let first = rects_of(&t, p).into_iter().nth(1).expect("text");
    assert!((first.x - 208.0).abs() < 0.5, "{first:?}");
    assert!(first.right() <= 328.5);
    // The container does not grow to contain floats (not a formatting
    // context root)
    assert!(rect_of(&t, c).h < 50.0);
}

#[test]
fn clear_and_formatting_contexts_contain_floats() {
    let (mut tree, html, body) = doc();
    let c = el(&mut tree, body, "div", "overflow: hidden");
    el(&mut tree, c, "div", "float: left; width: 100px; height: 60px");
    let after = el(&mut tree, body, "div", "height: 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, c).h, 60.0);
    assert_eq!(rect_of(&t, after).y, 68.0);

    let (mut tree, html, body) = doc();
    el(&mut tree, body, "div", "float: left; width: 100px; height: 60px");
    let cleared = el(&mut tree, body, "div", "clear: both; height: 10px");
    let beside = el(&mut tree, body, "div", "float: right; width: 50px; height: 5px");
    let bfc = el(&mut tree, body, "div", "overflow: hidden; height: 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, cleared).y, 68.0);
    assert_eq!(rect_of(&t, beside).y, 78.0);
    // A formatting context root narrows beside the float
    let b = rect_of(&t, bfc);
    assert_eq!((b.y, b.w), (78.0, 734.0));
}

#[test]
fn tables_size_columns_from_content() {
    let (mut tree, html, body) = doc();
    let table = el(&mut tree, body, "table", "display: table; border-spacing: 2px");
    let tbody = el(&mut tree, table, "tbody", "display: table-row-group");
    let mut cells = Vec::new();
    for r in 0..3 {
        let tr = el(&mut tree, tbody, "tr", "display: table-row");
        for c in 0..2 {
            let td = el(&mut tree, tr, "td", "display: table-cell; padding: 1px");
            text(&mut tree, td, if c == 0 { "a" } else if r == 1 { "a much longer cell text" } else { "b" });
            cells.push(td);
        }
    }
    let t = layout(&tree, html, 800.0);
    let r = |i: usize| rect_of(&t, cells[i]);
    // Cells of a column line up and share its width; rows line up
    assert_eq!(r(0).x, r(2).x);
    assert_eq!(r(1).x, r(3).x);
    assert_eq!(r(1).w, r(3).w);
    assert!(r(1).w > r(0).w * 3.0, "{:?} {:?}", r(0), r(1));
    assert_eq!(r(0).y, r(1).y);
    assert!(r(2).y > r(0).bottom());
    // Spacing between and around cells; the table shrinks to fit
    assert_eq!(r(1).x - r(0).right(), 2.0);
    let tr = rect_of(&t, table);
    assert!((tr.right() - r(1).right() - 2.0).abs() < 0.01);
    assert!(tr.w < 400.0);
}

#[test]
fn table_captions_go_on_their_side() {
    // A figure laid out as a table, its caption below its image
    // (Wikipedia's thumbnails); a top caption stays above
    let (mut tree, html, body) = doc();
    let figure = el(&mut tree, body, "figure", "display: table; margin: 0");
    let image = el(&mut tree, figure, "div", "width: 100px; height: 50px");
    let below = el(&mut tree, figure, "figcaption", "display: table-caption; caption-side: bottom; height: 20px");
    let above = el(&mut tree, figure, "figcaption", "display: table-caption; height: 10px");
    let t = layout(&tree, html, 800.0);
    let (img, b, a) = (rect_of(&t, image), rect_of(&t, below), rect_of(&t, above));
    assert_eq!(a.y, 8.0);
    assert!(img.y >= a.bottom(), "{a:?} {img:?}");
    assert!(b.y >= img.bottom(), "{img:?} {b:?}");
    assert_eq!(rect_of(&t, figure).bottom(), b.bottom());
}

#[test]
fn floats_share_max_content_with_inline_content() {
    let (mut tree, html, body) = doc();
    // Text then a float: side by side in a shrink-to-fit float
    let a = el(&mut tree, body, "div", "float: right");
    let link = el(&mut tree, a, "a", "");
    text(&mut tree, link, "Donate");
    el(&mut tree, a, "div", "float: right; width: 300px; height: 50px");
    // A float beside a box that avoids floats, but above a plain block
    let b = el(&mut tree, body, "div", "float: left; clear: both");
    el(&mut tree, b, "div", "float: left; width: 100px; height: 20px");
    el(&mut tree, b, "div", "overflow: hidden; width: 50px; height: 20px");
    let c = el(&mut tree, body, "div", "float: left; clear: both");
    el(&mut tree, c, "div", "float: left; width: 100px; height: 20px");
    el(&mut tree, c, "div", "width: 50px; height: 20px");
    let t = layout(&tree, html, 800.0);
    let donate = rects_of(&t, link)[0].w;
    assert!((rect_of(&t, a).w - (300.0 + donate)).abs() < 0.5, "{:?}", rect_of(&t, a));
    assert_eq!(rect_of(&t, b).w, 150.0);
    assert_eq!(rect_of(&t, c).w, 100.0);
}

#[test]
fn justify_self_aligns_blocks_in_block_layout() {
    let (mut tree, html, body) = doc();
    let wrap = |tree: &mut DomTree, style: &str| el(tree, body, "div", &format!("width: 300px; {style}"));
    // Aligned blocks are as wide as their content and placed in the free
    // space; justify-items stands in for auto
    let p = wrap(&mut tree, "");
    let center = el(&mut tree, p, "div", "justify-self: center; width: 100px; height: 10px");
    let p = wrap(&mut tree, "justify-items: end");
    let end = el(&mut tree, p, "div", "width: 100px; height: 10px; margin-right: 20px");
    let p = wrap(&mut tree, "");
    let shrunk = el(&mut tree, p, "div", "justify-self: start; display: flex");
    el(&mut tree, shrunk, "div", "width: 40px; height: 10px");
    // Auto margins win; stretch fills
    let p = wrap(&mut tree, "");
    let auto = el(&mut tree, p, "div", "justify-self: left; margin: 0 auto; width: 100px; height: 10px");
    let p = wrap(&mut tree, "");
    let stretch = el(&mut tree, p, "div", "justify-self: stretch; height: 10px");
    let t = layout(&tree, html, 800.0);
    let x = |n| rect_of(&t, n).x - 8.0;
    assert_eq!((x(center), rect_of(&t, center).w), (100.0, 100.0));
    assert_eq!(x(end), 180.0);
    assert_eq!((x(shrunk), rect_of(&t, shrunk).w), (0.0, 40.0));
    assert_eq!(x(auto), 100.0);
    assert_eq!(rect_of(&t, stretch).w, 300.0);
}

#[test]
fn margins_collapsing_through_empty_boxes_move_what_follows() {
    let (mut tree, html, body) = doc();
    el(&mut tree, body, "div", "height: 10px");
    // An empty box's margins go through its parent's top, even when a
    // cleared box follows: the parent and its floats start below them
    let a = el(&mut tree, body, "div", "");
    let empty = el(&mut tree, a, "div", "margin: 20px 0");
    let float = el(&mut tree, a, "div", "float: left; width: 10px; height: 10px");
    let cleared = el(&mut tree, a, "div", "clear: both");
    // In a formatting context root they stay inside, above its floats
    let b = el(&mut tree, body, "div", "overflow: hidden");
    el(&mut tree, b, "div", "margin: 20px 0");
    let inner = el(&mut tree, b, "div", "float: left; width: 10px; height: 10px");
    let t = layout(&tree, html, 800.0);
    let y = |n| rect_of(&t, n).y - 8.0;
    assert_eq!((y(a), y(empty), y(float), y(cleared)), (30.0, 30.0, 30.0, 40.0));
    assert_eq!(y(inner) - y(b), 20.0);
}

#[test]
fn images_keep_their_ratio_when_a_given_size_is_clamped() {
    // width="1024" (a hint) and max-width: 100% in a 598px box, height
    // auto: the height follows the clamped width
    let (mut tree, html, body) = doc();
    let wrap = el(&mut tree, body, "div", "width: 598px");
    let img = el(&mut tree, wrap, "img", "width: 1024px; max-width: 100%; height: auto");
    tree.set_attribute(img, "data-natural", "1024x576");
    let tall = el(&mut tree, wrap, "img", "height: 576px; max-height: 288px; display: block");
    tree.set_attribute(tall, "data-natural", "1024x576");
    let t = layout(&tree, html, 800.0);
    let r = rect_of(&t, img);
    assert_eq!(r.w, 598.0);
    assert!((r.h - 598.0 * 576.0 / 1024.0).abs() < 0.01, "{r:?}");
    assert_eq!((rect_of(&t, tall).w, rect_of(&t, tall).h), (512.0, 288.0));
}

#[test]
fn percentage_sized_images_add_nothing_to_min_content() {
    // A wrapping flex row of two flex: 1 columns stays one line when one
    // holds a 1024px wide image (its width attribute's) that max-width: 95%
    // can shrink, or one that width: 50% sizes, but not a fixed one
    let (mut tree, html, body) = doc();
    let row = |tree: &mut DomTree, img_style: &str| {
        let r = el(tree, body, "div", "display: flex; flex-wrap: wrap; width: 600px");
        let a = el(tree, r, "div", "flex: 1");
        let img = el(tree, a, "img", img_style);
        tree.set_attribute(img, "data-natural", "1024x10");
        let b = el(tree, r, "div", "flex: 1; height: 10px");
        (a, b)
    };
    let max = row(&mut tree, "width: 1024px; max-width: 95%");
    let pct = row(&mut tree, "width: 50%");
    let fixed = row(&mut tree, "width: 1024px");
    let t = layout(&tree, html, 800.0);
    for (a, b) in [max, pct] {
        assert_eq!((rect_of(&t, a).w, rect_of(&t, b).y), (300.0, rect_of(&t, a).y));
    }
    assert!(rect_of(&t, fixed.1).y > rect_of(&t, fixed.0).y);
}

#[test]
fn trailing_spaces_in_inline_boxes_take_no_room() {
    let (mut tree, html, body) = doc();
    // `<b>x </b> ` ends its line with a space inside the box and one after
    // it: neither widens the shrink-to-fit float
    let a = el(&mut tree, body, "div", "float: left");
    let b = el(&mut tree, a, "b", "");
    text(&mut tree, b, "x ");
    text(&mut tree, a, " ");
    // Followed by more on the line, the space stays inside its box: before
    // the box's padding
    let c = el(&mut tree, body, "div", "clear: both");
    let tag = el(&mut tree, c, "span", "padding: 0 5px");
    text(&mut tree, tag, "x ");
    text(&mut tree, c, " y ");
    let bare = el(&mut tree, c, "span", "padding: 0 5px");
    text(&mut tree, bare, "x");
    let t = layout(&tree, html, 800.0);
    assert!((rect_of(&t, a).w - rects_of(&t, b)[0].w).abs() < 0.01, "{:?} {:?}", rect_of(&t, a), rects_of(&t, b));
    let space = rects_of(&t, tag)[0].w - rects_of(&t, bare)[0].w;
    assert!(space > 2.0 && space < 6.0, "{space}");
}

#[test]
fn absolute_boxes_stay_in_their_line() {
    let (mut tree, html, body) = doc();
    // An absolutely positioned label between inline blocks does not
    // split their line, and the spaces around it collapse to one
    let p = el(&mut tree, body, "div", "float: left");
    let a = el(&mut tree, p, "span", "display: inline-block; width: 40px; height: 10px");
    text(&mut tree, p, " ");
    let label = el(&mut tree, p, "label", "position: absolute");
    text(&mut tree, label, "L");
    text(&mut tree, p, " ");
    let b = el(&mut tree, p, "span", "display: inline-block; width: 100px; height: 10px");
    // A block-level one starts below its line
    let q = el(&mut tree, body, "div", "clear: both");
    text(&mut tree, q, "words ");
    let block = el(&mut tree, q, "div", "position: absolute");
    text(&mut tree, block, "B");
    text(&mut tree, q, " more");
    let t = layout(&tree, html, 800.0);
    let (ra, rb, rl) = (rect_of(&t, a), rect_of(&t, b), rect_of(&t, label));
    assert_eq!(ra.y, rb.y);
    let gap = rb.x - ra.right();
    assert!(gap > 2.0 && gap < 8.0, "one space between: {gap}");
    assert!((rl.x - (ra.right() + gap)).abs() < 0.5 && rl.y <= ra.y, "{rl:?}");
    assert!((rect_of(&t, p).w - (140.0 + gap)).abs() < 0.5);
    let (rq, rbl) = (rect_of(&t, q), rect_of(&t, block));
    assert_eq!(rbl.x, rq.x);
    assert!(rbl.y >= rq.y + rq.h - 0.5, "{rbl:?} {rq:?}");
}

#[test]
fn relative_inline_boxes_move_with_their_text() {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "div", "");
    text(&mut tree, p, "before ");
    let span = el(&mut tree, p, "span", "position: relative; top: 10px; left: 5px; padding: 2px");
    text(&mut tree, span, "moved");
    text(&mut tree, p, " after");
    let still = el(&mut tree, body, "div", "");
    text(&mut tree, still, "before ");
    let plain = el(&mut tree, still, "span", "padding: 2px");
    text(&mut tree, plain, "moved");
    text(&mut tree, still, " after");
    let t = layout(&tree, html, 800.0);
    let (a, b) = (rect_of(&t, span), rect_of(&t, plain));
    assert_eq!((a.x - b.x, a.y - (b.y - rect_of(&t, still).y + rect_of(&t, p).y)), (5.0, 10.0));
    // The line is laid out as if it had not moved
    assert_eq!(rect_of(&t, p).h, rect_of(&t, still).h);
}

#[test]
fn cells_align_their_content_by_its_height() {
    // A cell's height is a minimum: content taller than it is centered in
    // the row by its own height, and a tall cell centers short content
    let (mut tree, html, body) = doc();
    let table = el(&mut tree, body, "table", "display: table; border-spacing: 0");
    let tr = el(&mut tree, table, "tr", "display: table-row");
    let short = el(&mut tree, tr, "td", "display: table-cell; height: 4px; padding: 0; vertical-align: middle; line-height: 16px");
    let a = el(&mut tree, short, "span", "");
    text(&mut tree, a, "a");
    let tall = el(&mut tree, tr, "td", "display: table-cell; height: 40px; padding: 0; vertical-align: middle; line-height: 20px");
    let b = el(&mut tree, tall, "span", "");
    text(&mut tree, b, "b");
    let t = layout(&tree, html, 800.0);
    let row = rect_of(&t, tr);
    assert_eq!(row.h, 40.0);
    assert_eq!((rect_of(&t, short).h, rect_of(&t, tall).h), (40.0, 40.0));
    // 16px of line centered in 40 (12 above), 20px of line likewise (10)
    let (ra, rb) = (rect_of(&t, a), rect_of(&t, b));
    assert!(ra.y - row.y > 12.0 - 2.0 && ra.y - row.y < 12.0 + 4.0, "{ra:?} {row:?}");
    assert!(rb.y - row.y > 10.0 - 2.0 && rb.y - row.y < 10.0 + 4.0, "{rb:?} {row:?}");
}

/// [`TestStyler`] for a document in quirks mode
struct QuirksStyler;

impl Styler for QuirksStyler {
    fn style(&mut self, tree: &DomTree, node: NodeId, parent: &Style) -> Style {
        TestStyler.style(tree, node, parent)
    }

    fn quirks(&self) -> bool {
        true
    }
}

#[test]
fn quirks_mode_line_height() {
    let page = || {
        let (mut tree, html, body) = doc();
        // Text only inside a span: the block's own line height (its strut)
        // does not count in quirks mode
        let p = el(&mut tree, body, "div", "line-height: 40px");
        let span = el(&mut tree, p, "span", "line-height: 10px");
        text(&mut tree, span, "x");
        // Nor around a lone inline block (an image in a table cell)
        let q = el(&mut tree, body, "div", "line-height: 40px");
        el(&mut tree, q, "span", "display: inline-block; width: 5px; height: 12px");
        // Text directly in the block keeps it
        let r = el(&mut tree, body, "div", "line-height: 40px");
        text(&mut tree, r, "y");
        (tree, html, p, q, r)
    };
    let (tree, html, p, q, r) = page();
    let mut fonts = FontContext::default();
    let quirks = layout_document(&tree, html, &mut QuirksStyler, &mut fonts, (800.0, 600.0));
    assert_eq!(rect_of(&quirks, p).h, 10.0);
    assert!(rect_of(&quirks, q).h <= 13.0, "{:?}", rect_of(&quirks, q));
    assert_eq!(rect_of(&quirks, r).h, 40.0);
    let standard = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&standard, p).h, rect_of(&standard, q).h, rect_of(&standard, r).h), (40.0, 40.0, 40.0));
}

#[test]
fn vertical_align_offsets_as_blink() {
    // Spans in their parent's font and line height: sub and super move by
    // a fifth and a third of the font size and a pixel; middle puts the
    // box's middle half an x-height above the baseline
    let (mut tree, html, body) = doc();
    let line = |tree: &mut DomTree, va: &str| {
        let p = el(tree, body, "div", "font-size: 16px; line-height: 20px");
        let base = el(tree, p, "span", "");
        text(tree, base, "x");
        let s = el(tree, p, "span", &format!("vertical-align: {va}"));
        text(tree, s, "x");
        (base, s)
    };
    let sub = line(&mut tree, "sub");
    let sup = line(&mut tree, "super");
    let mid = line(&mut tree, "middle");
    let t = layout(&tree, html, 800.0);
    let shift = |(p, s): (NodeId, NodeId)| rects_of(&t, s)[0].y - rects_of(&t, p)[0].y;
    assert!((shift(sub) - 4.2).abs() < 0.01, "{}", shift(sub));
    assert!((shift(sup) + 16.0 / 3.0 + 1.0).abs() < 0.01, "{}", shift(sup));
    let mut fonts = FontContext::default();
    let mut style = Style::default();
    std::sync::Arc::make_mut(&mut style.inherited).font_size = 16.0;
    let font = fonts.resolve(&style);
    let (a, d) = (font.ascent(), font.descent());
    let above = ((20.0 - a - d) / 2.0).floor();
    let expected = (a + above - (20.0 - a - above)) / 2.0 - font.x_height() / 2.0;
    assert!((shift(mid) - expected).abs() < 0.01, "{} {expected}", shift(mid));
}

#[test]
fn inline_blocks_sit_on_their_line_boxes_baseline() {
    // An inline block whose text is moved by vertical-align still has its
    // line box's baseline: beside the same text, it adds no height
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "div", "font-size: 16px; line-height: 23px");
    text(&mut tree, p, "x ");
    let ib = el(&mut tree, p, "span", "display: inline-block");
    let tag = el(&mut tree, ib, "span", "font-size: 10px; vertical-align: middle");
    text(&mut tree, tag, "tag");
    let t = layout(&tree, html, 800.0);
    assert!((rect_of(&t, p).h - rect_of(&t, ib).h).abs() < 0.01, "{:?} {:?}", rect_of(&t, p), rect_of(&t, ib));
}

#[test]
fn flex_items_shrink_by_their_inner_base_sizes() {
    // Two 400px items in 400px: the padded one's inner base is 300px, so
    // it gives up 300/700 of the 400px overflow, the other 400/700
    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; width: 400px");
    let plain = el(&mut tree, row, "div", "width: 400px; box-sizing: border-box; height: 5px");
    let padded = el(&mut tree, row, "div", "width: 400px; box-sizing: border-box; padding-left: 100px; height: 5px");
    let t = layout(&tree, html, 800.0);
    let (a, b) = (rect_of(&t, plain).w, rect_of(&t, padded).w);
    assert!((a - (400.0 - 400.0 * 400.0 / 700.0)).abs() < 0.01, "{a}");
    assert!((b - (400.0 - 400.0 * 300.0 / 700.0)).abs() < 0.01, "{b}");
}

#[test]
fn images_keep_their_ratio_under_two_limits() {
    // Too wide and too tall: the tighter limit sets both sizes
    let (mut tree, html, body) = doc();
    let tall = el(&mut tree, body, "img", "display: block; max-width: 150px; max-height: 64px");
    tree.set_attribute(tall, "data-natural", "2119x2500");
    let wide = el(&mut tree, body, "img", "display: block; max-width: 150px; max-height: 64px");
    tree.set_attribute(wide, "data-natural", "3000x400");
    let t = layout(&tree, html, 800.0);
    let (a, b) = (rect_of(&t, tall), rect_of(&t, wide));
    assert_eq!(a.h, 64.0);
    assert!((a.w - 64.0 * 2119.0 / 2500.0).abs() < 0.01, "{a:?}");
    assert_eq!(b.w, 150.0);
    assert!((b.h - 20.0).abs() < 0.01, "{b:?}");
}

#[test]
fn intrinsic_size_keywords() {
    let (mut tree, html, body) = doc();
    let block = |tree: &mut DomTree, style: &str, w: f32| {
        let d = el(tree, body, "div", style);
        el(tree, d, "span", &format!("display: inline-block; width: {w}px; height: 5px"));
        text(tree, d, " ");
        el(tree, d, "span", "display: inline-block; width: 30px; height: 5px");
        d
    };
    let min = block(&mut tree, "width: min-content", 50.0);
    let max = block(&mut tree, "width: max-content", 50.0);
    // (two 500px items: wider than the container together, not alone)
    let fit = el(&mut tree, body, "div", "width: fit-content");
    el(&mut tree, fit, "span", "display: inline-block; width: 500px; height: 5px");
    text(&mut tree, fit, " ");
    el(&mut tree, fit, "span", "display: inline-block; width: 500px; height: 5px");
    let floor = block(&mut tree, "width: 10px; min-width: max-content", 50.0);
    let cap = block(&mut tree, "max-width: min-content", 50.0);
    let t = layout(&tree, html, 800.0);
    let space = rect_of(&t, max).w - 80.0;
    assert_eq!(rect_of(&t, min).w, 50.0);
    assert!(space > 2.0 && space < 6.0, "{space}");
    assert_eq!(rect_of(&t, fit).w, 784.0);
    assert_eq!(rect_of(&t, floor).w, 80.0 + space);
    assert_eq!(rect_of(&t, cap).w, 50.0);
}

#[test]
fn spanning_cells_widen_columns_by_their_max_widths() {
    // A cell across both columns needs 400px: the 20px and 60px columns
    // grow in proportion, to 100px and 300px
    let (mut tree, html, body) = doc();
    let table = el(&mut tree, body, "table", "display: table; width: max-content; border-spacing: 0");
    let r1 = el(&mut tree, table, "tr", "display: table-row");
    let a = el(&mut tree, r1, "td", "display: table-cell; padding: 0");
    el(&mut tree, a, "span", "display: inline-block; width: 20px; height: 5px");
    let b = el(&mut tree, r1, "td", "display: table-cell; padding: 0");
    el(&mut tree, b, "span", "display: inline-block; width: 60px; height: 5px");
    let r2 = el(&mut tree, table, "tr", "display: table-row");
    let wide = el(&mut tree, r2, "td", "display: table-cell; padding: 0");
    tree.set_attribute(wide, "colspan", "2");
    el(&mut tree, wide, "span", "display: inline-block; width: 400px; height: 5px");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, a).w, rect_of(&t, b).w), (100.0, 300.0));
}

#[test]
fn table_extra_width_goes_to_auto_columns() {
    // A 600px table: the fixed column keeps its width, the percentage one
    // gets its share, and the auto one the rest
    let (mut tree, html, body) = doc();
    let table = el(&mut tree, body, "table", "display: table; width: 600px; border-spacing: 0");
    let tr = el(&mut tree, table, "tr", "display: table-row");
    let fixed = el(&mut tree, tr, "td", "display: table-cell; width: 18px; padding: 0 4px 0 0");
    text(&mut tree, fixed, "Y");
    let auto = el(&mut tree, tr, "td", "display: table-cell; padding: 0");
    text(&mut tree, auto, "Hacker News");
    let pct = el(&mut tree, tr, "td", "display: table-cell; width: 25%; padding: 0");
    text(&mut tree, pct, "login");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, fixed).w, 22.0);
    assert_eq!(rect_of(&t, pct).w, 150.0);
    assert_eq!(rect_of(&t, auto).w, 600.0 - 22.0 - 150.0);
}

#[test]
fn table_spans_and_widths() {
    let (mut tree, html, body) = doc();
    let table = el(&mut tree, body, "table", "display: table; width: 400px; border-spacing: 0");
    let tr1 = el(&mut tree, table, "tr", "display: table-row");
    let wide = el(&mut tree, tr1, "td", "display: table-cell");
    tree.set_attribute(wide, "colspan", "2");
    let tall = el(&mut tree, tr1, "td", "display: table-cell; vertical-align: middle");
    tree.set_attribute(tall, "rowspan", "2");
    text(&mut tree, tall, "x");
    let tr2 = el(&mut tree, table, "tr", "display: table-row");
    let a = el(&mut tree, tr2, "td", "display: table-cell; height: 50px");
    let b = el(&mut tree, tr2, "td", "display: table-cell");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, table).w, 400.0);
    let (rw, ra, rb, rt) = (rect_of(&t, wide), rect_of(&t, a), rect_of(&t, b), rect_of(&t, tall));
    assert_eq!(rw.x, ra.x);
    assert!((rw.right() - rb.right()).abs() < 0.01);
    assert_eq!(rt.x, rb.right());
    // The row-spanning cell covers both rows
    assert!((rt.bottom() - ra.bottom()).abs() < 0.01 && rt.y == rw.y);
    assert_eq!(ra.h, 50.0);
}

#[test]
fn collapsed_borders_are_shared() {
    let (mut tree, html, body) = doc();
    let table = el(&mut tree, body, "table", "display: table; border-collapse: collapse; border: 1px solid black");
    let mut cells = Vec::new();
    for _ in 0..2 {
        let tr = el(&mut tree, table, "tr", "display: table-row");
        for _ in 0..2 {
            let td = el(&mut tree, tr, "td", "display: table-cell; border: 1px solid black; width: 20px; height: 10px");
            cells.push(td);
        }
    }
    let t = layout(&tree, html, 800.0);
    let r = |i: usize| rect_of(&t, cells[i]);
    // Neighbours overlap by one border; the outer cells cover the table's
    assert_eq!(r(1).x, r(0).right() - 1.0);
    assert_eq!(r(2).y, r(0).bottom() - 1.0);
    let tb = rect_of(&t, table);
    assert_eq!((r(0).x, r(0).y), (tb.x, tb.y));
    assert_eq!((r(3).right(), r(3).bottom()), (tb.right(), tb.bottom()));
}

#[test]
fn webkit_center_centers_block_children() {
    let (mut tree, html, body) = doc();
    let c = el(&mut tree, body, "div", "text-align: -webkit-center");
    let child = el(&mut tree, c, "div", "width: 200px; height: 5px; text-align: left");
    let plain = el(&mut tree, body, "div", "text-align: center");
    let child2 = el(&mut tree, plain, "div", "width: 200px; height: 5px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, child).x, 8.0 + 292.0);
    assert_eq!(rect_of(&t, child2).x, 8.0);
}

#[test]
fn grid_tracks_and_auto_placement() {
    let (mut tree, html, body) = doc();
    let g = el(&mut tree, body, "div", "display: grid; width: 600px; grid-template-columns: 100px 1fr 2fr; gap: 10px 20px");
    let items: Vec<NodeId> = (0..5).map(|_| el(&mut tree, g, "div", "height: 30px")).collect();
    let t = layout(&tree, html, 800.0);
    let r: Vec<Rect> = items.iter().map(|&n| rect_of(&t, n)).collect();
    // 600 - 100 - 2*20 = 460 shared 1:2
    assert_eq!((r[0].x, r[0].w), (8.0, 100.0));
    assert!((r[1].x - 128.0).abs() < 0.01 && (r[1].w - 460.0 / 3.0).abs() < 0.01, "{:?}", r[1]);
    assert!((r[2].w - 920.0 / 3.0).abs() < 0.01);
    // Second row after the gap
    assert_eq!((r[3].x, r[3].y), (8.0, 8.0 + 30.0 + 10.0));
    assert_eq!(rect_of(&t, g).h, 70.0);
}

#[test]
fn grid_areas_lines_and_spans() {
    let (mut tree, html, body) = doc();
    let g = el(&mut tree, body, "div", "display: grid; width: 400px; grid-template-columns: 100px 1fr; grid-template-rows: 50px auto 20px; grid-template-areas: 'head head' 'nav main' 'foot foot'");
    let main = el(&mut tree, g, "div", "grid-area: main; height: 80px");
    let head = el(&mut tree, g, "div", "grid-area: head");
    let nav = el(&mut tree, g, "div", "grid-area: nav");
    let foot = el(&mut tree, g, "div", "grid-column: 1 / -1; grid-row: 3");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, head), Rect::new(8.0, 8.0, 400.0, 50.0));
    assert_eq!(rect_of(&t, main), Rect::new(108.0, 58.0, 300.0, 80.0));
    // Stretched to the row that main made 80px tall
    assert_eq!(rect_of(&t, nav), Rect::new(8.0, 58.0, 100.0, 80.0));
    assert_eq!(rect_of(&t, foot), Rect::new(8.0, 138.0, 400.0, 20.0));
}

#[test]
fn shrink_wrapped_grid_of_flexible_columns_fits_its_content() {
    // A grid of minmax(0, 1fr) columns, shrink-wrapped (a centered flex
    // column's item), is as wide as an item spanning them needs
    let (mut tree, html, body) = doc();
    let col = el(&mut tree, body, "div", "display: flex; flex-direction: column; align-items: center");
    let grid = el(&mut tree, col, "div", "display: grid; grid-template-columns: repeat(4, minmax(0, 1fr))");
    let item = el(&mut tree, grid, "div", "grid-column: span 4; width: 200px; height: 10px");
    let single = el(&mut tree, col, "div", "display: grid; grid-template-columns: 1fr 2fr");
    let a = el(&mut tree, single, "div", "width: 30px; height: 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, grid).w, 200.0);
    assert_eq!(rect_of(&t, item).w, 200.0);
    // One item per track: each fr is what the largest needs per fr
    assert_eq!(rect_of(&t, single).w, 90.0);
    assert_eq!(rect_of(&t, a).w, 30.0);
}

#[test]
fn grid_item_spanning_a_flexible_row_leaves_it_the_space() {
    // A card: a fixed height, an image spanning every row, a 50px row and
    // a flexible row for the body. The spanning image does not inflate
    // the auto rows; the body's row takes what is left.
    let (mut tree, html, body) = doc();
    let g = el(&mut tree, body, "div", "display: grid; width: 200px; height: 300px; grid-template-rows: auto auto minmax(0, 1fr) auto");
    let media = el(&mut tree, g, "div", "grid-row: 1 / -1; grid-column: 1; height: 300px");
    let safe = el(&mut tree, g, "div", "grid-row: 2; grid-column: 1; height: 50px");
    let card_body = el(&mut tree, g, "div", "grid-row: 3; grid-column: 1");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, media), Rect::new(8.0, 8.0, 200.0, 300.0));
    assert_eq!(rect_of(&t, safe), Rect::new(8.0, 8.0, 200.0, 50.0));
    assert_eq!(rect_of(&t, card_body), Rect::new(8.0, 58.0, 200.0, 250.0));
}

#[test]
fn grid_auto_fill_and_alignment() {
    let (mut tree, html, body) = doc();
    let g = el(&mut tree, body, "div", "display: grid; width: 500px; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); justify-items: center");
    let items: Vec<NodeId> = (0..5).map(|_| el(&mut tree, g, "div", "width: 50px; height: 10px")).collect();
    let t = layout(&tree, html, 800.0);
    // Four 125px columns; the fifth item wraps
    let r0 = rect_of(&t, items[0]);
    assert_eq!(r0.x, 8.0 + (125.0 - 50.0) / 2.0);
    assert_eq!(rect_of(&t, items[4]).y, 18.0);
    assert_eq!(rect_of(&t, items[3]).x, 8.0 + 375.0 + 37.5);
}

#[test]
fn grid_spans_and_dense_flow() {
    let (mut tree, html, body) = doc();
    let g = el(&mut tree, body, "div", "display: grid; width: 300px; grid-template-columns: repeat(3, 1fr); grid-auto-rows: 10px; grid-auto-flow: row dense");
    let a = el(&mut tree, g, "div", "grid-column: span 2");
    let b = el(&mut tree, g, "div", "grid-column: span 2");
    let c = el(&mut tree, g, "div", "");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, a).x, rect_of(&t, a).w), (8.0, 200.0));
    assert_eq!(rect_of(&t, b).y, 18.0);
    // Dense: c fills the hole after a
    assert_eq!((rect_of(&t, c).x, rect_of(&t, c).y), (208.0, 8.0));
}

#[test]
fn floats_in_a_flex_item_sit_side_by_side() {
    let (mut tree, html, body) = doc();
    let flex = el(&mut tree, body, "div", "display: flex; justify-content: flex-end; width: 600px");
    el(&mut tree, flex, "div", "display: block; width: 100px; height: 10px");
    let item = el(&mut tree, flex, "div", "display: block");
    let ul = el(&mut tree, item, "ul", "display: inline; margin: 0; padding: 0");
    let mut lis = Vec::new();
    for _ in 0..3 {
        text(&mut tree, ul, " ");
        let li = el(&mut tree, ul, "li", "display: list-item; float: left; margin-right: 10px");
        let b = el(&mut tree, li, "a", "display: inline-block; width: 50px; height: 20px");
        let _ = b;
        lis.push(li);
    }
    let t = layout(&tree, html, 800.0);
    let r: Vec<Rect> = lis.iter().map(|&l| rect_of(&t, l)).collect();
    // The item is a formatting context: it holds its floats
    let u = rect_of(&t, item);
    assert_eq!(r[0].y, u.y, "{u:?} {r:?}");
    assert!(u.h >= r[0].h);
    assert_eq!(r[0].y, r[1].y);
    assert_eq!(r[1].y, r[2].y);
    assert!(r[1].x > r[0].x && r[2].x > r[1].x);
}


/// All inline text of a box tree, block by block
fn box_texts(b: &LayoutBox, out: &mut Vec<String>) {
    match &b.kind {
        BoxKind::Inline(c) => out.push(c.text.trim().to_string()),
        BoxKind::Block(kids) | BoxKind::Flex(kids) | BoxKind::Grid(kids) => kids.iter().for_each(|k| box_texts(k, out)),
        _ => {}
    }
}

#[test]
fn counters_number_generated_content() {
    let (mut tree, html, body) = doc();
    // Nested lists numbered with counters(): ol { counter-reset: item }
    // li { counter-increment: item } li::before { counters(item, ".") }
    let list = |tree: &mut DomTree, parent: NodeId| {
        let ol = el(tree, parent, "div", "counter-reset: item");
        let mut items = Vec::new();
        for _ in 0..2 {
            let li = el(tree, ol, "div", "counter-increment: item");
            tree.set_attribute(li, "data-before", r#"content: counters(item, ".") " " counter(item, lower-alpha)"#);
            items.push(li);
        }
        items
    };
    let items = list(&mut tree, body);
    list(&mut tree, items[0]);
    let after = el(&mut tree, body, "p", "");
    tree.set_attribute(after, "data-before", r#"content: "[" counter(item) "]""#);
    // A ::before that increments its own counter, and list-item
    let ol = el(&mut tree, body, "ol", "");
    tree.set_attribute(ol, "start", "5");
    for _ in 0..2 {
        let li = el(&mut tree, ol, "li", "list-style-type: none");
        tree.set_attribute(li, "data-before", r#"content: counter(list-item) "/" counter(n); counter-increment: n 2"#);
    }
    let mut builder = TestStyler;
    let root = build_box_tree(&tree, html, &mut builder).unwrap();
    let mut texts = Vec::new();
    box_texts(&root, &mut texts);
    texts.retain(|t| !t.is_empty());
    // The inner list nests a new instance; it ends with its element, and
    // the outer list's counter stays in scope for the following sibling
    assert_eq!(texts, ["1 a", "1.1 a", "1.2 b", "2 b", "[2]", "5/2", "6/2"]);
}

/// The glyph counts of the laid-out words, left to right, and the
/// leftmost word's x
fn visual_words(t: &FragmentTree) -> (Vec<usize>, f32) {
    let mut words: Vec<(f32, usize)> = Vec::new();
    t.for_each(|f| {
        if let Fragment::Text(tf) = f {
            words.extend(tf.words.iter().map(|(w, x)| (*x, w.glyphs.len())));
        }
    });
    words.sort_by(|a, b| a.0.total_cmp(&b.0));
    (words.iter().map(|w| w.1).collect(), words.first().map_or(0.0, |w| w.0))
}

/// Whether the system has faces for Hebrew and Arabic (the bidi tests
/// count glyphs)
fn has_rtl_fonts() -> bool {
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "p", "display: block");
    text(&mut tree, p, "\u{5D0} \u{627}");
    visual_words(&layout(&tree, html, 600.0)).0.iter().all(|&g| g > 0)
}

#[test]
fn right_to_left_text_is_reordered() {
    if !has_rtl_fonts() {
        return;
    }
    // A right-to-left paragraph (2-, 4- and 5-glyph words): the first
    // word is rightmost; the number stays left to right
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "p", "direction: rtl; display: block");
    text(&mut tree, p, "\u{5D0}\u{5D1} \u{5D3}\u{5D4}\u{5D5}\u{5D6} 12345");
    let (glyphs, left) = visual_words(&layout(&tree, html, 600.0));
    assert_eq!(glyphs, [5, 4, 2]);
    // Right-aligned (text-align: start)
    assert!(left > 300.0, "{left}");

    // Hebrew inside English: only the Hebrew run is reversed
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "p", "display: block");
    text(&mut tree, p, "abc \u{5D0}\u{5D1} \u{5D3}\u{5D4}\u{5D5}\u{5D6}\u{5D7} xyzwvu");
    let (glyphs, left) = visual_words(&layout(&tree, html, 600.0));
    assert_eq!(glyphs, [3, 5, 2, 6]);
    assert!(left < 20.0, "{left}");
}

#[test]
fn numbers_and_punctuation_split_from_right_to_left_words() {
    if !has_rtl_fonts() {
        return;
    }
    // "2026." ending Arabic text: the period goes to the number's left
    let (mut tree, html, body) = doc();
    let p = el(&mut tree, body, "p", "direction: rtl; display: block");
    text(&mut tree, p, "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64A} 2026.");
    let (glyphs, _) = visual_words(&layout(&tree, html, 600.0));
    assert_eq!(glyphs, [1, 4, 6]);
}

#[test]
fn right_to_left_blocks_flex_and_grid_mirror() {
    let (mut tree, html, body) = doc();
    let rtl = el(&mut tree, body, "div", "direction: rtl; display: block; width: 600px");
    let fixed = el(&mut tree, rtl, "div", "display: block; width: 100px; height: 10px");
    let flex = el(&mut tree, rtl, "div", "display: flex");
    let f1 = el(&mut tree, flex, "div", "width: 100px; height: 10px");
    let f2 = el(&mut tree, flex, "div", "width: 50px; height: 10px");
    let grid = el(&mut tree, rtl, "div", "display: grid; grid-template-columns: 200px 1fr");
    let g1 = el(&mut tree, grid, "div", "height: 10px");
    let g2 = el(&mut tree, grid, "div", "height: 10px");
    let t = layout(&tree, html, 800.0);
    let right = |n| { let r = rect_of(&t, n); r.x + r.w };
    let x0 = rect_of(&t, rtl).x;
    // A fixed-width block sits at the right
    assert!((right(fixed) - (x0 + 600.0)).abs() < 0.5, "{:?}", rect_of(&t, fixed));
    // Flex items start from the right: f1 rightmost, f2 to its left
    assert!((right(f1) - (x0 + 600.0)).abs() < 0.5);
    assert!((right(f2) - rect_of(&t, f1).x).abs() < 0.5);
    // The first grid column is the rightmost
    assert!((right(g1) - (x0 + 600.0)).abs() < 0.5);
    assert!((rect_of(&t, g1).w - 200.0).abs() < 0.5);
    assert!((right(g2) - rect_of(&t, g1).x).abs() < 0.5);
}

#[test]
fn aspect_ratio_sizes_automatic_dimensions() {
    let (mut tree, html, body) = doc();
    // Height from width: content box, then border box
    let a = el(&mut tree, body, "div", "display: block; width: 90px; aspect-ratio: 1.8");
    let b = el(&mut tree, body, "div", "display: block; width: 100px; padding: 10px; box-sizing: border-box; aspect-ratio: 2 / 1");
    // A stretched block takes the container's width, then the ratio
    let wrap = el(&mut tree, body, "div", "display: block; width: 300px");
    let c = el(&mut tree, wrap, "div", "display: block; aspect-ratio: 16 / 9");
    // Width from height for shrink-to-fit boxes
    let d = el(&mut tree, body, "div", "display: inline-block; height: 20px; aspect-ratio: 3");
    // A specified height wins over the ratio
    let e = el(&mut tree, body, "div", "display: block; width: 50px; height: 7px; aspect-ratio: 1");
    let t = layout(&tree, html, 800.0);
    let size = |n| { let r = rect_of(&t, n); (r.w.round(), r.h.round()) };
    assert_eq!(size(a), (90.0, 50.0));
    assert_eq!(size(b), (100.0, 50.0));
    assert_eq!(size(c), (300.0, 169.0));
    assert_eq!(size(d), (60.0, 20.0));
    assert_eq!(size(e), (50.0, 7.0));
}

#[test]
fn nowrap_flex_items_keep_their_text_width() {
    let (mut tree, html, body) = doc();
    let row = el(&mut tree, body, "div", "display: flex; width: 100px");
    let a = el(&mut tree, row, "span", "white-space: nowrap");
    text(&mut tree, a, "one two three four");
    let b = el(&mut tree, row, "span", "white-space: nowrap");
    text(&mut tree, b, "five six");
    let t = layout(&tree, html, 800.0);
    // min-content of nowrap text includes its spaces: the items overflow
    // the row rather than overlapping
    let (ra, rb) = (rect_of(&t, a), rect_of(&t, b));
    let texts: Vec<Rect> = t.element_rects().into_iter().filter(|(n, r)| *n == a && r.w > 0.0).map(|(_, r)| r).collect();
    let text_right = texts.iter().map(|r| r.right()).fold(0.0f32, f32::max);
    assert!(rb.x >= text_right - 0.5, "{ra:?} {rb:?} text ends at {text_right}");
}

#[test]
fn white_space_between_flex_and_grid_items_is_not_rendered() {
    let (mut tree, html, body) = doc();
    // Even preserved, white space between items makes no item of its own
    // (CSS Grid §6.1, Flexbox §4): shiki's code lines are grid items with
    // newlines between them
    let grid = el(&mut tree, body, "div", "display: grid; white-space: pre");
    let a = el(&mut tree, grid, "div", "height: 10px");
    text(&mut tree, grid, "\n  ");
    let b = el(&mut tree, grid, "div", "height: 10px");
    let flex = el(&mut tree, body, "div", "display: flex; flex-direction: column; white-space: pre-wrap");
    let c = el(&mut tree, flex, "div", "height: 10px");
    text(&mut tree, flex, "\n");
    let d = el(&mut tree, flex, "div", "height: 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, b).y - rect_of(&t, a).y, 10.0);
    assert_eq!(rect_of(&t, grid).h, 20.0);
    assert_eq!(rect_of(&t, d).y - rect_of(&t, c).y, 10.0);
}

#[test]
fn grid_min_content_comes_from_minimum_contributions() {
    let (mut tree, html, body) = doc();
    // A minmax(0, 1fr) column needs nothing at min-content: its flex
    // fraction is zero there
    let fr = el(&mut tree, body, "div", "display: grid; grid-template-columns: minmax(0, 1fr); width: min-content");
    el(&mut tree, fr, "div", "width: 200px; height: 10px");
    // A scroll container has no automatic minimum, so an auto column's
    // min-content is what its other items need, and its max-content what
    // the scroll container's content does
    let scrolls = |tree: &mut DomTree, width: &str| {
        let g = el(tree, body, "div", &format!("display: grid; {width}"));
        let s = el(tree, g, "div", "overflow-x: auto");
        el(tree, s, "div", "width: 300px; height: 10px");
        let o = el(tree, g, "div", "");
        el(tree, o, "div", "width: 120px; height: 10px");
        (g, s)
    };
    let (min, _) = scrolls(&mut tree, "width: min-content");
    let (max, _) = scrolls(&mut tree, "width: max-content");
    // In a narrower container the column fits it, and the scroll
    // container scrolls
    let narrow = el(&mut tree, body, "div", "width: 200px");
    let g = el(&mut tree, narrow, "div", "display: grid");
    let s = el(&mut tree, g, "div", "overflow-x: auto");
    el(&mut tree, s, "div", "width: 300px; height: 10px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, fr).w, 0.0);
    assert_eq!(rect_of(&t, min).w, 120.0);
    assert_eq!(rect_of(&t, max).w, 300.0);
    assert_eq!((rect_of(&t, g).w, rect_of(&t, s).w), (200.0, 200.0));
}

#[test]
fn intrinsic_widths_size_flex_and_grid_items() {
    let (mut tree, html, body) = doc();
    let words = |tree: &mut DomTree, parent: NodeId| {
        el(tree, parent, "div", "display: inline-block; width: 50px; height: 10px");
        text(tree, parent, " ");
        el(tree, parent, "div", "display: inline-block; width: 80px; height: 10px");
    };
    // A row item's flex base size, and a column or grid item's width, are
    // the keyword's rather than stretched
    let row = el(&mut tree, body, "div", "display: flex; width: 400px");
    let in_row = el(&mut tree, row, "div", "width: min-content");
    words(&mut tree, in_row);
    let column = el(&mut tree, body, "div", "display: flex; flex-direction: column; width: 400px");
    let in_column = el(&mut tree, column, "div", "width: max-content");
    words(&mut tree, in_column);
    let grid = el(&mut tree, body, "div", "display: grid; width: 400px");
    let in_grid = el(&mut tree, grid, "div", "width: min-content");
    words(&mut tree, in_grid);
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, in_row).w, 80.0);
    let w = rect_of(&t, in_column).w;
    assert!(w > 130.0 && w < 150.0, "{w}");
    assert_eq!(rect_of(&t, in_grid).w, 80.0);
}

#[test]
fn preserved_spaces_take_room_at_line_ends_and_starts() {
    let (mut tree, html, body) = doc();
    // Under pre, spaces at a line's end take room rather than hang (CSS
    // Text 3 §4.1.3): Tailwind's "text-8xl " labels sit apart
    let row = el(&mut tree, body, "div", "display: flex; white-space: pre");
    let a = el(&mut tree, row, "span", "");
    text(&mut tree, a, "ab ");
    let b = el(&mut tree, row, "span", "");
    text(&mut tree, b, "cd");
    let keep = el(&mut tree, body, "div", "white-space: pre; width: min-content");
    text(&mut tree, keep, "ab   ");
    // pre-wrap's hang
    let hang = el(&mut tree, body, "div", "white-space: pre-wrap; width: min-content");
    text(&mut tree, hang, "ab   ");
    // Spaces starting a line indent the inline box after them (a code
    // line's first token)
    let line = el(&mut tree, body, "div", "white-space: pre");
    text(&mut tree, line, "  ");
    let token = el(&mut tree, line, "span", "");
    text(&mut tree, token, "x");
    let t = layout(&tree, html, 800.0);
    let ab = rect_of(&t, hang).w;
    assert!(ab > 0.0);
    assert!(rect_of(&t, a).w > ab + 1.0);
    assert_eq!(rect_of(&t, b).x, rect_of(&t, a).x + rect_of(&t, a).w);
    assert!(rect_of(&t, keep).w > rect_of(&t, a).w + 1.0);
    assert!(rect_of(&t, token).x > rect_of(&t, line).x + 1.0);
}

#[test]
fn percentage_flex_bases_of_an_unknown_height_are_the_contents() {
    let (mut tree, html, body) = doc();
    // In a column of unknown height a basis of 0% (flex: 1's) cannot
    // resolve, so it is the content's (CSS Flexbox §9.2.3): react.dev's
    // code panels were collapsed to nothing. 0px is 0
    let column = |tree: &mut DomTree, item: &str| {
        let c = el(tree, body, "div", "display: flex; flex-direction: column");
        let i = el(tree, c, "div", item);
        el(tree, i, "div", "height: 100px");
        i
    };
    let pct = column(&mut tree, "flex: 1 1 0%; overflow: auto");
    let shorthand = column(&mut tree, "flex: 1; overflow: hidden");
    let px = column(&mut tree, "flex: 1 1 0px; overflow: auto");
    // With a height the percentage resolves, and the item grows into it
    let sized = el(&mut tree, body, "div", "display: flex; flex-direction: column; height: 50px");
    let grows = el(&mut tree, sized, "div", "flex: 1 1 0%");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, pct).h, 100.0);
    assert_eq!(rect_of(&t, shorthand).h, 100.0);
    assert_eq!(rect_of(&t, px).h, 0.0);
    assert_eq!(rect_of(&t, grows).h, 50.0);
}

#[test]
fn automatic_column_heights_are_clamped_before_flexing() {
    let (mut tree, html, body) = doc();
    // A column whose height comes from min-height grows its flexible
    // items into it (CSS Flexbox §9.2 #4): vercel.com's hero is a
    // min-height column with a flex: 1 child
    let tall = el(&mut tree, body, "div", "display: flex; flex-direction: column; min-height: 300px");
    let grows = el(&mut tree, tall, "div", "flex: 1");
    el(&mut tree, grows, "div", "height: 18px");
    el(&mut tree, tall, "div", "height: 20px");
    // max-height shrinks them, and justify-content shares what is left
    let short = el(&mut tree, body, "div", "display: flex; flex-direction: column; max-height: 100px");
    let mut shrinks = Vec::new();
    for _ in 0..2 {
        let i = el(&mut tree, short, "div", "min-height: 0; overflow: hidden");
        el(&mut tree, i, "div", "height: 100px");
        shrinks.push(i);
    }
    let centered = el(&mut tree, body, "div", "display: flex; flex-direction: column; min-height: 200px; justify-content: center");
    let middle = el(&mut tree, centered, "div", "height: 50px");
    let t = layout(&tree, html, 800.0);
    assert_eq!(rect_of(&t, tall).h, 300.0);
    assert_eq!(rect_of(&t, grows).h, 280.0);
    assert_eq!(rect_of(&t, short).h, 100.0);
    assert_eq!(shrinks.iter().map(|&i| rect_of(&t, i).h).collect::<Vec<_>>(), [50.0, 50.0]);
    assert_eq!(rect_of(&t, middle).y - rect_of(&t, centered).y, 75.0);
}

#[test]
fn absolutely_positioned_boxes_take_their_width_from_aspect_ratio() {
    let (mut tree, html, body) = doc();
    // An automatic width follows a definite height through aspect-ratio,
    // within max-width
    let cb = el(&mut tree, body, "div", "position: relative; width: 400px; height: 200px");
    let art = el(&mut tree, cb, "div", "position: absolute; left: 50%; height: 100%; aspect-ratio: 3 / 2");
    let capped = el(&mut tree, cb, "div", "position: absolute; left: 0; height: 100px; aspect-ratio: 4; max-width: 300px");
    // Both insets still stretch it
    let stretched = el(&mut tree, cb, "div", "position: absolute; left: 10px; right: 10px; height: 50px; aspect-ratio: 1");
    let t = layout(&tree, html, 800.0);
    assert_eq!((rect_of(&t, art).w, rect_of(&t, art).h), (300.0, 200.0));
    assert_eq!(rect_of(&t, capped).w, 300.0);
    assert_eq!(rect_of(&t, stretched).w, 380.0);
}

#[test]
fn stretched_grid_items_keep_their_max_sizes() {
    let (mut tree, html, body) = doc();
    // Stretching fills the grid area within min/max-width and
    // min/max-height (CSS Box Alignment §6.1): vercel.com's col-span-8
    // headings have a max-width
    let grid = el(&mut tree, body, "div", "display: grid; grid-template-columns: 600px; grid-template-rows: 200px");
    let item = el(&mut tree, grid, "div", "max-width: 300px; max-height: 50px; padding: 5px");
    let wide = el(&mut tree, body, "div", "display: grid; grid-template-columns: 100px");
    let min = el(&mut tree, wide, "div", "min-width: 150px");
    let t = layout(&tree, html, 800.0);
    let r = rect_of(&t, item);
    assert_eq!((r.w, r.h), (310.0, 60.0));
    assert_eq!(rect_of(&t, min).w, 150.0);
}

#[test]
fn contained_boxes() {
    let (mut tree, html, body) = doc();
    // Layout containment: an independent formatting context, so a child's
    // margin stays inside (vercel.com's content-visibility: auto sections)
    let auto = el(&mut tree, body, "div", "content-visibility: auto");
    let child = el(&mut tree, auto, "div", "margin-top: 20px; height: 30px");
    // Size containment: contain-intrinsic-size, not the contents, sizes it
    let sized = el(&mut tree, body, "div", "contain: size; contain-intrinsic-size: 40px 50px; float: left");
    el(&mut tree, sized, "div", "width: 200px; height: 100px");
    // content-visibility: hidden skips the contents
    let hidden = el(&mut tree, body, "div", "content-visibility: hidden; clear: both");
    el(&mut tree, hidden, "div", "height: 100px");
    // A contained box is the containing block of positioned descendants
    let paint = el(&mut tree, body, "div", "contain: paint; margin-left: 100px; height: 10px");
    let abs = el(&mut tree, paint, "div", "position: absolute; left: 5px; top: 0; width: 10px; height: 10px");
    let t = layout(&tree, html, 800.0);
    let (a, c) = (rect_of(&t, auto), rect_of(&t, child));
    assert_eq!((a.y - rect_of(&t, body).y, a.h, c.y - a.y), (0.0, 50.0, 20.0));
    let r = rect_of(&t, sized);
    assert_eq!((r.w, r.h), (40.0, 50.0));
    assert_eq!(rect_of(&t, hidden).h, 0.0);
    assert_eq!(rect_of(&t, abs).x, rect_of(&t, paint).x + 5.0);
    // Paint containment clips but does not scroll: a sticky box inside
    // sticks to the viewport (Wikipedia's contents column)
    let mut scrolls = None;
    t.for_each(|f| {
        if let Fragment::Box(b) = f {
            if b.node == paint {
                scrolls = Some(b.scroll_extent.is_some());
            }
        }
    });
    assert_eq!(scrolls, Some(false));
}

#[test]
fn inline_boxes_ending_with_a_newline_end_on_its_line() {
    let (mut tree, html, body) = doc();
    // A highlighted code token ending in a newline ends on that line: it
    // has no empty part starting the next (htmx.org's code samples)
    let pre = el(&mut tree, body, "pre", "white-space: pre; margin: 0");
    let token = el(&mut tree, pre, "span", "");
    text(&mut tree, token, "abc\n");
    let next = el(&mut tree, pre, "span", "");
    text(&mut tree, next, "de");
    let t = layout(&tree, html, 800.0);
    let parts = rects_of(&t, token);
    assert!(parts.iter().all(|r| r.y == parts[0].y), "{parts:?}");
    let n = rect_of(&t, next);
    assert!(n.y >= parts[0].y + parts[0].h - 0.5, "{parts:?} {n:?}");
    assert_eq!(n.x, rect_of(&t, pre).x);
}
