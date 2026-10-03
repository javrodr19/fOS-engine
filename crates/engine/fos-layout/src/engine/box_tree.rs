//! The box tree: what CSS generates from the DOM and computed styles
//!
//! Block containers hold either block-level boxes or one inline formatting
//! context, never both: runs of inline content beside blocks are wrapped
//! in anonymous blocks, and a block inside an inline splits the inline
//! (CSS 2.1 §9.2.1.1). An inline formatting context is a flat list of
//! items (text, inline box starts and ends, atomic inlines, forced breaks)
//! over one string with white space already processed (CSS Text §4.1).

use fos_css::properties::Color;
use fos_css::style::{Display, ListStylePosition, ListStyleType, Style, TextTransform, WhiteSpace};
use fos_dom::{DomTree, NodeId};

/// Computes styles for the box tree builder (the browser's cascade)
pub trait Styler {
    /// The element's computed style, as a child of `parent`
    fn style(&mut self, tree: &DomTree, node: NodeId, parent: &Style) -> Style;
    /// The builder descends into / comes back from an element's children
    fn enter(&mut self, _tree: &DomTree, _node: NodeId) {}
    fn leave(&mut self) {}
    /// A replaced element's natural size in px, when the embedder knows it
    /// (a decoded image)
    fn natural_size(&mut self, _tree: &DomTree, _node: NodeId) -> Option<(f32, f32)> {
        None
    }
}

/// A replaced element (or form control drawn as one)
#[derive(Clone, Debug)]
pub struct Replaced {
    /// Natural width and height (and their ratio, for auto sizes)
    pub natural: Option<(f32, f32)>,
    pub what: ReplacedWhat,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReplacedWhat {
    Image,
    Canvas,
    Video,
    Frame,
    Svg,
    /// A text field: value or placeholder, and whether it is a placeholder
    TextField(String, bool),
    Check { radio: bool, checked: bool },
    /// A select: the selected option's text
    Select(String),
}

/// A list marker
#[derive(Clone, Debug)]
pub struct Marker {
    pub text: String,
    pub style: Style,
    pub outside: bool,
}

#[derive(Debug)]
pub enum BoxKind {
    /// A block container of block-level children (possibly none)
    Block(Vec<LayoutBox>),
    /// A block container establishing an inline formatting context
    Inline(InlineContent),
    Replaced(Replaced),
    /// A flex container and its items (blockified; text runs wrapped in
    /// anonymous blocks)
    Flex(Vec<LayoutBox>),
    /// A table: captions and rows of cells (row groups flattened, header
    /// rows first and footer rows last)
    Table(Box<TableBox>),
}

#[derive(Debug, Default)]
pub struct TableBox {
    pub captions: Vec<LayoutBox>,
    pub rows: Vec<TableRow>,
}

#[derive(Debug)]
pub struct TableRow {
    /// `NodeId::NONE` for anonymous rows
    pub node: NodeId,
    pub style: Style,
    pub cells: Vec<TableCell>,
}

#[derive(Debug)]
pub struct TableCell {
    pub b: LayoutBox,
    pub colspan: u32,
    pub rowspan: u32,
}

#[derive(Debug)]
pub struct LayoutBox {
    /// `NodeId::NONE` for anonymous boxes
    pub node: NodeId,
    pub style: Style,
    pub kind: BoxKind,
    pub marker: Option<Marker>,
}

impl LayoutBox {
    fn anonymous(parent: &Style, kind: BoxKind) -> LayoutBox {
        let mut style = Style::inherit_from(parent);
        std::sync::Arc::make_mut(&mut style.box_).display = Display::Block;
        LayoutBox { node: NodeId::NONE, style, kind, marker: None }
    }

    /// Establishes an independent formatting context (its margins never
    /// collapse with its children's)
    pub fn is_bfc_root(&self) -> bool {
        let b = &self.style.box_;
        matches!(self.kind, BoxKind::Replaced(_))
            || !matches!(b.display, Display::Block | Display::ListItem)
            || b.overflow_x != fos_css::style::Overflow::Visible
            || b.overflow_y != fos_css::style::Overflow::Visible
            || b.float != fos_css::style::Float::None
            || self.style.is_out_of_flow()
    }
}

/// One inline formatting context
#[derive(Debug)]
pub struct InlineContent {
    /// All its text, white space processed
    pub text: String,
    /// Styles of its inline boxes; 0 is the container's
    pub styles: Vec<Style>,
    pub items: Vec<InlineItem>,
    /// Text decorations propagated from the container and its block
    /// ancestors (line flags and color)
    pub decoration: (u8, Color),
}

#[derive(Debug)]
pub enum InlineItem {
    /// `text[start..end]`, in `styles[style]`, from a child of element `node`
    Text { start: usize, end: usize, style: u32, node: NodeId },
    /// An inline box starts (its left margin, border and padding)
    Open { style: u32, node: NodeId },
    /// The innermost open inline box ends
    Close,
    /// inline-block, inline replaced elements...
    Atomic(Box<LayoutBox>),
    /// `<br>`, or a preserved newline
    Break,
}

impl InlineContent {
    /// Nothing but collapsible white space (dropped beside blocks)
    fn is_blank(&self) -> bool {
        self.items.iter().all(|i| match i {
            InlineItem::Text { start, end, .. } => self.text[*start..*end].trim_matches(' ').is_empty(),
            InlineItem::Open { .. } | InlineItem::Close => true,
            InlineItem::Atomic(_) | InlineItem::Break => false,
        }) && !self.items.iter().any(|i| matches!(i, InlineItem::Open { style, .. } if has_edges(&self.styles[*style as usize])))
    }
}

/// An inline box with borders or padding shows even when empty
fn has_edges(s: &Style) -> bool {
    s.border.has_border() || s.box_.padding.iter().any(|p| !p.is_zero()) || s.background.is_visible()
}

/// Builds the inline content of one block container
struct InlineBuilder {
    content: InlineContent,
    /// Open inline boxes: (style index, node)
    open: Vec<(u32, NodeId)>,
    /// The last character was collapsible white space (or the start)
    after_space: bool,
}

impl InlineBuilder {
    fn new(container: &Style, decoration: (u8, Color)) -> Self {
        InlineBuilder { content: InlineContent { text: String::new(), styles: vec![container.clone()], items: Vec::new(), decoration }, open: Vec::new(), after_space: true }
    }

    fn current_style(&self) -> u32 {
        self.open.last().map_or(0, |o| o.0)
    }

    fn push_text(&mut self, raw: &str, node: NodeId) {
        let si = self.current_style();
        let style = &self.content.styles[si as usize];
        let ws = style.inherited.white_space;
        let transform = style.inherited.text_transform;
        let mut processed = String::with_capacity(raw.len());
        if ws.collapses_spaces() {
            // Segment breaks: kept as breaks (pre-line) or spaces
            for (li, line) in raw.split('\n').enumerate() {
                if li > 0 {
                    if ws.keeps_newlines() {
                        // Spaces before a break go
                        while processed.ends_with(' ') {
                            processed.pop();
                        }
                        processed.push('\n');
                        self.after_space = true;
                    } else if !self.after_space {
                        processed.push(' ');
                        self.after_space = true;
                    }
                }
                for c in line.chars() {
                    let c = if c == '\t' || c == '\r' || c == '\u{c}' { ' ' } else { c };
                    if c == ' ' {
                        if !self.after_space {
                            processed.push(' ');
                            self.after_space = true;
                        }
                    } else {
                        processed.push(c);
                        self.after_space = false;
                    }
                }
            }
        } else {
            for c in raw.chars() {
                match c {
                    '\r' => {}
                    '\t' => processed.push_str("        "),
                    _ => processed.push(c),
                }
            }
            self.after_space = processed.ends_with('\n') || processed.ends_with(' ');
        }
        let processed = apply_transform(&processed, transform);
        // Preserved newlines become breaks
        for (i, part) in processed.split('\n').enumerate() {
            if i > 0 {
                self.content.items.push(InlineItem::Break);
            }
            if !part.is_empty() {
                let start = self.content.text.len();
                self.content.text.push_str(part);
                self.content.items.push(InlineItem::Text { start, end: self.content.text.len(), style: si, node });
            }
        }
    }

    fn open(&mut self, style: Style, node: NodeId) {
        let si = self.content.styles.len() as u32;
        self.content.styles.push(style);
        self.content.items.push(InlineItem::Open { style: si, node });
        self.open.push((si, node));
    }

    fn close(&mut self) {
        if self.open.pop().is_some() {
            self.content.items.push(InlineItem::Close);
        }
    }

    fn atomic(&mut self, b: LayoutBox) {
        self.content.items.push(InlineItem::Atomic(Box::new(b)));
        self.after_space = false;
    }

    fn forced_break(&mut self) {
        // Spaces right before a break are removed
        if let Some(InlineItem::Text { end, start, .. }) = self.content.items.last_mut() {
            while *end > *start && self.content.text.as_bytes()[*end - 1] == b' ' {
                *end -= 1;
            }
        }
        self.content.items.push(InlineItem::Break);
        self.after_space = true;
    }

    /// Take the content so far (closing open inline boxes; they reopen in
    /// what follows), for an anonymous block before a block-level child
    fn take(&mut self, container: &Style) -> InlineContent {
        for _ in 0..self.open.len() {
            self.content.items.push(InlineItem::Close);
        }
        let reopen: Vec<Style> = self.open.iter().map(|(si, _)| self.content.styles[*si as usize].clone()).collect();
        let nodes: Vec<NodeId> = self.open.iter().map(|o| o.1).collect();
        let decoration = self.content.decoration;
        let taken = std::mem::replace(&mut self.content, InlineContent { text: String::new(), styles: vec![container.clone()], items: Vec::new(), decoration });
        self.open.clear();
        for (s, n) in reopen.into_iter().zip(nodes) {
            self.open(s, n);
        }
        self.after_space = true;
        taken
    }
}

fn apply_transform(s: &str, t: TextTransform) -> String {
    match t {
        TextTransform::None => s.to_string(),
        TextTransform::Uppercase => s.to_uppercase(),
        TextTransform::Lowercase => s.to_lowercase(),
        TextTransform::Capitalize => {
            let mut out = String::with_capacity(s.len());
            let mut start = true;
            for c in s.chars() {
                if start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    start = false;
                } else {
                    if c.is_whitespace() {
                        start = true;
                    } else if c.is_alphanumeric() {
                        start = false;
                    }
                    out.push(c);
                }
            }
            out
        }
    }
}

/// The children of one block container being collected
struct Container<'a> {
    style: &'a Style,
    blocks: Vec<LayoutBox>,
    inline: InlineBuilder,
}

impl Container<'_> {
    /// End the current inline run before a block-level box
    fn flush_inline(&mut self) {
        let content = self.inline.take(self.style);
        if !content.is_blank() {
            self.blocks.push(LayoutBox::anonymous(self.style, BoxKind::Inline(content)));
        }
    }

    fn finish(mut self) -> BoxKind {
        if self.blocks.is_empty() {
            let content = self.inline.take(self.style);
            return if content.is_blank() && content.items.iter().all(|i| matches!(i, InlineItem::Text { .. })) {
                BoxKind::Block(Vec::new())
            } else {
                BoxKind::Inline(content)
            };
        }
        self.flush_inline();
        BoxKind::Block(self.blocks)
    }
}

pub struct BoxTreeBuilder<'a, S: Styler> {
    tree: &'a DomTree,
    styler: &'a mut S,
    /// Ordinals of open ordered lists
    list_counters: Vec<i32>,
    /// Decorations in effect from block ancestors
    deco: (u8, Color),
}

/// Build the box tree of the document whose root element is `root`
pub fn build_box_tree<S: Styler>(tree: &DomTree, root: NodeId, styler: &mut S) -> Option<LayoutBox> {
    let mut b = BoxTreeBuilder { tree, styler, list_counters: Vec::new(), deco: (0, Color::BLACK) };
    let parent = Style::default();
    let mut style = b.styler.style(tree, root, &parent);
    if style.display() == Display::None {
        return None;
    }
    // The root element is always a block
    if style.display().is_inline_level() || style.display() == Display::Contents {
        std::sync::Arc::make_mut(&mut style.box_).display = style.display().blockified();
    }
    Some(b.element_box(root, style, false))
}

/// A style's own text decoration
fn own_decoration(style: &Style) -> (u8, Color) {
    (style.box_.text_decoration_line, style.box_.text_decoration_color.unwrap_or(style.color()))
}

impl<S: Styler> BoxTreeBuilder<'_, S> {
    /// The box of element `node` (a block container, or replaced);
    /// `in_flow` boxes get their ancestors' text decorations
    fn element_box(&mut self, node: NodeId, style: Style, in_flow: bool) -> LayoutBox {
        if let Some(r) = self.replaced(node, &style) {
            return LayoutBox { node, style, kind: BoxKind::Replaced(r), marker: None };
        }
        let saved = self.deco;
        let own = own_decoration(&style);
        self.deco = if in_flow && !style.is_out_of_flow() && style.box_.float == fos_css::style::Float::None {
            (saved.0 | own.0, if own.0 != 0 { own.1 } else { saved.1 })
        } else {
            own
        };
        let marker = (style.display() == Display::ListItem).then(|| self.marker(node, &style)).flatten();
        let is_ol = self.tag(node) == "ol";
        if is_ol {
            let start = self.tree.get_attribute(node, "start").and_then(|s| s.trim().parse().ok()).unwrap_or(1);
            self.list_counters.push(start);
        } else if matches!(self.tag(node), "ul" | "menu" | "dir") {
            self.list_counters.push(1);
        }
        if matches!(style.display(), Display::Table | Display::InlineTable) {
            self.styler.enter(self.tree, node);
            let table = self.table(node, &style);
            self.styler.leave();
            self.deco = saved;
            if is_ol || matches!(self.tag(node), "ul" | "menu" | "dir") {
                self.list_counters.pop();
            }
            return LayoutBox { node, style, kind: BoxKind::Table(Box::new(table)), marker: None };
        }
        if matches!(style.display(), Display::Flex | Display::InlineFlex) {
            let mut items = Vec::new();
            let mut text = InlineBuilder::new(&style, self.deco);
            self.styler.enter(self.tree, node);
            self.flex_items(node, &style, &mut items, &mut text);
            self.styler.leave();
            let content = text.take(&style);
            if !content.is_blank() {
                items.push(LayoutBox::anonymous(&style, BoxKind::Inline(content)));
            }
            self.deco = saved;
            if is_ol || matches!(self.tag(node), "ul" | "menu" | "dir") {
                self.list_counters.pop();
            }
            return LayoutBox { node, style, kind: BoxKind::Flex(items), marker: None };
        }
        let mut c = Container { style: &style, blocks: Vec::new(), inline: InlineBuilder::new(&style, self.deco) };
        // Inside markers lead the content
        if let Some(m) = marker.as_ref().filter(|m| !m.outside) {
            c.inline.open(m.style.clone(), NodeId::NONE);
            c.inline.push_text(&format!("{} ", m.text.trim_end()), NodeId::NONE);
            c.inline.close();
        }
        self.styler.enter(self.tree, node);
        self.children(node, &style, &mut c);
        self.styler.leave();
        if is_ol || matches!(self.tag(node), "ul" | "menu" | "dir") {
            self.list_counters.pop();
        }
        let kind = c.finish();
        self.deco = saved;
        LayoutBox { node, style: style.clone(), kind, marker: marker.filter(|m| m.outside) }
    }

    fn tag(&self, node: NodeId) -> &str {
        self.tree.get(node).and_then(|n| n.as_element()).map_or("", |e| self.tree.resolve(e.name.local))
    }

    /// Add element `parent`'s children to container `c`
    fn children(&mut self, parent: NodeId, parent_style: &Style, c: &mut Container) {
        let kids: Vec<NodeId> = self.tree.children(parent).map(|(id, _)| id).collect();
        for child in kids {
            let Some(n) = self.tree.get(child) else { continue };
            if let Some(text) = n.as_text() {
                if !text.is_empty() {
                    c.inline.push_text(text, parent);
                }
                continue;
            }
            if !n.is_element() {
                continue;
            }
            let style = self.styler.style(self.tree, child, parent_style);
            let display = style.display();
            if display == Display::None {
                continue;
            }
            match self.tag(child) {
                "br" => {
                    c.inline.forced_break();
                    continue;
                }
                "wbr" => continue,
                _ => {}
            }
            if display == Display::Contents {
                self.styler.enter(self.tree, child);
                self.children(child, &style, c);
                self.styler.leave();
                continue;
            }
            let replaced = self.is_replaced(child);
            if display == Display::Inline && !replaced {
                c.inline.open(style.clone(), child);
                self.styler.enter(self.tree, child);
                self.children(child, &style, c);
                self.styler.leave();
                c.inline.close();
            } else if display.is_inline_level() || (replaced && display == Display::Inline) {
                let b = self.element_box(child, style, false);
                c.inline.atomic(b);
            } else {
                // Block-level (tables, grids and out-of-flow boxes are laid
                // out as blocks for now)
                c.flush_inline();
                let b = self.element_box(child, style, true);
                c.blocks.push(b);
            }
        }
    }

    /// Element children with their styles, skipping `display: none`
    /// (and text, which tables do not render outside cells)
    fn styled_children(&mut self, parent: NodeId, parent_style: &Style) -> Vec<(NodeId, Style)> {
        let kids: Vec<NodeId> = self.tree.children(parent).map(|(id, _)| id).collect();
        let mut out = Vec::new();
        for child in kids {
            if !self.tree.get(child).is_some_and(|n| n.is_element()) {
                continue;
            }
            let style = self.styler.style(self.tree, child, parent_style);
            if style.display() != Display::None {
                out.push((child, style));
            }
        }
        out
    }

    fn anonymous_row(style: &Style) -> TableRow {
        TableRow { node: NodeId::NONE, style: Style::inherit_from(style), cells: Vec::new() }
    }

    /// A table's captions and rows (CSS 2.1 §17.2.1, simplified: stray
    /// cells get anonymous rows, other stray elements become cells)
    fn table(&mut self, node: NodeId, style: &Style) -> TableBox {
        let mut t = TableBox::default();
        let (mut head, mut body, mut foot) = (Vec::new(), Vec::new(), Vec::new());
        let mut anon: Option<TableRow> = None;
        for (child, cstyle) in self.styled_children(node, style) {
            match cstyle.display() {
                Display::TableCaption => {
                    let mut cs = cstyle;
                    std::sync::Arc::make_mut(&mut cs.box_).display = Display::Block;
                    t.captions.push(self.element_box(child, cs, true));
                }
                Display::TableHeaderGroup | Display::TableRowGroup | Display::TableFooterGroup => {
                    body.extend(anon.take());
                    self.styler.enter(self.tree, child);
                    let rows = self.row_group(child, &cstyle);
                    self.styler.leave();
                    match cstyle.display() {
                        Display::TableHeaderGroup if head.is_empty() => head = rows,
                        Display::TableFooterGroup if foot.is_empty() => foot = rows,
                        _ => body.extend(rows),
                    }
                }
                Display::TableRow => {
                    body.extend(anon.take());
                    body.push(self.row(child, cstyle));
                }
                Display::TableColumn | Display::TableColumnGroup => {}
                _ => {
                    let cell = self.cell(child, cstyle);
                    anon.get_or_insert_with(|| Self::anonymous_row(style)).cells.push(cell);
                }
            }
        }
        body.extend(anon);
        t.rows = head;
        t.rows.extend(body);
        t.rows.extend(foot);
        t
    }

    fn row_group(&mut self, node: NodeId, style: &Style) -> Vec<TableRow> {
        let mut rows = Vec::new();
        let mut anon: Option<TableRow> = None;
        for (child, cstyle) in self.styled_children(node, style) {
            if cstyle.display() == Display::TableRow {
                rows.extend(anon.take());
                rows.push(self.row(child, cstyle));
            } else {
                let cell = self.cell(child, cstyle);
                anon.get_or_insert_with(|| Self::anonymous_row(style)).cells.push(cell);
            }
        }
        rows.extend(anon);
        rows
    }

    fn row(&mut self, node: NodeId, style: Style) -> TableRow {
        self.styler.enter(self.tree, node);
        let cells = self.styled_children(node, &style).into_iter().map(|(c, cs)| self.cell(c, cs)).collect();
        self.styler.leave();
        TableRow { node, style, cells }
    }

    fn cell(&mut self, node: NodeId, mut style: Style) -> TableCell {
        if style.display() != Display::TableCell {
            std::sync::Arc::make_mut(&mut style.box_).display = Display::TableCell;
        }
        let span = |name: &str, max: u32| self.tree.get_attribute(node, name).and_then(|v| v.trim().parse::<u32>().ok()).map_or(1, |v| v.clamp(1, max));
        let (colspan, rowspan) = (span("colspan", 1000), span("rowspan", 65534));
        TableCell { b: self.element_box(node, style, true), colspan, rowspan }
    }

    /// A flex container's children as items: each element is blockified;
    /// runs of text between them become anonymous items
    fn flex_items(&mut self, parent: NodeId, parent_style: &Style, items: &mut Vec<LayoutBox>, text: &mut InlineBuilder) {
        let kids: Vec<NodeId> = self.tree.children(parent).map(|(id, _)| id).collect();
        for child in kids {
            let Some(n) = self.tree.get(child) else { continue };
            if let Some(t) = n.as_text() {
                if !t.is_empty() {
                    text.push_text(t, parent);
                }
                continue;
            }
            if !n.is_element() {
                continue;
            }
            let mut style = self.styler.style(self.tree, child, parent_style);
            let display = style.display();
            if display == Display::None {
                continue;
            }
            if display == Display::Contents {
                self.styler.enter(self.tree, child);
                self.flex_items(child, &style, items, text);
                self.styler.leave();
                continue;
            }
            let content = text.take(parent_style);
            if !content.is_blank() {
                items.push(LayoutBox::anonymous(parent_style, BoxKind::Inline(content)));
            }
            let blockified = display.blockified();
            if blockified != display {
                std::sync::Arc::make_mut(&mut style.box_).display = blockified;
            }
            let b = self.element_box(child, style, true);
            items.push(b);
        }
    }

    fn is_replaced(&self, node: NodeId) -> bool {
        match self.tag(node) {
            "img" | "canvas" | "video" | "iframe" | "embed" | "object" | "svg" | "textarea" | "select" => true,
            "input" => !matches!(self.tree.get_attribute(node, "type").map(|t| t.to_ascii_lowercase()).as_deref(), Some("hidden" | "button" | "submit" | "reset")),
            _ => false,
        }
    }

    fn replaced(&mut self, node: NodeId, _style: &Style) -> Option<Replaced> {
        if !self.is_replaced(node) {
            return None;
        }
        let attr_px = |name: &str| self.tree.get_attribute(node, name).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok()).filter(|v| *v >= 0.0);
        let (w_attr, h_attr) = (attr_px("width"), attr_px("height"));
        let tag = self.tag(node).to_string();
        let (what, natural) = match tag.as_str() {
            "img" => {
                let natural = self.styler.natural_size(self.tree, node).or(match (w_attr, h_attr) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                });
                (ReplacedWhat::Image, natural)
            }
            "canvas" => (ReplacedWhat::Canvas, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "video" => (ReplacedWhat::Video, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "iframe" | "embed" | "object" => (ReplacedWhat::Frame, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "svg" => (ReplacedWhat::Svg, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "select" => {
                let selected = self.selected_option(node);
                (ReplacedWhat::Select(selected), None)
            }
            "textarea" => {
                let text: String = self.tree.children(node).filter_map(|(_, n)| n.as_text()).collect();
                let (shown, placeholder) = if text.is_empty() {
                    (self.tree.get_attribute(node, "placeholder").unwrap_or("").to_string(), true)
                } else {
                    (text, false)
                };
                (ReplacedWhat::TextField(shown, placeholder), None)
            }
            _ => {
                let ty = self.tree.get_attribute(node, "type").map(|t| t.to_ascii_lowercase()).unwrap_or_default();
                match ty.as_str() {
                    "checkbox" | "radio" => {
                        let checked = self.tree.get_attribute(node, "checked").is_some();
                        (ReplacedWhat::Check { radio: ty == "radio", checked }, Some((13.0, 13.0)))
                    }
                    "image" => (ReplacedWhat::Image, w_attr.zip(h_attr)),
                    _ => {
                        let value = self.tree.get_attribute(node, "value").unwrap_or("");
                        let (shown, placeholder) = if value.is_empty() {
                            (self.tree.get_attribute(node, "placeholder").unwrap_or("").to_string(), true)
                        } else if ty == "password" {
                            ("\u{2022}".repeat(value.chars().count()), false)
                        } else {
                            (value.to_string(), false)
                        };
                        (ReplacedWhat::TextField(shown, placeholder), None)
                    }
                }
            }
        };
        Some(Replaced { natural, what })
    }

    fn selected_option(&self, select: NodeId) -> String {
        let mut first = None;
        let mut stack = vec![select];
        while let Some(n) = stack.pop() {
            let kids: Vec<NodeId> = self.tree.children(n).map(|(id, _)| id).collect();
            for k in kids.into_iter().rev() {
                stack.push(k);
            }
            if self.tag(n) == "option" {
                let text: String = self.tree.children(n).filter_map(|(_, c)| c.as_text()).collect();
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if self.tree.get_attribute(n, "selected").is_some() {
                    return text;
                }
                first.get_or_insert(text);
            }
        }
        first.unwrap_or_default()
    }

    fn marker(&mut self, node: NodeId, style: &Style) -> Option<Marker> {
        let ordinal = match self.list_counters.last_mut() {
            Some(c) => {
                if let Some(v) = self.tree.get_attribute(node, "value").and_then(|v| v.trim().parse().ok()) {
                    *c = v;
                }
                let n = *c;
                *c += 1;
                n
            }
            None => 1,
        };
        let text = marker_text(style.inherited.list_style_type, ordinal)?;
        let mut mstyle = Style::inherit_from(style);
        // Markers keep the list item's font but not its decorations
        std::sync::Arc::make_mut(&mut mstyle.inherited).white_space = WhiteSpace::Pre;
        Some(Marker { text, style: mstyle, outside: style.inherited.list_style_position == ListStylePosition::Outside })
    }
}

/// The marker string for a list item's ordinal
pub fn marker_text(kind: ListStyleType, n: i32) -> Option<String> {
    let alpha = |n: i32, base: u8| -> String {
        let mut n = n.max(1) as u32;
        let mut s = Vec::new();
        while n > 0 {
            n -= 1;
            s.push((base + (n % 26) as u8) as char);
            n /= 26;
        }
        s.iter().rev().collect()
    };
    let roman = |n: i32| -> String {
        if !(1..4000).contains(&n) {
            return n.to_string();
        }
        let table = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
        let mut n = n;
        let mut s = String::new();
        for (v, r) in table {
            while n >= v {
                s.push_str(r);
                n -= v;
            }
        }
        s
    };
    Some(match kind {
        ListStyleType::None => return None,
        ListStyleType::Disc => "\u{2022} ".to_string(),
        ListStyleType::Circle => "\u{25e6} ".to_string(),
        ListStyleType::Square => "\u{25aa} ".to_string(),
        ListStyleType::Decimal => format!("{n}. "),
        ListStyleType::DecimalLeadingZero => format!("{n:02}. "),
        ListStyleType::LowerAlpha => format!("{}. ", alpha(n, b'a')),
        ListStyleType::UpperAlpha => format!("{}. ", alpha(n, b'A')),
        ListStyleType::LowerRoman => format!("{}. ", roman(n)),
        ListStyleType::UpperRoman => format!("{}. ", roman(n).to_uppercase()),
        ListStyleType::LowerGreek => {
            let c = char::from_u32(0x3b1 + ((n.max(1) - 1) % 24) as u32).unwrap_or('α');
            format!("{c}. ")
        }
    })
}
