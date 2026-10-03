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
