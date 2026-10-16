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
    /// The document is in quirks mode
    fn quirks(&self) -> bool {
        false
    }
    /// The builder descends into / comes back from an element's children
    fn enter(&mut self, _tree: &DomTree, _node: NodeId) {}
    fn leave(&mut self) {}
    /// A replaced element's natural size in px, when the embedder knows it
    /// (a decoded image)
    fn natural_size(&mut self, _tree: &DomTree, _node: NodeId) -> Option<(f32, f32)> {
        None
    }
    /// An image element's content: its natural size and what the painter
    /// draws
    fn image(&mut self, _tree: &DomTree, _node: NodeId) -> Option<((f32, f32), ImageHandle)> {
        None
    }
    /// An inline `<svg>` element's picture: its natural size and what the
    /// painter draws (`style` is the element's)
    fn inline_svg(&mut self, _tree: &DomTree, _node: NodeId, _style: &Style) -> Option<((f32, f32), ImageHandle)> {
        None
    }
    /// The style of the element's `::before` or `::after` box (a child
    /// of `style`), when it has content
    fn pseudo(&mut self, _tree: &DomTree, _node: NodeId, _pe: fos_dom::PseudoElement, _style: &Style) -> Option<Style> {
        None
    }
    /// An image of generated content (`content: url(...)`), once loaded
    fn content_image(&mut self, _url: &str) -> Option<((f32, f32), ImageHandle)> {
        None
    }
    /// Whether only the ratio of an image's natural size is its own (an
    /// SVG with a viewBox but no width or height)
    fn ratio_only(&self, _image: &ImageHandle) -> bool {
        false
    }
}

/// A generated box's content: runs of text and images
enum Generated {
    Text(String),
    Image(LayoutBox),
}

/// Counters in scope, outermost first: name, value and the nesting level
/// of the element that created it
pub type Counters = Vec<(std::sync::Arc<str>, i32, u32)>;

/// The text of a generated box's `content`, with the counters in scope
pub fn generated_text(tree: &DomTree, node: NodeId, style: &Style, counters: &[(std::sync::Arc<str>, i32, u32)]) -> String {
    items_text(tree, node, style.box_.content.iter().flat_map(|c| c.iter()), counters)
}

/// The text of content items (images have none)
fn items_text<'a>(tree: &DomTree, node: NodeId, items: impl Iterator<Item = &'a fos_css::style::ContentItem>, counters: &[(std::sync::Arc<str>, i32, u32)]) -> String {
    use fos_css::style::ContentItem;
    let mut out = String::new();
    for item in items {
        match item {
            ContentItem::Image(_) => {}
            ContentItem::Text(t) => out.push_str(t),
            ContentItem::Attr(name) => out.push_str(tree.get_attribute(node, name).unwrap_or("")),
            ContentItem::OpenQuote => out.push('\u{201C}'),
            ContentItem::CloseQuote => out.push('\u{201D}'),
            ContentItem::Counter(name, kind) => {
                let v = counters.iter().rev().find(|c| c.0 == *name).map_or(0, |c| c.1);
                out.push_str(&counter_text(*kind, v));
            }
            ContentItem::Counters(name, sep, kind) => {
                let values: Vec<String> = counters.iter().filter(|c| c.0 == *name).map(|c| counter_text(*kind, c.1)).collect();
                if values.is_empty() {
                    out.push_str(&counter_text(*kind, 0));
                } else {
                    out.push_str(&values.join(sep));
                }
            }
        }
    }
    out
}

/// Add a generated box's text and images to inline content
fn push_generated(inline: &mut InlineBuilder, content: Vec<Generated>, node: NodeId) {
    for g in content {
        match g {
            Generated::Text(t) => inline.push_text(&t, node),
            Generated::Image(b) => inline.atomic(b),
        }
    }
}

/// Image content the embedder supplies (layout only passes it to paint)
#[derive(Clone)]
pub struct ImageHandle(pub std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl std::fmt::Debug for ImageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImageHandle")
    }
}

/// A replaced element (or form control drawn as one)
#[derive(Clone, Debug)]
pub struct Replaced {
    /// Natural width and height (and their ratio, for auto sizes)
    pub natural: Option<(f32, f32)>,
    /// Only the ratio of `natural` is natural: auto sizes fill the
    /// available width
    pub ratio_only: bool,
    /// `natural` is a default size with no ratio to keep (a video without
    /// both size attributes, a frame)
    pub no_ratio: bool,
    pub what: ReplacedWhat,
    /// The image to draw, once loaded
    pub image: Option<ImageHandle>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReplacedWhat {
    Image,
    Canvas,
    Video,
    Frame,
    Svg,
    /// A text field: value or placeholder, whether it is a placeholder,
    /// and how many characters wide it is (`size`)
    TextField(String, bool, u32),
    /// A textarea: its text or placeholder, whether it is a placeholder,
    /// and its `cols` and `rows`
    TextArea { text: String, placeholder: bool, cols: u32, rows: u32 },
    Check { radio: bool, checked: bool },
    /// A drop-down: the selected option's text and every option's (which
    /// it is as wide as); a list box (`multiple` or `size`) shows `rows`
    Select { shown: String, options: Vec<String>, rows: Option<u32> },
    /// A button input (submit, reset, button): its label
    Button(String),
    /// A file input
    File,
    /// A range input: where its thumb is, 0 to 1
    Range(f32),
    /// A color input's color (RGBA)
    ColorWell([u8; 4]),
    /// A progress bar (`meter: false`) or meter: how full, 0 to 1
    /// (`None`: indeterminate)
    Gauge { value: Option<f32>, meter: bool },
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
    /// A grid container and its items (made like flex items)
    Grid(Vec<LayoutBox>),
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
            || self.style.contains_formatting()
            // A block container aligning its content (CSS Box Alignment 3 §5.3)
            || b.align_content != fos_css::style::AlignContent::Normal
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
    /// An absolutely positioned box amid the content: it takes no room on
    /// the line, and is placed from where it stands in it
    OutOfFlow(Box<LayoutBox>),
    /// `<br>`, or a preserved newline
    Break,
}

impl InlineContent {
    /// Nothing but collapsible white space (dropped beside blocks): a
    /// preserved space makes a line (bun.sh's blank code lines are
    /// `<div> </div>` in a `<pre>`)
    fn is_blank(&self) -> bool {
        self.items.iter().all(|i| match i {
            InlineItem::Text { start, end, style, .. } => {
                let text = &self.text[*start..*end];
                text.is_empty() || (text.trim_matches(' ').is_empty() && self.styles[*style as usize].inherited.white_space.collapses_spaces())
            }
            InlineItem::Open { .. } | InlineItem::Close => true,
            InlineItem::Atomic(_) | InlineItem::Break | InlineItem::OutOfFlow(_) => false,
        }) && !self.items.iter().any(|i| matches!(i, InlineItem::Open { style, .. } if has_edges(&self.styles[*style as usize])))
    }

    /// Nothing but white space, preserved or not: such a run between flex
    /// or grid items is not rendered (CSS Flexbox §4, Grid §6.1)
    fn is_white_space(&self) -> bool {
        self.items.iter().all(|i| match i {
            InlineItem::Text { start, end, .. } => self.text[*start..*end].chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')),
            InlineItem::Open { .. } | InlineItem::Close | InlineItem::Break => true,
            InlineItem::Atomic(_) | InlineItem::OutOfFlow(_) => false,
        })
    }
}

/// An inline box with left/right borders, padding or margins makes its
/// line even when empty (CSS 2.1 §9.4.2; vertical ones do not)
fn has_edges(s: &Style) -> bool {
    let b = &s.box_;
    [1, 3].iter().any(|&i| s.border.width[i] > 0.0 || !b.padding[i].is_zero() || !matches!(b.margin[i], fos_css::style::LpAuto::Lp(l) if l.is_zero()))
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

    /// An absolutely positioned box: white space on either side of it
    /// collapses as if it were not there
    fn out_of_flow(&mut self, b: LayoutBox) {
        self.content.items.push(InlineItem::OutOfFlow(Box::new(b)));
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

    /// The combined opacity of the open inline boxes, which a block inside
    /// them (with no fragment of theirs around it) takes on
    fn open_opacity(&self) -> f32 {
        self.open.iter().map(|(si, _)| self.content.styles[*si as usize].box_.opacity).product()
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

/// Multiply `opacity` into a box's own
fn fade(style: &mut Style, opacity: f32) {
    if opacity < 1.0 {
        std::sync::Arc::make_mut(&mut style.box_).opacity *= opacity;
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
    /// CSS counters in scope
    counters: Counters,
    /// Nesting level of the element whose children are being built
    depth: u32,
    /// Decorations in effect from block ancestors
    deco: (u8, Color),
}

/// Build the box tree of the document whose root element is `root`
pub fn build_box_tree<S: Styler>(tree: &DomTree, root: NodeId, styler: &mut S) -> Option<LayoutBox> {
    let mut b = BoxTreeBuilder { tree, styler, counters: Vec::new(), depth: 0, deco: (0, Color::BLACK) };
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
    fn element_box(&mut self, node: NodeId, mut style: Style, in_flow: bool) -> LayoutBox {
        self.counters_for(Some(node), &style);
        // A fieldset establishes a block formatting context (HTML §15.3.13)
        if style.box_.display == Display::Block && self.tag(node) == "fieldset" {
            std::sync::Arc::make_mut(&mut style.box_).display = Display::FlowRoot;
        }
        // A button's content is centered in it, whatever its align-content
        // (HTML §15.5.3, as Chromium lays buttons out)
        if matches!(style.box_.display, Display::Block | Display::InlineBlock | Display::FlowRoot | Display::ListItem) && self.tag(node) == "button" {
            std::sync::Arc::make_mut(&mut style.box_).align_content = fos_css::style::AlignContent::Center;
        }
        if let Some(r) = self.replaced(node, &style) {
            return LayoutBox { node, style, kind: BoxKind::Replaced(r), marker: None };
        }
        // content-visibility: hidden skips the contents
        if style.box_.content_visibility == fos_css::style::ContentVisibility::Hidden && style.containment() != 0 {
            return LayoutBox { node, style, kind: BoxKind::Block(Vec::new()), marker: None };
        }
        let saved = self.deco;
        let own = own_decoration(&style);
        self.deco = if in_flow && !style.is_out_of_flow() && style.box_.float == fos_css::style::Float::None {
            (saved.0 | own.0, if own.0 != 0 { own.1 } else { saved.1 })
        } else {
            own
        };
        let marker = (style.display() == Display::ListItem).then(|| self.marker(node, &style)).flatten();
        if matches!(style.display(), Display::Table | Display::InlineTable) {
            self.enter(node);
            let table = self.table(node, &style);
            self.leave();
            self.deco = saved;
            return LayoutBox { node, style, kind: BoxKind::Table(Box::new(table)), marker: None };
        }
        if matches!(style.display(), Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid) {
            let mut items = Vec::new();
            let mut text = InlineBuilder::new(&style, self.deco);
            self.enter(node);
            self.flex_items(node, &style, &mut items, &mut text);
            self.leave();
            let content = text.take(&style);
            if !content.is_white_space() {
                items.push(LayoutBox::anonymous(&style, BoxKind::Inline(content)));
            }
            self.deco = saved;
            let kind = if matches!(style.display(), Display::Grid | Display::InlineGrid) { BoxKind::Grid(items) } else { BoxKind::Flex(items) };
            return LayoutBox { node, style, kind, marker: None };
        }
        let mut c = Container { style: &style, blocks: Vec::new(), inline: InlineBuilder::new(&style, self.deco) };
        // Inside markers lead the content
        if let Some(m) = marker.as_ref().filter(|m| !m.outside) {
            c.inline.open(m.style.clone(), NodeId::NONE);
            c.inline.push_text(&format!("{} ", m.text.trim_end()), NodeId::NONE);
            c.inline.close();
        }
        self.enter(node);
        self.children(node, &style, &mut c);
        self.leave();
        let kind = c.finish();
        self.deco = saved;
        LayoutBox { node, style: style.clone(), kind, marker: marker.filter(|m| m.outside) }
    }

    /// Descend into element `node`'s children
    fn enter(&mut self, node: NodeId) {
        self.styler.enter(self.tree, node);
        self.depth += 1;
    }

    /// Come back from an element's children, ending the counters they
    /// created
    fn leave(&mut self) {
        self.styler.leave();
        self.depth -= 1;
        while self.counters.last().is_some_and(|c| c.2 > self.depth) {
            self.counters.pop();
        }
    }

    /// Apply an element's (or, with no node, a generated box's)
    /// `counter-reset`, `counter-increment` and `counter-set`, in that
    /// order, with the `list-item` counter HTML lists imply (CSS Lists
    /// §4.4)
    fn counters_for(&mut self, node: Option<NodeId>, style: &Style) {
        use std::sync::Arc;
        let list_item = || Arc::<str>::from("list-item");
        let names = |l: &Option<Arc<[(Arc<str>, i32)]>>| l.as_deref().unwrap_or(&[]).to_vec();
        let (mut resets, mut increments, mut sets) = (names(&style.box_.counter_reset), names(&style.box_.counter_increment), names(&style.box_.counter_set));
        if let Some(node) = node {
            let mentions = |l: &[(Arc<str>, i32)]| l.iter().any(|c| &*c.0 == "list-item");
            match self.tag(node) {
                "ol" if !mentions(&resets) => {
                    let start: i32 = self.tree.get_attribute(node, "start").and_then(|s| s.trim().parse().ok()).unwrap_or(1);
                    resets.push((list_item(), start.saturating_sub(1)));
                }
                "ul" | "menu" | "dir" if !mentions(&resets) => resets.push((list_item(), 0)),
                _ => {}
            }
            if style.display() == Display::ListItem {
                if !mentions(&increments) {
                    increments.push((list_item(), 1));
                }
                if let Some(v) = self.tree.get_attribute(node, "value").and_then(|v| v.trim().parse::<i32>().ok()) {
                    sets.push((list_item(), v));
                }
            }
        }
        let depth = self.depth;
        for (name, v) in resets {
            // A sibling's counter of the same name is replaced
            match self.counters.last_mut().filter(|c| c.2 == depth && c.0 == name) {
                Some(c) => c.1 = v,
                None => {
                    self.counters.retain(|c| !(c.2 == depth && c.0 == name));
                    self.counters.push((name, v, depth));
                }
            }
        }
        for (list, add) in [(increments, true), (sets, false)] {
            for (name, v) in list {
                let i = match self.counters.iter().rposition(|c| c.0 == name) {
                    Some(i) => i,
                    None => {
                        self.counters.push((name, 0, depth));
                        self.counters.len() - 1
                    }
                };
                let c = &mut self.counters[i].1;
                *c = if add { c.saturating_add(v) } else { v };
            }
        }
    }

    fn tag(&self, node: NodeId) -> &str {
        self.tree.get(node).and_then(|n| n.as_element()).map_or("", |e| self.tree.resolve(e.name.local))
    }

    /// The element's `::before` (or `::after`) box, if it has one: its
    /// style (display other than none) and content
    fn pseudo_box(&mut self, node: NodeId, style: &Style, after: bool) -> Option<(Style, Vec<Generated>)> {
        use fos_css::style::ContentItem;
        let pe = if after { fos_dom::PseudoElement::After } else { fos_dom::PseudoElement::Before };
        let ps = self.styler.pseudo(self.tree, node, pe, style)?;
        if ps.display() == Display::None {
            return None;
        }
        self.counters_for(None, &ps);
        let items = ps.box_.content.clone().unwrap_or_else(|| std::sync::Arc::from(Vec::new()));
        let mut out = Vec::new();
        // Runs of text between images
        for run in items.split_inclusive(|i| matches!(i, ContentItem::Image(_))) {
            let text = items_text(self.tree, node, run.iter(), &self.counters);
            if !text.is_empty() {
                out.push(Generated::Text(text));
            }
            if let Some(ContentItem::Image(url)) = run.last() {
                let (natural, image) = match self.styler.content_image(url) {
                    Some((n, h)) => (Some(n), Some(h)),
                    // Nothing (not 300×150) until it loads
                    None => (Some((0.0, 0.0)), None),
                };
                let mut st = Style::inherit_from(&ps);
                std::sync::Arc::make_mut(&mut st.box_).display = Display::Inline;
                out.push(Generated::Image(LayoutBox { node: NodeId::NONE, style: st, kind: BoxKind::Replaced(Replaced { natural, what: ReplacedWhat::Image, image, ratio_only: false, no_ratio: false }), marker: None }));
            }
        }
        Some((ps, out))
    }

    /// A non-inline generated box (block, inline-block, flex item, ...)
    fn generated_block(&mut self, node: NodeId, ps: Style, content: Vec<Generated>, after: bool) -> LayoutBox {
        let mut inline = InlineBuilder::new(&ps, if ps.is_out_of_flow() { own_decoration(&ps) } else { self.deco });
        push_generated(&mut inline, content, node);
        let content = inline.take(&ps);
        LayoutBox { node: node.generated(after), style: ps, kind: BoxKind::Inline(content), marker: None }
    }

    /// Add element `parent`'s generated box to container `c`
    fn generated(&mut self, parent: NodeId, parent_style: &Style, c: &mut Container, after: bool) {
        let Some((ps, content)) = self.pseudo_box(parent, parent_style, after) else { return };
        let display = ps.display();
        if display == Display::Inline && !ps.is_out_of_flow() && ps.box_.float == fos_css::style::Float::None {
            c.inline.open(ps, parent.generated(after));
            push_generated(&mut c.inline, content, parent);
            c.inline.close();
        } else if display.is_inline_level() && !ps.is_out_of_flow() && ps.box_.float == fos_css::style::Float::None {
            let b = self.generated_block(parent, ps, content, after);
            c.inline.atomic(b);
        } else if ps.is_out_of_flow() && !c.inline.content.is_blank() {
            // Amid inline content, which it does not split
            let mut ps = ps;
            fade(&mut ps, c.inline.open_opacity());
            let b = self.generated_block(parent, ps, content, after);
            c.inline.out_of_flow(b);
        } else {
            c.flush_inline();
            let mut ps = ps;
            fade(&mut ps, c.inline.open_opacity());
            let b = self.generated_block(parent, ps, content, after);
            c.blocks.push(b);
        }
    }

    /// Add element `parent`'s children to container `c`
    fn children(&mut self, parent: NodeId, parent_style: &Style, c: &mut Container) {
        self.generated(parent, parent_style, c, false);
        self.element_children(parent, parent_style, c);
        self.generated(parent, parent_style, c, true);
    }

    fn element_children(&mut self, parent: NodeId, parent_style: &Style, c: &mut Container) {
        let kids = self.flat_children(parent);
        for child in kids {
            let Some(n) = self.tree.get(child) else { continue };
            if let Some(text) = n.as_text() {
                if !text.is_empty() {
                    c.inline.push_text(text, self.text_parent(n.parent, parent));
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
                self.counters_for(Some(child), &style);
                self.enter(child);
                self.children(child, &style, c);
                self.leave();
                continue;
            }
            let replaced = self.is_replaced(child);
            if display == Display::Inline && !replaced {
                self.counters_for(Some(child), &style);
                c.inline.open(style.clone(), child);
                self.enter(child);
                self.children(child, &style, c);
                self.leave();
                c.inline.close();
            } else if display.is_inline_level() || (replaced && display == Display::Inline) {
                let b = self.element_box(child, style, false);
                c.inline.atomic(b);
            } else if style.is_out_of_flow() && !c.inline.content.is_blank() {
                // Amid inline content, which it does not split
                let mut style = style;
                fade(&mut style, c.inline.open_opacity());
                let b = self.element_box(child, style, true);
                c.inline.out_of_flow(b);
            } else {
                // Block-level (tables, grids and out-of-flow boxes are laid
                // out as blocks for now)
                c.flush_inline();
                let mut style = style;
                fade(&mut style, c.inline.open_opacity());
                let b = self.element_box(child, style, true);
                c.blocks.push(b);
            }
        }
    }

    /// `parent`'s children in the flattened tree: a shadow host's are its
    /// shadow root's, a slot's are the nodes assigned to it (or its own,
    /// as fallback content, when none are)
    fn flat_children(&self, parent: NodeId) -> Vec<NodeId> {
        let own = |id: NodeId| self.tree.children(id).map(|(id, _)| id).collect::<Vec<_>>();
        if self.tree.has_shadow_roots() {
            if let Some(root) = self.tree.shadow_root(parent) {
                return own(root);
            }
            if self.tag(parent) == "slot" && self.tree.containing_shadow_host(parent).is_some() {
                let assigned = self.tree.assigned_nodes(parent);
                if !assigned.is_empty() {
                    return assigned;
                }
            }
        }
        own(parent)
    }

    /// The element a text node's runs belong to: its parent, or for text
    /// directly in a shadow root, the host
    fn text_parent(&self, dom_parent: NodeId, flat_parent: NodeId) -> NodeId {
        if self.tree.shadow_host(dom_parent).is_some() { flat_parent } else { dom_parent }
    }

    /// Element children with their styles, skipping `display: none`
    /// (and text, which tables do not render outside cells)
    fn styled_children(&mut self, parent: NodeId, parent_style: &Style) -> Vec<(NodeId, Style)> {
        let kids = self.flat_children(parent);
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
                    self.counters_for(Some(child), &cstyle);
                    self.enter(child);
                    let rows = self.row_group(child, &cstyle);
                    self.leave();
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
        self.counters_for(Some(node), &style);
        self.enter(node);
        let cells = self.styled_children(node, &style).into_iter().map(|(c, cs)| self.cell(c, cs)).collect();
        self.leave();
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
        self.generated_item(parent, parent_style, items, text, false);
        self.flex_children(parent, parent_style, items, text);
        self.generated_item(parent, parent_style, items, text, true);
    }

    /// A flex or grid container's generated box, as an item
    fn generated_item(&mut self, parent: NodeId, parent_style: &Style, items: &mut Vec<LayoutBox>, text: &mut InlineBuilder, after: bool) {
        let Some((mut ps, generated)) = self.pseudo_box(parent, parent_style, after) else { return };
        let content = text.take(parent_style);
        if !content.is_white_space() {
            items.push(LayoutBox::anonymous(parent_style, BoxKind::Inline(content)));
        }
        let blockified = ps.display().blockified();
        if blockified != ps.display() {
            std::sync::Arc::make_mut(&mut ps.box_).display = blockified;
        }
        let b = self.generated_block(parent, ps, generated, after);
        items.push(b);
    }

    fn flex_children(&mut self, parent: NodeId, parent_style: &Style, items: &mut Vec<LayoutBox>, text: &mut InlineBuilder) {
        let kids = self.flat_children(parent);
        for child in kids {
            let Some(n) = self.tree.get(child) else { continue };
            if let Some(t) = n.as_text() {
                if !t.is_empty() {
                    text.push_text(t, self.text_parent(n.parent, parent));
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
                self.counters_for(Some(child), &style);
                self.enter(child);
                self.flex_items(child, &style, items, text);
                self.leave();
                continue;
            }
            let content = text.take(parent_style);
            if !content.is_white_space() {
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
            "img" | "canvas" | "video" | "iframe" | "embed" | "object" | "svg" | "textarea" | "select" | "progress" | "meter" => true,
            "input" => !matches!(self.tree.get_attribute(node, "type").map(|t| t.to_ascii_lowercase()).as_deref(), Some("hidden")),
            _ => false,
        }
    }

    fn replaced(&mut self, node: NodeId, style: &Style) -> Option<Replaced> {
        if !self.is_replaced(node) {
            return None;
        }
        let attr_px = |name: &str| self.tree.get_attribute(node, name).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok()).filter(|v| *v >= 0.0);
        let (w_attr, h_attr) = (attr_px("width"), attr_px("height"));
        let tag = self.tag(node).to_string();
        let mut image = None;
        let (what, natural) = match tag.as_str() {
            "img" => {
                let natural = match self.styler.image(self.tree, node) {
                    Some((size, handle)) => {
                        image = Some(handle);
                        Some(size)
                    }
                    // Not (yet) loaded: the size its attributes give, or
                    // nothing
                    None => self.styler.natural_size(self.tree, node).or(Some((w_attr.unwrap_or(0.0), h_attr.unwrap_or(0.0)))),
                };
                (ReplacedWhat::Image, natural)
            }
            "canvas" => (ReplacedWhat::Canvas, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "video" => (ReplacedWhat::Video, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "iframe" | "embed" | "object" => (ReplacedWhat::Frame, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            "svg" => match self.styler.inline_svg(self.tree, node, style) {
                Some((size, handle)) => {
                    image = Some(handle);
                    (ReplacedWhat::Image, Some((w_attr.unwrap_or(size.0), h_attr.unwrap_or(size.1))))
                }
                None => (ReplacedWhat::Svg, Some((w_attr.unwrap_or(300.0), h_attr.unwrap_or(150.0)))),
            },
            "select" => {
                let (shown, options) = self.options(node);
                let size = self.tree.get_attribute(node, "size").and_then(|v| v.trim().parse::<u32>().ok()).filter(|&n| n > 0);
                let multiple = self.tree.get_attribute(node, "multiple").is_some();
                // A list box: `multiple` (4 rows) or a size over 1
                let rows = match size {
                    Some(n) if n > 1 => Some(n),
                    Some(_) if !multiple => None,
                    _ if multiple => Some(size.unwrap_or(4)),
                    _ => None,
                };
                (ReplacedWhat::Select { shown, options, rows }, None)
            }
            "textarea" => {
                let text: String = self.tree.children(node).filter_map(|(_, n)| n.as_text()).collect();
                let (text, placeholder) = if text.is_empty() {
                    (self.tree.get_attribute(node, "placeholder").unwrap_or("").to_string(), true)
                } else {
                    (text, false)
                };
                let count = |name: &str, default: u32| self.tree.get_attribute(node, name).and_then(|v| v.trim().parse::<u32>().ok()).filter(|&n| n > 0).unwrap_or(default);
                (ReplacedWhat::TextArea { text, placeholder, cols: count("cols", 20), rows: count("rows", 2) }, None)
            }
            "progress" | "meter" => {
                let num = |name: &str| self.tree.get_attribute(node, name).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
                let meter = tag == "meter";
                let value = if meter {
                    let (min, max) = (num("min").unwrap_or(0.0), num("max").unwrap_or(1.0));
                    let v = num("value").unwrap_or(0.0).clamp(min, max.max(min));
                    Some(if max > min { (v - min) / (max - min) } else { 0.0 })
                } else {
                    let max = num("max").filter(|&m| m > 0.0).unwrap_or(1.0);
                    num("value").map(|v| (v / max).clamp(0.0, 1.0))
                };
                (ReplacedWhat::Gauge { value, meter }, None)
            }
            _ => {
                let ty = self.tree.get_attribute(node, "type").map(|t| t.to_ascii_lowercase()).unwrap_or_default();
                match ty.as_str() {
                    "checkbox" | "radio" => {
                        let checked = self.tree.get_attribute(node, "checked").is_some();
                        (ReplacedWhat::Check { radio: ty == "radio", checked }, Some((13.0, 13.0)))
                    }
                    "image" => match self.styler.image(self.tree, node) {
                        Some((size, handle)) => {
                            image = Some(handle);
                            (ReplacedWhat::Image, Some(size))
                        }
                        None => (ReplacedWhat::Image, Some((w_attr.unwrap_or(0.0), h_attr.unwrap_or(0.0)))),
                    },
                    "submit" | "reset" | "button" => {
                        let label = match self.tree.get_attribute(node, "value") {
                            Some(v) => v.to_string(),
                            None if ty == "submit" => "Submit".to_string(),
                            None if ty == "reset" => "Reset".to_string(),
                            None => String::new(),
                        };
                        (ReplacedWhat::Button(label), None)
                    }
                    "file" => (ReplacedWhat::File, None),
                    "range" => {
                        let num = |name: &str, default: f32| self.tree.get_attribute(node, name).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(default);
                        let (min, max) = (num("min", 0.0), num("max", 100.0));
                        let value = num("value", min + (max - min) / 2.0);
                        let at = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
                        (ReplacedWhat::Range(at), None)
                    }
                    "color" => {
                        let value = self.tree.get_attribute(node, "value").unwrap_or("#000000");
                        let c = fos_css::properties::Color::from_hex(value.trim().trim_start_matches('#')).unwrap_or(fos_css::properties::Color::BLACK);
                        (ReplacedWhat::ColorWell([c.r, c.g, c.b, c.a]), None)
                    }
                    _ => {
                        let value = self.tree.get_attribute(node, "value").unwrap_or("");
                        let (shown, placeholder) = if value.is_empty() {
                            (self.tree.get_attribute(node, "placeholder").unwrap_or("").to_string(), true)
                        } else if ty == "password" {
                            ("\u{2022}".repeat(value.chars().count()), false)
                        } else {
                            (value.to_string(), false)
                        };
                        let size = self.tree.get_attribute(node, "size").and_then(|v| v.trim().parse::<u32>().ok()).filter(|&n| n > 0).unwrap_or(20);
                        (ReplacedWhat::TextField(shown, placeholder, size), None)
                    }
                }
            }
        };
        // Width and height attributes give the size an image lacks
        let ratio_only = w_attr.is_none() && h_attr.is_none() && image.as_ref().is_some_and(|i| self.styler.ratio_only(i));
        let no_ratio = match what {
            ReplacedWhat::Video => w_attr.is_none() || h_attr.is_none(),
            ReplacedWhat::Frame => true,
            _ => false,
        };
        Some(Replaced { natural, what, image, ratio_only, no_ratio })
    }

    /// A select's shown option text (the selected option's, else the
    /// first's) and every option's text
    fn options(&self, select: NodeId) -> (String, Vec<String>) {
        let mut shown = None;
        let mut all = Vec::new();
        let mut stack = vec![select];
        while let Some(n) = stack.pop() {
            let kids: Vec<NodeId> = self.tree.children(n).map(|(id, _)| id).collect();
            for k in kids.into_iter().rev() {
                stack.push(k);
            }
            if self.tag(n) == "option" && all.len() < 1000 {
                let text: String = self.tree.children(n).filter_map(|(_, c)| c.as_text()).collect();
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if shown.is_none() && self.tree.get_attribute(n, "selected").is_some() {
                    shown = Some(text.clone());
                }
                all.push(text);
            }
        }
        (shown.or_else(|| all.first().cloned()).unwrap_or_default(), all)
    }

    fn marker(&mut self, node: NodeId, style: &Style) -> Option<Marker> {
        // `::marker` styles it, and its `content` replaces the text
        let styled = self.styler.pseudo(self.tree, node, fos_dom::PseudoElement::Marker, style);
        let text = match styled.as_ref().and_then(|s| s.box_.content.as_ref().map(|c| (s, c.is_empty()))) {
            Some((_, true)) => return None,
            Some((ms, false)) => generated_text(self.tree, node, ms, &self.counters),
            None => {
                let ordinal = self.counters.iter().rev().find(|c| &*c.0 == "list-item").map_or(1, |c| c.1);
                marker_text(style.inherited.list_style_type, ordinal)?
            }
        };
        let mut mstyle = styled.unwrap_or_else(|| Style::inherit_from(style));
        // Markers keep the list item's font but not its decorations
        std::sync::Arc::make_mut(&mut mstyle.inherited).white_space = WhiteSpace::Pre;
        Some(Marker { text, style: mstyle, outside: style.inherited.list_style_position == ListStylePosition::Outside })
    }
}

/// A counter's value in list style `kind` (a `ListStyleType`
/// discriminant; decimal when absent)
pub fn counter_text(kind: Option<u8>, n: i32) -> String {
    let kind = kind.and_then(ListStyleType::from_u8).unwrap_or(ListStyleType::Decimal);
    match marker_text(kind, n) {
        None => String::new(),
        Some(t) => {
            let t = t.trim_end();
            // Bullets stand alone; numbers lose the marker's period
            if matches!(kind, ListStyleType::Disc | ListStyleType::Circle | ListStyleType::Square) { t.to_string() } else { t.trim_end_matches('.').to_string() }
        }
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
