//! The parsed form of a markdown chunk. Plain data, no GPUI: the renderer turns it into elements,
//! and the streaming cache keeps it for frozen chunks.

use std::ops::Range;

use crate::links::FileLink;

/// The placeholder character an [`Atom`] occupies in [`Inline::text`].
pub const ATOM_CHAR: char = '\u{FFFC}';

/// Top-level content of a chunk, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

/// A block, laid out vertically. `id` fields are the block's byte offset in its chunk, which the
/// renderer offsets by the chunk start so element state (wrap toggles, scroll) survives re-chunking.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// `<p>`. `tight` paragraphs are the bare text of a tight list item (no margins).
    Paragraph {
        content: Inline,
        tight: bool,
    },
    Heading {
        level: u8,
        content: Inline,
    },
    Quote(Vec<Block>),
    List(List),
    Code(CodeBlock),
    Table(Table),
    Rule,
    Details(Details),
    /// The footnote definitions section, in reference order.
    Footnotes(Vec<Footnote>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct List {
    /// `Some(start)` for an ordered list.
    pub start: Option<u64>,
    pub items: Vec<ListItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    /// `Some(checked)` for a GFM task list item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeBlock {
    pub id: usize,
    /// The language label (`extractFenceLanguage`): the fence's first word, `text` when absent.
    pub language: String,
    /// A file name from the fence meta (`title="x.ts"`, `file=`, or a bare `path/name.ext`).
    pub title: Option<String>,
    /// The code without its final newline (the mdast `value`).
    pub code: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub id: usize,
    pub alignments: Vec<Align>,
    pub header: Vec<Inline>,
    pub rows: Vec<Vec<Inline>>,
}

/// `<details>` from raw HTML.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    pub id: usize,
    pub summary: Option<Inline>,
    pub open: bool,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footnote {
    pub number: usize,
    pub blocks: Vec<Block>,
}

/// Inline content: text plus styling, links and atoms over byte ranges of `text`.
///
/// `spans` cover `text` exactly and in order; `links` are sorted and disjoint. Whitespace is
/// already collapsed the way HTML renders it, `\n` is a forced line break.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inline {
    pub text: String,
    pub spans: Vec<Span>,
    pub links: Vec<Link>,
    pub atoms: Vec<Atom>,
}

impl Inline {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: InlineStyle,
}

/// Character styling. Links are tracked separately in [`Inline::links`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    /// Inline code: 12px mono with the bordered box.
    pub code: bool,
    /// `<kbd>`/`<samp>`/`<tt>` from raw HTML: mono at the surrounding size, no box.
    pub mono: bool,
    pub superscript: bool,
    pub subscript: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub range: Range<usize>,
    /// The destination after the fork's `urlTransform`; empty when it was unsafe.
    pub href: String,
    /// End of the leading run that must not wrap: the favicon plus the protocol (or first
    /// character). Text after it may break anywhere (`<wbr>` after every character).
    /// `None` for links without a favicon, which wrap normally.
    pub nowrap_until: Option<usize>,
}

/// An inline object occupying one [`ATOM_CHAR`] in [`Inline::text`].
#[derive(Clone, Debug, PartialEq)]
pub struct Atom {
    pub offset: usize,
    pub kind: AtomKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AtomKind {
    /// A link to a file, rendered as a chip. `href` is the normalized destination (for copy).
    FileChip { link: FileLink, href: String },
    /// The favicon in front of an external link to `host`.
    Favicon { host: String },
    /// A footnote reference `[^label]`, shown as its number.
    FootnoteRef { number: usize },
    /// The `↩` link at the end of a footnote definition (added by the renderer).
    FootnoteBackref { number: usize },
    /// A task list item's read-only checkbox (added by the renderer).
    TaskCheckbox { checked: bool },
    /// An image; it breaks the paragraph like the fork's `display: block` images.
    Image { url: String, alt: String },
}
