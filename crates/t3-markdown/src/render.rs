//! Document -> GPUI elements, reproducing `.chat-markdown` (fork `index.css:696-1038`) and the
//! `ChatMarkdown.tsx` chrome (code block header, table footer, details, file chips).
//!
//! CSS vertical margins collapse; taffy's flexbox does not, so blocks report their margins
//! ([`Laid`]) and [`stack`] inserts the collapsed gaps.

use std::{collections::HashMap, rc::Rc};

use gpui_kit::{
    AnyElement, Context, Div, FontStyle, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, TextAlign, Window, div, font, img, prelude::FluentBuilder as _, px, relative,
    size,
};
use t3_highlight::{Highlighted, Theme};
use t3_ui::{Icon, IconName};

use crate::{
    copy::{CopyFormat, Markup},
    document::{
        ATOM_CHAR, Align, Atom, AtomKind, Block, CodeBlock, Details, Footnote, Inline, InlineStyle,
        List, Span, Table,
    },
    inline_text::{
        AtomSlot, Decorations, InlineText, LinkHandler, LinkSlot, RunStyle, TextContent, TextWrap,
    },
    style::{MarkdownStyle, metrics},
    view::Markdown,
};

/// A rendered block with its CSS margins. `own` are the block's own margins; `inner` the
/// margins of its first/last children that collapse through it (no padding or border).
struct Laid {
    element: AnyElement,
    own: (f32, f32),
    inner: (f32, f32),
}

impl Laid {
    fn new(element: impl IntoElement, own: (f32, f32)) -> Self {
        Self {
            element: element.into_any_element(),
            own,
            inner: (0., 0.),
        }
    }

    fn top(&self) -> f32 {
        self.own.0.max(self.inner.0)
    }

    fn bottom(&self) -> f32 {
        self.own.1.max(self.inner.1)
    }
}

/// Stacks blocks vertically with collapsed margins between them. Returns the children (with
/// spacers) plus the leading and trailing margins left for the container to collapse or apply.
fn stack(blocks: Vec<Laid>) -> (Vec<AnyElement>, f32, f32) {
    let leading = blocks.first().map_or(0., Laid::top);
    let trailing = blocks.last().map_or(0., Laid::bottom);
    let mut children = Vec::with_capacity(blocks.len() * 2);
    let mut previous_bottom: Option<f32> = None;
    for block in blocks {
        if let Some(bottom) = previous_bottom {
            let gap = bottom.max(block.top());
            if gap > 0. {
                children.push(div().flex_none().h(px(gap)).into_any_element());
            }
        }
        previous_bottom = Some(block.bottom());
        children.push(block.element);
    }
    (children, leading, trailing)
}

/// A container whose children's outer margins collapse through it (no vertical padding/border).
fn collapsing(container: Div, blocks: Vec<Laid>, own: (f32, f32)) -> Laid {
    let (children, leading, trailing) = stack(blocks);
    Laid {
        element: container.children(children).into_any_element(),
        own,
        inner: (leading, trailing),
    }
}

/// A container with vertical padding or border: child margins stay inside as spacing.
fn enclosing(container: Div, blocks: Vec<Laid>) -> Div {
    let (children, leading, trailing) = stack(blocks);
    container
        .when(leading > 0., |this| {
            this.child(div().flex_none().h(px(leading)))
        })
        .children(children)
        .when(trailing > 0., |this| {
            this.child(div().flex_none().h(px(trailing)))
        })
}

/// Inherited text properties while rendering.
#[derive(Clone)]
struct Ctx {
    style: Rc<MarkdownStyle>,
    color: Hsla,
    size: Pixels,
    line_height: Pixels,
    /// Byte offset of the chunk being rendered, so block ids are message-global.
    base: usize,
    in_tail: bool,
    ul_depth: usize,
    ol_depth: usize,
    suffixes: Rc<HashMap<String, String>>,
    theme: Theme,
    /// Copy-as-markdown framing: prefix of the next block's first line (list marker), and of
    /// every other line (quote `> `, list continuation indent).
    copy_prefix: String,
    copy_indent: String,
}

impl Ctx {
    /// The copy framing for a block in this context.
    fn copy_format(&self, gap: bool) -> CopyFormat {
        CopyFormat {
            first_prefix: self.copy_prefix.clone(),
            line_prefix: self.copy_indent.clone(),
            fence: None,
            block_gap: gap,
        }
    }

    /// The context for blocks after the first in a container: no marker any more.
    fn continued(&self) -> Self {
        Self {
            copy_prefix: self.copy_indent.clone(),
            ..self.clone()
        }
    }
}

/// Monotonic element ids within one render pass.
struct Ids(usize);

impl Ids {
    fn next(&mut self, name: &'static str) -> gpui_kit::ElementId {
        self.0 += 1;
        gpui_kit::ElementId::NamedInteger(name.into(), self.0 as u64)
    }
}

/// Renders a whole message.
pub(crate) fn render_markdown(
    this: &mut Markdown,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let style = Rc::new(this.resolved_style(cx));
    let colors = &style.colors;
    let color = if this.options().full_foreground {
        colors.foreground
    } else {
        colors.foreground.opacity(0.8)
    };
    let line_height = metrics::BODY_SIZE * metrics::RELAXED;
    let mut ctx = Ctx {
        style: style.clone(),
        color,
        size: metrics::BODY_SIZE,
        line_height,
        base: 0,
        in_tail: false,
        ul_depth: 0,
        ol_depth: 0,
        suffixes: this.suffixes(),
        theme: if colors.is_dark {
            Theme::Dark
        } else {
            Theme::Light
        },
        copy_prefix: String::new(),
        copy_indent: String::new(),
    };
    let mut ids = Ids(0);
    let chunks: Vec<_> = this.chunks().to_vec();
    let last = chunks.len().saturating_sub(1);
    let mut blocks = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        ctx.base = chunk.start;
        ctx.in_tail = this.is_streaming() && index == last;
        for block in &chunk.document.blocks {
            blocks.push(render_block(this, &ctx, &mut ids, block, window, cx));
        }
    }
    // `.chat-markdown > :first-child { margin-top: 0 }` and `:last-child { margin-bottom: 0 }`.
    if let Some(first) = blocks.first_mut() {
        first.own.0 = 0.;
    }
    if let Some(last) = blocks.last_mut() {
        last.own.1 = 0.;
    }
    let (children, leading, trailing) = stack(blocks);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .font_family(style.sans_family.clone())
        .text_size(metrics::BODY_SIZE)
        .line_height(line_height)
        .text_color(color)
        .when(leading > 0., |this| this.pt(px(leading)))
        .when(trailing > 0., |this| this.pb(px(trailing)))
        .children(children)
        .into_any_element()
}

fn render_blocks(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    blocks: &[Block],
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> Vec<Laid> {
    let continued = ctx.continued();
    blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let ctx = if index == 0 { ctx } else { &continued };
            render_block(this, ctx, ids, block, window, cx)
        })
        .collect()
}

fn render_block(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    block: &Block,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> Laid {
    let margin = metrics::BLOCK_MARGIN;
    match block {
        Block::Paragraph { content, tight } => {
            let own = if *tight { (0., 0.) } else { (margin, margin) };
            let kind = if *tight {
                TextKind::Tight
            } else {
                TextKind::Body
            };
            Laid::new(paragraph(this, ctx, ids, content, kind, window, cx), own)
        }
        Block::Heading { level, content } => {
            let size = match level {
                1 => px(20.),
                2 => px(18.),
                3 => px(16.),
                _ => px(14.),
            };
            let colors = &ctx.style.colors;
            let heading = Ctx {
                copy_prefix: format!("{}{} ", ctx.copy_prefix, "#".repeat(usize::from(*level))),
                color: if *level == 6 {
                    colors.muted_foreground
                } else {
                    colors.foreground
                },
                size,
                line_height: size * metrics::HEADING_LINE_HEIGHT,
                ..ctx.clone()
            };
            let element = inline(this, &heading, ids, content, TextKind::Heading, window, cx);
            Laid::new(
                element,
                (metrics::HEADING_MARGIN_TOP, metrics::HEADING_MARGIN_BOTTOM),
            )
        }
        Block::Quote(children) => {
            let quote = Ctx {
                color: ctx.style.colors.muted_foreground,
                copy_prefix: format!("{}> ", ctx.copy_prefix),
                copy_indent: format!("{}> ", ctx.copy_indent),
                ..ctx.clone()
            };
            let inner = render_blocks(this, &quote, ids, children, window, cx);
            collapsing(
                div()
                    .flex()
                    .flex_col()
                    .border_l(px(metrics::QUOTE_BORDER))
                    .border_color(ctx.style.colors.border)
                    .pl(px(metrics::QUOTE_PADDING))
                    .text_color(quote.color),
                inner,
                (margin, margin),
            )
        }
        Block::List(list) => render_list(this, ctx, ids, list, window, cx),
        Block::Code(code) => Laid::new(code_block(this, ctx, code, window, cx), (margin, margin)),
        Block::Table(table) => Laid::new(
            render_table(this, ctx, ids, table, window, cx),
            (margin, margin),
        ),
        Block::Rule => Laid::new(
            div()
                .flex_none()
                .w_full()
                .h(px(1.))
                .bg(ctx.style.colors.border),
            (0., 0.),
        ),
        Block::Details(details) => Laid::new(
            render_details(this, ctx, ids, details, window, cx),
            (8., 8.),
        ),
        Block::Footnotes(notes) => Laid::new(
            render_footnotes(this, ctx, ids, notes, window, cx),
            (20., 0.),
        ),
    }
}

/// A paragraph; images split it like the fork's `display: block` images.
fn paragraph(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    content: &Inline,
    kind: TextKind,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let images: Vec<&Atom> = content
        .atoms
        .iter()
        .filter(|atom| matches!(atom.kind, AtomKind::Image { .. }))
        .collect();
    if images.is_empty() {
        return inline(this, ctx, ids, content, kind, window, cx);
    }
    let mut parts = div().flex().flex_col().w_full();
    let mut start = 0;
    for image in images {
        let before = slice_inline(content, start..image.offset);
        if !before.text.trim().is_empty() {
            parts = parts.child(inline(this, ctx, ids, &before, kind, window, cx));
        }
        if let AtomKind::Image { url, .. } = &image.kind {
            parts = parts.child(img(SharedString::from(url.clone())).max_w_full());
        }
        start = image.offset + ATOM_CHAR.len_utf8();
    }
    let after = slice_inline(content, start..content.text.len());
    if !after.text.trim().is_empty() {
        parts = parts.child(inline(this, ctx, ids, &after, kind, window, cx));
    }
    parts.into_any_element()
}

/// The part of `content` in `range`, with spans, links and atoms rebased.
fn slice_inline(content: &Inline, range: std::ops::Range<usize>) -> Inline {
    let clip = |inner: &std::ops::Range<usize>| {
        let start = inner.start.max(range.start);
        let end = inner.end.min(range.end);
        (start < end).then(|| start - range.start..end - range.start)
    };
    Inline {
        text: content.text[range.clone()].to_string(),
        spans: content
            .spans
            .iter()
            .filter_map(|span| {
                clip(&span.range).map(|range| Span {
                    range,
                    style: span.style,
                })
            })
            .collect(),
        links: content
            .links
            .iter()
            .filter_map(|link| {
                clip(&link.range).map(|clipped| crate::document::Link {
                    range: clipped,
                    href: link.href.clone(),
                    nowrap_until: link
                        .nowrap_until
                        .filter(|lead| range.contains(lead))
                        .map(|lead| lead - range.start),
                })
            })
            .collect(),
        atoms: content
            .atoms
            .iter()
            .filter(|atom| range.contains(&atom.offset))
            .map(|atom| Atom {
                offset: atom.offset - range.start,
                kind: atom.kind.clone(),
            })
            .collect(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextKind {
    /// A paragraph with margins.
    Body,
    /// The bare text of a tight list item.
    Tight,
    Heading,
    /// A table cell; `header` cells are bold and never wrap.
    Cell {
        header: bool,
        collapsed: bool,
    },
    /// A list marker (`::marker` is not selectable).
    Marker,
    /// A `<details>` summary (`font-medium`).
    Summary,
}

/// Builds the [`InlineText`] for inline content in the current context.
fn inline(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    content: &Inline,
    kind: TextKind,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    aligned_inline(this, ctx, ids, content, kind, TextAlign::Left, window, cx)
}

/// [`inline`] with a `text-align` (table cells).
#[allow(clippy::too_many_arguments)]
fn aligned_inline(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    content: &Inline,
    kind: TextKind,
    align: TextAlign,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let colors = &ctx.style.colors;
    let base_weight = match kind {
        TextKind::Heading | TextKind::Cell { header: true, .. } => FontWeight::SEMIBOLD,
        TextKind::Summary => FontWeight::MEDIUM,
        _ => FontWeight::NORMAL,
    };
    let wrap = match kind {
        TextKind::Cell { header: true, .. }
        | TextKind::Cell {
            collapsed: true, ..
        } => TextWrap::Truncate,
        TextKind::Cell { .. } => TextWrap::Normal,
        _ => TextWrap::Normal,
    };
    let sans = ctx.style.sans_family.clone();
    let mono = ctx.style.mono_family.clone();
    let run_style = |style: &InlineStyle, in_link: bool| {
        let mut weight = base_weight;
        if style.bold {
            // `font-weight: bolder`.
            weight = if base_weight.0 >= 600. {
                FontWeight::BLACK
            } else {
                FontWeight::BOLD
            };
        }
        let mut run_font = font(if style.code || style.mono {
            mono.clone()
        } else {
            sans.clone()
        });
        run_font.weight = weight;
        if style.italic {
            run_font.style = FontStyle::Italic;
        }
        let mut size = if style.code {
            metrics::INLINE_CODE_SIZE
        } else {
            ctx.size
        };
        let mut shift = Pixels::ZERO;
        if style.superscript || style.subscript {
            size *= 0.75;
            shift = if style.superscript {
                size * 0.5
            } else {
                -(size * 0.25)
            };
        }
        let color = if style.code {
            colors.foreground
        } else if in_link {
            colors.info_foreground
        } else {
            ctx.color
        };
        RunStyle {
            font: run_font,
            size,
            color,
            strikethrough: style.strike,
            code_box: style.code,
            shift,
        }
    };

    let mut runs = Vec::with_capacity(content.spans.len());
    for span in &content.spans {
        // Split spans at link boundaries so link text takes the link color.
        let mut start = span.range.start;
        while start < span.range.end {
            let link = content
                .links
                .iter()
                .find(|link| link.range.contains(&start));
            let end = match link {
                Some(link) => link.range.end.min(span.range.end),
                None => content
                    .links
                    .iter()
                    .map(|link| link.range.start)
                    .filter(|link_start| *link_start > start)
                    .min()
                    .unwrap_or(span.range.end)
                    .min(span.range.end),
            };
            runs.push((start..end, run_style(&span.style, link.is_some())));
            start = end;
        }
    }

    let mut atoms = Vec::new();
    let mut slots = Vec::new();
    let text_system = cx.text_system().clone();
    let strut_font = {
        let mut strut = font(sans.clone());
        strut.weight = base_weight;
        strut
    };
    let strut_id = text_system.resolve_font(&strut_font);
    let x_height = text_system.x_height(strut_id, ctx.size);
    for atom in &content.atoms {
        let in_link = content
            .links
            .iter()
            .any(|link| link.range.contains(&atom.offset));
        let (slot, element) = atom_element(this, ctx, atom, in_link, x_height, window, cx);
        slots.push(slot);
        atoms.push(element);
    }

    let copy = match kind {
        TextKind::Body | TextKind::Heading => ctx.copy_format(true),
        TextKind::Tight => ctx.copy_format(false),
        TextKind::Cell { .. } | TextKind::Marker | TextKind::Summary => CopyFormat::default(),
    };
    let text_content = TextContent {
        text: SharedString::from(content.text.clone()),
        runs,
        links: content
            .links
            .iter()
            .map(|link| LinkSlot {
                range: link.range.clone(),
                href: SharedString::from(link.href.clone()),
                nowrap_until: link.nowrap_until,
            })
            .collect(),
        atoms: slots,
        strut_font,
        strut_size: ctx.size,
        line_height: ctx.line_height,
        wrap,
        align,
        decorations: Decorations {
            code_background: colors.muted,
            code_border: colors.border,
            selection: colors.selection,
        },
        markup: inline_markup(content),
        copy,
    };
    let entity = cx.entity().downgrade();
    let on_link: LinkHandler = Rc::new(move |href, _, cx| {
        let href = href.clone();
        let _ = entity.update(cx, |this, cx| this.emit_url(href, cx));
    });
    InlineText::new(ids.next("md-text"), Rc::new(text_content), atoms)
        .on_link(on_link)
        .selectable(kind != TextKind::Marker)
        .into_any_element()
}

/// The markup copy-as-markdown re-applies: style flags merged into ranges, plus links.
fn inline_markup(content: &Inline) -> Vec<(std::ops::Range<usize>, Markup)> {
    let mut out = Vec::new();
    type Flag = fn(&InlineStyle) -> bool;
    let flags: [(Flag, Markup); 4] = [
        (|style| style.bold, Markup::Bold),
        (|style| style.italic, Markup::Italic),
        (|style| style.strike, Markup::Strike),
        (|style| style.code, Markup::Code),
    ];
    for (flag, markup) in flags {
        let mut open: Option<std::ops::Range<usize>> = None;
        for span in &content.spans {
            match (&mut open, flag(&span.style)) {
                (Some(range), true) if range.end == span.range.start => range.end = span.range.end,
                (_, true) => {
                    if let Some(range) = open.take() {
                        out.push((range, markup.clone()));
                    }
                    open = Some(span.range.clone());
                }
                (_, false) => {
                    if let Some(range) = open.take() {
                        out.push((range, markup.clone()));
                    }
                }
            }
        }
        if let Some(range) = open {
            out.push((range, markup.clone()));
        }
    }
    out.extend(
        content
            .links
            .iter()
            .map(|link| (link.range.clone(), Markup::Link(link.href.clone()))),
    );
    out
}

/// Lays out one atom: its slot geometry and the element drawn there.
fn atom_element(
    this: &mut Markdown,
    ctx: &Ctx,
    atom: &Atom,
    in_link: bool,
    x_height: Pixels,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> (AtomSlot, AnyElement) {
    let colors = &ctx.style.colors;
    let slot = |width: Pixels, height: Pixels, top: Pixels, copy: SharedString| AtomSlot {
        offset: atom.offset,
        size: size(width, height),
        margin_left: Pixels::ZERO,
        margin_right: Pixels::ZERO,
        top_from_baseline: top,
        copy_text: copy,
    };
    match &atom.kind {
        AtomKind::Favicon { host } => {
            // `.chat-markdown-link-favicon`: 14px, margin-inline 0.25em 0.2em, vertical-align
            // -0.125em (the icon's bottom sits 0.125em below the baseline). Google's favicon
            // service, with the globe while loading or when the host has no favicon.
            let mut favicon = slot(
                px(metrics::FAVICON),
                px(metrics::FAVICON),
                ctx.size * 0.125 - px(metrics::FAVICON),
                SharedString::default(),
            );
            favicon.margin_left = ctx.size * 0.25;
            favicon.margin_right = ctx.size * 0.2;
            let color = if in_link {
                colors.info_foreground
            } else {
                ctx.color
            };
            let source = format!(
                "https://www.google.com/s2/favicons?domain={}&sz=32",
                url_encode(host)
            );
            let globe =
                move || icon(IconName::Globe, px(metrics::FAVICON), color).into_any_element();
            let element = img(SharedString::from(source))
                .size(px(metrics::FAVICON))
                .rounded(px(6.))
                .with_loading(globe)
                .with_fallback(globe)
                .into_any_element();
            (favicon, element)
        }
        AtomKind::FileChip { link, href } => {
            let mut label = link.basename.clone();
            if let Some(suffix) = ctx.suffixes.get(&link.file_path) {
                label.push_str(" · ");
                label.push_str(suffix);
            }
            if let Some(line) = link.line {
                label.push_str(&format!(" · L{line}"));
                if let Some(column) = link.column {
                    label.push_str(&format!(":C{column}"));
                }
            }
            let label = SharedString::from(label);
            let mut label_font = font(ctx.style.sans_family.clone());
            label_font.weight = FontWeight::MEDIUM;
            let label_width = text_width(&label, &label_font, metrics::CHIP_LABEL_SIZE, window);
            let width =
                px(2. * (1. + metrics::CHIP_PADDING_X) + metrics::CHIP_ICON + metrics::CHIP_GAP)
                    + label_width;
            // Content is the 14px icon or the 15px label line (`leading-tight`).
            let height = px(2. * (1. + metrics::CHIP_PADDING_Y) + 15.);
            // `vertical-align: middle`: centered on the parent's baseline + half its x-height.
            let top = -(x_height / 2.) - height / 2.;
            let link_value = link.clone();
            let entity = cx.entity().downgrade();
            let element = div()
                .id(SharedString::from(format!("md-chip-{}", atom.offset)))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(metrics::CHIP_GAP))
                .w(width)
                .h(height)
                .px(px(metrics::CHIP_PADDING_X))
                .py(px(metrics::CHIP_PADDING_Y))
                .rounded(px(metrics::CHIP_RADIUS))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.accent.opacity(0.4))
                .hover(|style| style.bg(colors.accent.opacity(0.7)))
                .cursor_pointer()
                .text_size(metrics::CHIP_LABEL_SIZE)
                .line_height(px(15.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(file_icon(
                    &link.file_path,
                    colors.is_dark,
                    px(metrics::CHIP_ICON),
                ))
                .child(div().flex_none().whitespace_nowrap().child(label.clone()))
                .on_click(move |_, _, cx| {
                    let link = link_value.clone();
                    let _ = entity.update(cx, |this, cx| this.emit_file(link, cx));
                })
                .into_any_element();
            let _ = this;
            // `data-markdown-copy`: `[basename](href)`.
            let copy = SharedString::from(format!("[{}]({href})", link.basename));
            (slot(width, height, top, copy), element)
        }
        AtomKind::FootnoteRef { number } | AtomKind::FootnoteBackref { number } => {
            // `<sup><a data-footnote-ref>`: inline-flex, min-width 1rem, centered, 11px semibold,
            // raised 0.5em of the sup's 75% font size.
            let text: SharedString = match atom.kind {
                AtomKind::FootnoteRef { .. } => number.to_string().into(),
                _ => "↩".into(),
            };
            let mut label_font = font(ctx.style.sans_family.clone());
            label_font.weight = FontWeight::SEMIBOLD;
            let text_size = px(11.);
            let width = text_width(&text, &label_font, text_size, window).max(px(16.));
            let line = text_size * metrics::RELAXED;
            let raise = if matches!(atom.kind, AtomKind::FootnoteRef { .. }) {
                ctx.size * 0.75 * 0.5
            } else {
                Pixels::ZERO
            };
            let (ascent, descent) =
                crate::inline_text::font_metrics(cx.text_system(), &label_font, text_size);
            let top = -raise - ascent - (line - ascent - descent) / 2.;
            let element = div()
                .flex()
                .justify_center()
                .w(width)
                .h(line)
                .text_size(text_size)
                .line_height(line)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.info_foreground)
                .child(text.clone())
                .into_any_element();
            (slot(width, line, top, text), element)
        }
        AtomKind::TaskCheckbox { checked } => {
            // A disabled native checkbox: 13px, `margin: 0 0.35em 0.15em -1.25rem`,
            // `vertical-align: middle` (its margin box centered on baseline + x-height/2).
            let box_size = px(13.);
            let margin_bottom = ctx.size * 0.15;
            let mut checkbox = slot(
                box_size,
                box_size,
                -(x_height / 2.) - (box_size + margin_bottom) / 2.,
                if *checked { "[x]".into() } else { "[ ]".into() },
            );
            checkbox.margin_left = px(-metrics::LIST_INDENT);
            checkbox.margin_right = ctx.size * 0.35;
            // Chromium's disabled native checkbox under the page's `color-scheme`.
            let (fill, border, mark) = match (colors.is_dark, *checked) {
                (true, true) => (0x757575ff, 0x757575ff, 0x3b3b3bff),
                (true, false) => (0x3b3b3bff, 0x626262ff, 0),
                (false, true) => (0xd1d1d1ff, 0xd1d1d1ff, 0xedededff),
                (false, false) => (0xf8f8f8ff, 0xd1d1d1ff, 0),
            };
            let element = div()
                .flex()
                .items_center()
                .justify_center()
                .size(box_size)
                .rounded(px(2.))
                .border_1()
                .border_color(gpui_kit::rgba(border))
                .bg(gpui_kit::rgba(fill))
                .when(*checked, |this| {
                    this.child(icon(IconName::Check, px(11.), gpui_kit::rgba(mark).into()))
                })
                .into_any_element();
            (checkbox, element)
        }
        AtomKind::Image { .. } => (
            slot(
                Pixels::ZERO,
                Pixels::ZERO,
                Pixels::ZERO,
                SharedString::default(),
            ),
            div().into_any_element(),
        ),
    }
}

/// `encodeURIComponent` for a host name.
fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Shaped width of a single-style string.
fn text_width(text: &str, run_font: &gpui_kit::Font, size: Pixels, window: &mut Window) -> Pixels {
    let runs = [gpui_kit::TextRun {
        len: text.len(),
        font: run_font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    window
        .text_system()
        .layout_line(text, size, &runs, None)
        .width
}

fn render_list(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    list: &List,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> Laid {
    let ordered = list.start.is_some();
    let nested = Ctx {
        ul_depth: ctx.ul_depth + usize::from(!ordered),
        ol_depth: ctx.ol_depth + usize::from(ordered),
        ..ctx.clone()
    };
    let mut items = Vec::new();
    for (index, item) in list.items.iter().enumerate() {
        let mut blocks: Vec<Laid> = Vec::new();
        let mut item_blocks = item.blocks.clone();
        if let Some(checked) = item.task {
            // The checkbox renders inline before the first paragraph's text.
            if let Some(Block::Paragraph { content, .. }) = item_blocks.first_mut() {
                *content = with_checkbox(content, checked);
            } else {
                item_blocks.insert(
                    0,
                    Block::Paragraph {
                        content: with_checkbox(&Inline::default(), checked),
                        tight: true,
                    },
                );
            }
        }
        let marker = if ordered {
            format!("{}. ", list.start.unwrap_or(1) + index as u64)
        } else {
            "- ".to_string()
        };
        let item_ctx = Ctx {
            copy_prefix: format!("{}{marker}", ctx.copy_indent),
            copy_indent: format!("{}{}", ctx.copy_indent, " ".repeat(marker.len())),
            ..nested.clone()
        };
        blocks.extend(render_blocks(
            this,
            &item_ctx,
            ids,
            &item_blocks,
            window,
            cx,
        ));
        let mut li = div().relative().flex().flex_col().w_full();
        if item.task.is_none() {
            let marker = if ordered {
                let number = list.start.unwrap_or(1) + index as u64;
                ordered_marker(this, ctx, ids, number, window, cx)
            } else {
                bullet_marker(ctx, cx)
            };
            li = li.child(marker);
        }
        let mut laid = collapsing(li, blocks, (0., 0.));
        if index > 0 {
            laid.own.0 = metrics::LIST_ITEM_GAP;
        }
        items.push(laid);
    }
    collapsing(
        div().flex().flex_col().pl(px(metrics::LIST_INDENT)),
        items,
        (metrics::BLOCK_MARGIN, metrics::BLOCK_MARGIN),
    )
}

/// An ordered `::marker` (`1. ` / `a. ` / `i. ` by `ol` depth), right-aligned to the item's
/// content edge on its first line.
fn ordered_marker(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    number: u64,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let label = match ctx.ol_depth {
        0 => number.to_string(),
        1 => alpha(number),
        _ => roman(number),
    };
    let text = format!("{label}.\u{a0}");
    let marker = Inline {
        spans: vec![Span {
            range: 0..text.len(),
            style: InlineStyle::default(),
        }],
        text,
        ..Inline::default()
    };
    let element = inline(this, ctx, ids, &marker, TextKind::Marker, window, cx);
    div()
        .absolute()
        .top_0()
        .right(relative(1.))
        .whitespace_nowrap()
        .child(element)
        .into_any_element()
}

/// A `disc` / `circle` / `square` marker by `ul` depth. Chromium paints these as shapes, not
/// glyphs: `size = (ascent * 2/3 + 1) / 2` px, top at `ascent / 2` below the line's ascent line,
/// right edge 8px before the content (measured against the fork).
fn bullet_marker(ctx: &Ctx, cx: &mut Context<Markdown>) -> AnyElement {
    let (ascent, descent) = crate::inline_text::font_metrics(
        cx.text_system(),
        &font(ctx.style.sans_family.clone()),
        ctx.size,
    );
    let ascent = f32::from(ascent) as i32;
    let size = (ascent * 2 / 3 + 1) / 2;
    let offset = 3 * (ascent - ascent * 2 / 3) / 2;
    let baseline = (ctx.line_height - px(ascent as f32) - descent) / 2. + px(ascent as f32);
    let top = baseline - px((ascent - offset) as f32);
    let size = px(size as f32);
    let shape = div().absolute().left(-(size + px(8.))).top(top).size(size);
    match ctx.ul_depth {
        0 => shape.rounded_full().bg(ctx.color),
        // `circle` is a 1px stroke centered on the shape's edge.
        1 => div()
            .absolute()
            .left(-(size + px(8.5)))
            .top(top - px(0.5))
            .size(size + px(1.))
            .rounded_full()
            .border_1()
            .border_color(ctx.color),
        _ => shape.bg(ctx.color),
    }
    .into_any_element()
}

fn alpha(mut number: u64) -> String {
    let mut out = Vec::new();
    while number > 0 {
        number -= 1;
        out.push(b'a' + (number % 26) as u8);
        number /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn roman(mut number: u64) -> String {
    const NUMERALS: [(u64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while number >= value {
            out.push_str(numeral);
            number -= value;
        }
    }
    out
}

/// Prefixes the content with the task checkbox atom and the space after it.
fn with_checkbox(content: &Inline, checked: bool) -> Inline {
    let prefix = format!("{ATOM_CHAR} ");
    let shift = prefix.len();
    let mut out = Inline {
        text: format!("{prefix}{}", content.text),
        spans: vec![Span {
            range: 0..shift,
            style: InlineStyle::default(),
        }],
        links: content
            .links
            .iter()
            .map(|link| crate::document::Link {
                range: link.range.start + shift..link.range.end + shift,
                href: link.href.clone(),
                nowrap_until: link.nowrap_until.map(|lead| lead + shift),
            })
            .collect(),
        atoms: vec![Atom {
            offset: 0,
            kind: AtomKind::TaskCheckbox { checked },
        }],
    };
    out.spans.extend(content.spans.iter().map(|span| Span {
        range: span.range.start + shift..span.range.end + shift,
        style: span.style,
    }));
    out.atoms.extend(content.atoms.iter().map(|atom| Atom {
        offset: atom.offset + shift,
        kind: atom.kind.clone(),
    }));
    out
}

fn code_block(
    this: &mut Markdown,
    ctx: &Ctx,
    block: &CodeBlock,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let colors = ctx.style.colors.clone();
    let id = ctx.base + block.id;
    let highlighted = this.highlight(id, block, ctx.in_tail, ctx.theme);
    let wraps = this.code_wraps(id);
    let copied = this.copied.contains(&id);
    let code_text = expand_tabs(&block.code);
    let mut content = code_content(ctx, &code_text, &highlighted, wraps);
    // `resolveCodeBlockLanguage`: the fence language, omitted for `text`.
    content.copy.fence = Some(if block.language == "text" {
        String::new()
    } else {
        block.language.clone()
    });
    let body = InlineText::new(("md-code", id as u64), Rc::new(content), Vec::new());
    let entity = cx.entity().downgrade();
    let wrap_entity = entity.clone();
    let copy_value = format!("{}\n", block.code);

    let title = code_title(ctx, block, &colors);
    let actions = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.))
        .child(
            chrome_button(
                ("md-code-wrap", id as u64),
                IconName::TextWrap,
                wraps,
                &colors,
            )
            .on_click(move |_, _, cx| {
                let _ = wrap_entity.update(cx, |this, cx| this.toggle_wrap(id, cx));
            }),
        )
        .child(
            chrome_button(
                ("md-code-copy", id as u64),
                if copied {
                    IconName::Check
                } else {
                    IconName::Copy
                },
                false,
                &colors,
            )
            .on_click(move |_, _, cx| {
                let value = copy_value.clone();
                let _ = entity.update(cx, |this, cx| this.copy(id, value, cx));
            }),
        );
    let [pt, pr, pb, pl] = metrics::CODE_HEADER_PADDING;
    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.muted)
        .pt(px(pt))
        .pr(px(pr))
        .pb(px(pb))
        .pl(px(pl))
        .text_color(colors.muted_foreground)
        .child(title)
        .child(actions);

    let pre = div()
        .px(px(metrics::CODE_PADDING_X))
        .py(px(metrics::CODE_PADDING_Y));
    let body = if wraps {
        pre.w_full().child(body).into_any_element()
    } else {
        div()
            .id(("md-code-scroll", id as u64))
            .w_full()
            .overflow_x_scroll()
            .child(pre.flex_none().child(body))
            .into_any_element()
    };
    let _ = window;
    div()
        .flex()
        .flex_col()
        .w_full()
        .overflow_hidden()
        .border_1()
        .border_color(colors.border)
        .rounded(px(metrics::CODE_BLOCK_RADIUS))
        .bg(colors.code_block_background)
        .child(header)
        .child(body)
        .into_any_element()
}

/// `tab-size: 4` from Tailwind's preflight.
fn expand_tabs(code: &str) -> String {
    if !code.contains('\t') {
        return code.to_string();
    }
    let mut out = String::with_capacity(code.len() + 16);
    let mut column = 0;
    for ch in code.chars() {
        match ch {
            '\t' => {
                let spaces = 4 - column % 4;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' => {
                out.push('\n');
                column = 0;
            }
            ch => {
                out.push(ch);
                column += 1;
            }
        }
    }
    out
}

/// The highlighted code as text runs. Spans index the original code, so tabs are expanded only
/// when there are none to keep the ranges aligned.
fn code_content(ctx: &Ctx, code: &str, highlighted: &Highlighted, wraps: bool) -> TextContent {
    let mono = ctx.style.mono_family.clone();
    let default = highlighted.default_style();
    let style_for = |style: t3_highlight::Style| {
        let mut run_font = font(mono.clone());
        if style.bold {
            run_font.weight = FontWeight::BOLD;
        }
        if style.italic {
            run_font.style = FontStyle::Italic;
        }
        RunStyle {
            font: run_font,
            size: metrics::CODE_SIZE,
            color: gpui_kit::rgba(style.color).into(),
            strikethrough: false,
            code_box: false,
            shift: Pixels::ZERO,
        }
    };
    let runs = if highlighted.len() == code.len() {
        highlighted
            .spans()
            .map(|(range, style)| (range, style_for(style)))
            .collect()
    } else {
        vec![(0..code.len(), style_for(default))]
    };
    let strut_font = font(mono.clone());
    TextContent {
        text: SharedString::from(code.to_string()),
        runs,
        links: Vec::new(),
        atoms: Vec::new(),
        // The `pre` is 14px with `leading-snug`; its strut sets the 19.25px line pitch even
        // though the code inside is 12px.
        strut_font,
        strut_size: metrics::BODY_SIZE,
        line_height: metrics::BODY_SIZE * metrics::SNUG,
        wrap: if wraps {
            TextWrap::PreWrap
        } else {
            TextWrap::Pre
        },
        align: TextAlign::Left,
        markup: Vec::new(),
        copy: CopyFormat {
            block_gap: true,
            ..CopyFormat::default()
        },
        decorations: Decorations {
            code_background: Hsla::transparent_black(),
            code_border: Hsla::transparent_black(),
            selection: ctx.style.colors.selection,
        },
    }
}

/// The code block title: fence file name with its icon, a language icon, or the language.
fn code_title(ctx: &Ctx, block: &CodeBlock, colors: &crate::style::MarkdownColors) -> Div {
    let title = div()
        .flex()
        .flex_row()
        .items_center()
        .min_w_0()
        .gap(px(metrics::CODE_TITLE_GAP))
        .font_family(ctx.style.mono_family.clone())
        .text_size(metrics::CODE_TITLE_SIZE)
        .line_height(metrics::CODE_TITLE_SIZE * metrics::SNUG);
    if let Some(name) = &block.title {
        return title
            .child(file_icon(name, colors.is_dark, px(14.)))
            .child(div().truncate().child(SharedString::from(name.clone())));
    }
    match language_icon_file(&block.language) {
        Some(file) => title.child(file_icon(&file, colors.is_dark, px(14.))),
        None => title.child(
            div()
                .truncate()
                .child(SharedString::from(block.language.clone())),
        ),
    }
}

/// `syntheticFileNameForLanguageId` + `hasSpecificPierreIconForFileName`: the synthetic file
/// name whose Pierre icon stands in for the language, when that icon is not the default.
fn language_icon_file(language: &str) -> Option<String> {
    let language = language.to_lowercase();
    let extension = match language.as_str() {
        "bash" | "shell" | "shellscript" => "sh",
        "csharp" => "cs",
        "javascript" => "js",
        "markdown" => "md",
        "plaintext" => "txt",
        "python" => "py",
        "ruby" => "rb",
        "rust" => "rs",
        "typescript" => "ts",
        "yaml" => "yml",
        other => other,
    };
    let file = format!("file.{extension}");
    let default = matches!(
        t3_ui::file_icon(&file, true),
        t3_ui::FileIcon::Mask { ref path, .. } if path.as_ref() == "icons/files/default.svg"
    );
    (!default).then_some(file)
}

/// The table: auto-layout columns, footer with expand and copy.
fn render_table(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    table: &Table,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let colors = ctx.style.colors.clone();
    let id = ctx.base + table.id;
    let expanded = this.table_expanded(id);
    let copied = this.copied.contains(&id);
    let cell_ctx = Ctx {
        size: metrics::TABLE_SIZE,
        line_height: metrics::TABLE_SIZE * metrics::RELAXED,
        ..ctx.clone()
    };
    let columns = table
        .header
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    let sizing = column_sizing(ctx, table, columns, expanded, window);
    let min_total: f32 = sizing.iter().map(|column| column.min).sum();

    let cell = |this: &mut Markdown,
                ids: &mut Ids,
                content: Option<&Inline>,
                column: usize,
                header: bool,
                window: &mut Window,
                cx: &mut Context<Markdown>| {
        let pad_y = if header {
            metrics::HEAD_PADDING_Y
        } else {
            metrics::CELL_PADDING_Y
        };
        let kind = TextKind::Cell {
            header,
            collapsed: !expanded,
        };
        let element =
            content.map(|content| inline(this, &cell_ctx, ids, content, kind, window, cx));
        let align = table.alignments.get(column).copied().unwrap_or(Align::None);
        let sizing = sizing[column];
        div()
            .flex()
            .flex_col()
            .flex_basis(px(sizing.max))
            .flex_grow(sizing.max)
            .flex_shrink(if sizing.max > 0. {
                (sizing.max - sizing.min) / sizing.max
            } else {
                0.
            })
            .min_w(px(sizing.min))
            .px(px(metrics::CELL_PADDING_X))
            .py(px(pad_y))
            .when(align == Align::Right, |this| this.items_end())
            .when(align == Align::Center, |this| this.items_center())
            .children(element.map(|element| {
                div()
                    .when(matches!(align, Align::Right | Align::Center), |this| {
                        this.flex_none().max_w_full()
                    })
                    .when(matches!(align, Align::None | Align::Left), |this| {
                        this.w_full()
                    })
                    .child(element)
            }))
    };

    let mut rows = Vec::new();
    let mut header = div()
        .flex()
        .flex_row()
        .border_b_1()
        .border_color(colors.table_rule);
    for column in 0..columns {
        header = header.child(cell(
            this,
            ids,
            table.header.get(column),
            column,
            true,
            window,
            cx,
        ));
    }
    rows.push(header.into_any_element());
    for row in &table.rows {
        let mut line = div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(colors.table_rule);
        for column in 0..columns {
            line = line.child(cell(this, ids, row.get(column), column, false, window, cx));
        }
        rows.push(line.into_any_element());
    }
    let entity = cx.entity().downgrade();
    let copy_entity = entity.clone();
    let markdown = table_markdown(table);
    div()
        .flex()
        .flex_col()
        .w_full()
        .child(
            div()
                .id(("md-table-scroll", id as u64))
                .w_full()
                .overflow_x_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w(px(min_total))
                        .children(rows),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .mt(px(2.))
                .child(
                    chrome_button(
                        ("md-table-expand", id as u64),
                        if expanded {
                            IconName::Minimize2
                        } else {
                            IconName::Maximize2
                        },
                        expanded,
                        &colors,
                    )
                    .on_click(move |_, _, cx| {
                        let _ = entity.update(cx, |this, cx| this.toggle_table(id, cx));
                    }),
                )
                .child(
                    chrome_button(
                        ("md-table-copy", id as u64),
                        if copied {
                            IconName::Check
                        } else {
                            IconName::Copy
                        },
                        false,
                        &colors,
                    )
                    .on_click(move |_, _, cx| {
                        let value = markdown.clone();
                        let _ = copy_entity.update(cx, |this, cx| this.copy(id, value, cx));
                    }),
                ),
        )
        .into_any_element()
}

/// A column's min-content and (capped) max-content widths, padding included.
#[derive(Clone, Copy, Default)]
struct ColumnSizing {
    min: f32,
    max: f32,
}

/// Chromium's auto table layout at `width: 100%`, as measured against the fork: each column
/// starts at its max-content width (`td` capped by `max-width: 24rem`), spare width is shared in
/// proportion to it, and when space is short columns shrink toward min-content. Expressed as
/// flex basis/grow/shrink per cell, every row resolves to the same column widths.
fn column_sizing(
    ctx: &Ctx,
    table: &Table,
    columns: usize,
    expanded: bool,
    window: &mut Window,
) -> Vec<ColumnSizing> {
    let mut header_font = font(ctx.style.sans_family.clone());
    header_font.weight = FontWeight::SEMIBOLD;
    let body_font = font(ctx.style.sans_family.clone());
    let pad = 2. * metrics::CELL_PADDING_X;
    let cap = metrics::CELL_MAX_WIDTH;
    let mut sizing = vec![ColumnSizing::default(); columns];
    let mut measure = |content: &Inline, column: usize, header: bool, window: &mut Window| {
        let font = if header { &header_font } else { &body_font };
        let text = content.text.replace(ATOM_CHAR, "  ");
        let full = f32::from(text_width(&text, font, metrics::TABLE_SIZE, window))
            + code_extra(content)
            + pad;
        // Collapsed cells never wrap; expanded `td`s may break anywhere; `th`s never wrap.
        let min = if header || !expanded {
            full
        } else {
            text.chars()
                .map(|ch| {
                    f32::from(text_width(
                        &ch.to_string(),
                        font,
                        metrics::TABLE_SIZE,
                        window,
                    ))
                })
                .fold(0f32, f32::max)
                + pad
        };
        // `max-width: 24rem` caps a cell's contribution (`th` only when collapsed).
        let capped = !header || !expanded;
        let (min, max) = if capped {
            (min.min(cap), full.min(cap))
        } else {
            (min, full)
        };
        let column = &mut sizing[column];
        column.max = column.max.max(max);
        column.min = column.min.max(min);
    };
    for (column, content) in table.header.iter().enumerate() {
        measure(content, column, true, window);
    }
    for row in &table.rows {
        for (column, content) in row.iter().enumerate().take(columns) {
            measure(content, column, false, window);
        }
    }
    sizing
}

/// Width taken by inline-code boxes in a cell (padding + border on both sides).
fn code_extra(content: &Inline) -> f32 {
    let spans = content.spans.iter().filter(|span| span.style.code).count() as f32;
    spans * 2. * (metrics::INLINE_CODE_PADDING_X + 1.)
}

/// `serializeTableElementToMarkdown`: a GFM table of the cells' text.
fn table_markdown(table: &Table) -> String {
    let cell = |content: &Inline| content.text.replace('|', "\\|").replace('\n', " ");
    let mut out = String::new();
    let header: Vec<String> = table.header.iter().map(cell).collect();
    out.push_str(&format!("| {} |\n", header.join(" | ")));
    let separators: Vec<&str> = table
        .alignments
        .iter()
        .map(|align| match align {
            Align::Left => ":---",
            Align::Center => ":---:",
            Align::Right => "---:",
            Align::None => "---",
        })
        .collect();
    out.push_str(&format!("| {} |\n", separators.join(" | ")));
    for row in &table.rows {
        let cells: Vec<String> = row.iter().map(cell).collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out
}

fn render_details(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    details: &Details,
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let colors = ctx.style.colors.clone();
    let id = ctx.base + details.id;
    let open = this.details_open.get(&id).copied().unwrap_or(details.open);
    let summary = match &details.summary {
        Some(summary) => {
            let summary_ctx = Ctx {
                color: colors.foreground,
                line_height: px(20.),
                ..ctx.clone()
            };
            inline(
                this,
                &summary_ctx,
                ids,
                summary,
                TextKind::Summary,
                window,
                cx,
            )
        }
        None => div().child("Details").into_any_element(),
    };
    let entity = cx.entity().downgrade();
    let initially_open = details.open;
    let trigger = div()
        .id(("md-details", id as u64))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .w_full()
        .py(px(8.))
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.foreground)
        .cursor_pointer()
        .child(icon(IconName::ChevronRight, px(16.), colors.muted_foreground).into_any_element())
        .child(summary)
        .on_click(move |_, _, cx| {
            let _ = entity.update(cx, |this, cx| this.toggle_details(id, initially_open, cx));
        });
    let mut container = div()
        .flex()
        .flex_col()
        .w_full()
        .border_t_1()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .child(trigger);
    if open {
        let panel_ctx = Ctx {
            color: colors.foreground.opacity(0.8),
            ..ctx.clone()
        };
        let blocks = render_blocks(this, &panel_ctx, ids, &details.blocks, window, cx);
        container = container.child(enclosing(
            div()
                .flex()
                .flex_col()
                .pb(px(12.))
                .pl(px(24.))
                .text_color(panel_ctx.color),
            blocks,
        ));
    }
    container.into_any_element()
}

fn render_footnotes(
    this: &mut Markdown,
    ctx: &Ctx,
    ids: &mut Ids,
    notes: &[Footnote],
    window: &mut Window,
    cx: &mut Context<Markdown>,
) -> AnyElement {
    let colors = ctx.style.colors.clone();
    let note_ctx = Ctx {
        color: colors.muted_foreground,
        size: px(12.),
        line_height: px(12. * metrics::RELAXED),
        ..ctx.clone()
    };
    let list = List {
        start: Some(1),
        items: notes
            .iter()
            .map(|note| {
                let mut blocks = note.blocks.clone();
                // The back reference closes the last paragraph.
                if let Some(Block::Paragraph { content, .. }) = blocks.last_mut() {
                    let offset = content.text.len() + 1;
                    content.text.push(' ');
                    content.text.push(ATOM_CHAR);
                    if let Some(last) = content.spans.last_mut() {
                        last.range.end = content.text.len();
                    } else {
                        content.spans.push(Span {
                            range: 0..content.text.len(),
                            style: InlineStyle::default(),
                        });
                    }
                    content.atoms.push(Atom {
                        offset,
                        kind: AtomKind::FootnoteBackref {
                            number: note.number,
                        },
                    });
                }
                crate::document::ListItem { task: None, blocks }
            })
            .collect(),
    };
    let mut laid = render_list(this, &note_ctx, ids, &list, window, cx);
    // `section[data-footnotes] ol { margin: 0 }`, `li + li { margin-top: 0.35rem }`.
    laid.own = (0., 0.);
    enclosing(
        div()
            .flex()
            .flex_col()
            .w_full()
            .border_t_1()
            .border_color(colors.border)
            .pt(px(12.))
            .text_size(px(12.))
            .line_height(note_ctx.line_height)
            .text_color(colors.muted_foreground),
        vec![laid],
    )
    .into_any_element()
}

fn icon(name: IconName, size: Pixels, color: Hsla) -> Icon {
    Icon::new(name).size(size).color(color)
}

/// The Pierre file-type icon the fork shows for `name`.
fn file_icon(name: &str, dark: bool, size: Pixels) -> AnyElement {
    t3_ui::file_icon(name, dark).render(size)
}

/// A ghost `icon-xs` button with the markdown chrome colors (`.chat-markdown-chrome-action`).
fn chrome_button(
    id: impl Into<gpui_kit::ElementId>,
    name: IconName,
    pressed: bool,
    colors: &crate::style::MarkdownColors,
) -> gpui_kit::Stateful<Div> {
    let rest = if pressed {
        colors.foreground
    } else {
        colors.muted_foreground
    };
    let hover = colors.foreground;
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_color(rest)
        .when(pressed, |this| this.bg(colors.chrome_pressed))
        .hover(|style| style.bg(colors.accent).text_color(hover))
        // Button svgs default to 80% opacity (`ui/button.tsx`).
        .child(div().opacity(0.8).child(icon(name, px(12.), rest)))
}
