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
