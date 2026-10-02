//! Parses unified git patches the way the web client's `@pierre/diffs` `parsePatchFiles` does,
//! so the native viewer shows the same files, hunks and line numbers.
//!
//! Patches come from the server as text (`orchestration.getTurnDiff`, `review.getDiffPreview`).
//! They are always partial (no full file contents), may hold many files, and can be cut off
//! mid-hunk with a trailing `[truncated]` marker. The parser never fails: anything it does not
//! recognize is skipped, and [`renderable_patch`] falls back to showing the raw text when no file
//! could be found at all.

use std::cmp::Ordering;

/// How a file changed. Mirrors Pierre's `FileDiffMetadata.type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    /// Renamed without content changes (`similarity index 100%`).
    RenamedPure,
    /// Renamed with content changes.
    RenamedChanged,
}

impl ChangeKind {
    pub fn is_rename(self) -> bool {
        matches!(self, Self::RenamedPure | Self::RenamedChanged)
    }
}

/// One file of a patch: its paths, hunks, and the text of every line it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// Display path: the new path, or the old one for deletions, without `a/` / `b/`.
    pub path: String,
    /// The old path, set only for renames.
    pub previous_path: Option<String>,
    pub kind: ChangeKind,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    /// `Binary files … differ` or `GIT binary patch`. Such files have no hunks.
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    /// Old-side text (context and deleted lines) in patch order, without line endings.
    /// [`Block`] indices point into this.
    pub old_lines: Vec<String>,
    /// New-side text (context and added lines) in patch order, without line endings.
    pub new_lines: Vec<String>,
    /// Added lines actually present in the patch (truncated hunks count what arrived).
    pub additions: usize,
    /// Deleted lines actually present in the patch.
    pub deletions: usize,
}

impl FileDiff {
    /// The largest line number the file can show, used to size the line-number gutter
    /// (Pierre's `totalLines`).
    pub fn total_lines(&self) -> usize {
        let from_hunks = self.hunks.last().map_or(0, |hunk| {
            let new_end = hunk.new_start as usize + hunk.new_count as usize;
            let old_end = hunk.old_start as usize + hunk.old_count as usize;
            new_end.max(old_end)
        });
        from_hunks
            .max(self.new_lines.len())
            .max(self.old_lines.len())
    }
}

/// A hunk: the `@@ -a,b +c,d @@ context` header plus its lines grouped into blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// Text after the closing `@@`, usually the enclosing function signature.
    pub context: Option<String>,
    /// Unmodified lines hidden before this hunk, shown as "N unmodified lines".
    pub collapsed_before: u32,
    pub blocks: Vec<Block>,
    /// `\ No newline at end of file` followed the last old-side line.
    pub old_missing_newline: bool,
    /// `\ No newline at end of file` followed the last new-side line.
    pub new_missing_newline: bool,
}

/// A run of lines inside a hunk. Context runs pair old and new lines one to one; change runs
/// hold `deletions` old lines followed by `additions` new lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Context {
        /// First index into [`FileDiff::old_lines`] / [`FileDiff::new_lines`].
        old_index: usize,
        new_index: usize,
        /// Line numbers of the first line on each side.
        old_line: u32,
        new_line: u32,
        len: usize,
    },
    Change {
        old_index: usize,
        new_index: usize,
        old_line: u32,
        new_line: u32,
        deletions: usize,
        additions: usize,
    },
}

/// What the diff panel shows for a patch string (`getRenderablePatch` in the web client).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderablePatch {
    /// Empty or whitespace-only patch.
    Empty,
    /// Parsed files, sorted by path like the panel does.
    Files(Vec<FileDiff>),
    /// Nothing recognizable: show the trimmed text with a reason line above it.
    Raw { text: String, reason: &'static str },
}

/// Reason shown above a raw patch that contained no recognizable file.
pub const UNSUPPORTED_REASON: &str = "Unsupported diff format. Showing raw patch.";

/// Parses `patch` for display: trims it, parses every file and sorts them by path with the
/// panel's natural, case-insensitive order.
pub fn renderable_patch(patch: &str) -> RenderablePatch {
    let trimmed = patch.trim();
    if trimmed.is_empty() {
        return RenderablePatch::Empty;
    }
    let mut files = parse_patch(trimmed);
    if files.is_empty() {
        return RenderablePatch::Raw {
            text: trimmed.to_owned(),
            reason: UNSUPPORTED_REASON,
        };
    }
    files.sort_by(|a, b| compare_natural(&a.path, &b.path));
    RenderablePatch::Files(files)
}

/// Parses every file of a git (or plain unified) patch, in patch order.
pub fn parse_patch(patch: &str) -> Vec<FileDiff> {
    let lines: Vec<&str> = patch.split('\n').collect();
    let is_git = lines.iter().any(|line| line.starts_with("diff --git "));
    let mut parser = Parser::default();
    let mut ix = 0;
    while ix < lines.len() {
        let line = lines[ix];
        ix += 1;
        // File and hunk boundaries always win, even inside an incomplete (truncated) hunk.
        if is_git && line.starts_with("diff --git ") {
            parser.start_file(FileBuilder::from_git_line(line));
            continue;
        }
        if line.starts_with("@@ ") {
            parser.start_hunk(line);
            continue;
        }
        if parser.in_hunk_body(line) {
            parser.push_hunk_line(line);
            continue;
        }
        if !is_git
            && line.starts_with("--- ")
            && lines.get(ix).is_some_and(|next| next.starts_with("+++ "))
        {
            let mut file = FileBuilder::default();
            file.unified_header(trim_cr(line));
            file.unified_header(trim_cr(lines[ix]));
            ix += 1;
            parser.start_file(file);
            continue;
        }
        parser.header_line(trim_cr(line), is_git);
    }
    parser.finish()
}

#[derive(Default)]
struct Parser {
    files: Vec<FileDiff>,
    file: Option<FileBuilder>,
}

impl Parser {
    fn start_file(&mut self, file: FileBuilder) {
        self.finish_file();
        self.file = Some(file);
    }

    fn finish_file(&mut self) {
        if let Some(file) = self.file.take() {
            self.files.push(file.finish());
        }
    }

    fn finish(mut self) -> Vec<FileDiff> {
        self.finish_file();
        self.files
    }

    fn start_hunk(&mut self, line: &str) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let Some(header) = HunkHeader::parse(trim_cr(line)) else {
            // Pierre drops a malformed hunk and its lines; so does the native parser.
            file.close_hunk();
            return;
        };
        file.begin_hunk(header);
    }

    fn in_hunk_body(&self, line: &str) -> bool {
        self.file
            .as_ref()
            .and_then(|file| file.hunk.as_ref())
            .is_some_and(|hunk| !hunk.complete() || line.starts_with('\\'))
    }

    fn push_hunk_line(&mut self, line: &str) {
        if let Some(file) = self.file.as_mut() {
            file.push_hunk_line(line);
        }
    }

    fn header_line(&mut self, line: &str, is_git: bool) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        // Header lines only count before the first hunk; later junk (a `-- ` signature,
        // the server's `[truncated]` marker) is ignored.
        if file.hunks.is_empty() && file.hunk.is_none() {
            if is_git {
                file.git_header(line);
            } else {
                file.unified_header(line);
            }
        }
    }
}

/// Parsed `@@ -a,b +c,d @@ context` header.
struct HunkHeader {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
    context: Option<String>,
}

impl HunkHeader {
    /// Parses a hunk header. Omitted counts default to 1, as in `diff`.
    fn parse(line: &str) -> Option<Self> {
        let rest = line.strip_prefix("@@ -")?;
        let (old_start, old_count, rest) = parse_range(rest)?;
        let rest = rest.strip_prefix(" +")?;
        let (new_start, new_count, rest) = parse_range(rest)?;
        let rest = rest.strip_prefix(" @@")?;
        let context = rest
            .strip_prefix(' ')
            .map(|text| text.to_owned())
            .filter(|text| !text.is_empty());
        Some(Self {
            old_start,
            old_count,
            new_start,
            new_count,
            context,
        })
    }
}

/// Parses `start[,count]` at the front of `text`, returning the rest.
fn parse_range(text: &str) -> Option<(u32, u32, &str)> {
    let (start, rest) = parse_number(text)?;
    match rest.strip_prefix(',') {
        Some(after_comma) => {
            let (count, rest) = parse_number(after_comma)?;
            Some((start, count, rest))
        }
        None => Some((start, 1, rest)),
    }
}

fn parse_number(text: &str) -> Option<(u32, &str)> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let value = text[..digits].parse().ok()?;
    Some((value, &text[digits..]))
}

/// Line kind of the previous hunk line, to attribute a `\ No newline` marker.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LastLine {
    None,
    Context,
    Deletion,
    Addition,
}

struct HunkBuilder {
    hunk: Hunk,
    old_seen: u32,
    new_seen: u32,
    old_line: u32,
    new_line: u32,
    last: LastLine,
}

impl HunkBuilder {
    /// A hunk is complete once both sides reached their header counts.
    fn complete(&self) -> bool {
        self.old_seen >= self.hunk.old_count && self.new_seen >= self.hunk.new_count
    }
}

#[derive(Default)]
struct FileBuilder {
    name: Option<String>,
    previous_name: Option<String>,
    kind: Option<ChangeKind>,
    old_mode: Option<String>,
    new_mode: Option<String>,
    binary: bool,
    old_is_null: bool,
    new_is_null: bool,
    hunks: Vec<Hunk>,
    hunk: Option<HunkBuilder>,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    additions: usize,
    deletions: usize,
}

impl FileBuilder {
    /// Starts a file from `diff --git a/OLD b/NEW` (paths may be C-quoted).
    fn from_git_line(line: &str) -> Self {
        let mut file = Self::default();
        if let Some((old, new)) = parse_git_names(trim_cr(line)) {
            if old != new {
                file.previous_name = Some(old);
            }
            file.name = Some(new);
        }
        file
    }

    fn git_header(&mut self, line: &str) {
        if let Some(path) = line.strip_prefix("--- ") {
            match parse_header_path(path) {
                HeaderPath::Null => self.old_is_null = true,
                HeaderPath::Prefixed(path) => {
                    self.previous_name = Some(path.clone());
                    self.name = Some(path);
                }
                HeaderPath::Other => {}
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            match parse_header_path(path) {
                HeaderPath::Null => self.new_is_null = true,
                HeaderPath::Prefixed(path) => self.name = Some(path),
                HeaderPath::Other => {}
            }
        } else if let Some(mode) = line.strip_prefix("new file mode ") {
            self.kind = Some(ChangeKind::Added);
            self.new_mode = Some(mode.trim().to_owned());
        } else if let Some(mode) = line.strip_prefix("deleted file mode ") {
            self.kind = Some(ChangeKind::Deleted);
            self.old_mode = Some(mode.trim().to_owned());
        } else if let Some(mode) = line.strip_prefix("old mode ") {
            self.old_mode = Some(mode.trim().to_owned());
        } else if let Some(mode) = line.strip_prefix("new mode ") {
            self.new_mode = Some(mode.trim().to_owned());
        } else if let Some(similarity) = line.strip_prefix("similarity index ") {
            self.kind = Some(if similarity.trim() == "100%" {
                ChangeKind::RenamedPure
            } else {
                ChangeKind::RenamedChanged
            });
        } else if let Some(path) = line.strip_prefix("rename from ") {
            self.previous_name = Some(unquote(path.trim()));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            self.name = Some(unquote(path.trim()));
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.binary = true;
        }
    }

    /// `--- old` / `+++ new` headers of a plain (non-git) unified diff.
    fn unified_header(&mut self, line: &str) {
        let (is_old, raw) = if let Some(path) = line.strip_prefix("--- ") {
            (true, path)
        } else if let Some(path) = line.strip_prefix("+++ ") {
            (false, path)
        } else {
            return;
        };
        // Drop a trailing tab-separated timestamp, as `diff -u` writes one.
        let raw = raw.split('\t').next().unwrap_or_default().trim();
        if raw == "/dev/null" {
            if is_old {
                self.old_is_null = true;
            } else {
                self.new_is_null = true;
            }
            return;
        }
        let path = strip_ab_prefix(&unquote(raw)).to_owned();
        if is_old {
            self.previous_name = Some(path.clone());
        }
        self.name = Some(path);
    }

    fn begin_hunk(&mut self, header: HunkHeader) {
        self.close_hunk();
        let last_end = self.hunks.last().map_or(0, |hunk| {
            hunk.new_start
                .saturating_add(hunk.new_count)
                .saturating_sub(1)
        });
        let collapsed_before = header.new_start.saturating_sub(1).saturating_sub(last_end);
        self.hunk = Some(HunkBuilder {
            old_line: header.old_start,
            new_line: header.new_start,
            hunk: Hunk {
                old_start: header.old_start,
                old_count: header.old_count,
                new_start: header.new_start,
                new_count: header.new_count,
                context: header.context,
                collapsed_before,
                blocks: Vec::new(),
                old_missing_newline: false,
                new_missing_newline: false,
            },
            old_seen: 0,
            new_seen: 0,
            last: LastLine::None,
        });
    }

    fn close_hunk(&mut self) {
        if let Some(builder) = self.hunk.take() {
            self.hunks.push(builder.hunk);
        }
    }

    fn push_hunk_line(&mut self, line: &str) {
        let Some(hunk) = self.hunk.as_mut() else {
            return;
        };
        let Some(&marker) = line.as_bytes().first() else {
            // Blank lines are not valid hunk lines (git writes " " for an empty context
            // line). Pierre skips them, which also drops the blank lines in front of the
            // server's `[truncated]` marker.
            return;
        };
        let text = || trim_cr(&line[1..]).to_owned();
        match marker {
            b' ' => {
                let old_index = self.old_lines.len();
                let new_index = self.new_lines.len();
                match hunk.hunk.blocks.last_mut() {
                    Some(Block::Context { len, .. }) if hunk.last == LastLine::Context => {
                        *len += 1;
                    }
                    _ => hunk.hunk.blocks.push(Block::Context {
                        old_index,
                        new_index,
                        old_line: hunk.old_line,
                        new_line: hunk.new_line,
                        len: 1,
                    }),
                }
                let text = text();
                self.old_lines.push(text.clone());
                self.new_lines.push(text);
                hunk.old_line += 1;
                hunk.new_line += 1;
                hunk.old_seen += 1;
                hunk.new_seen += 1;
                hunk.last = LastLine::Context;
            }
            b'-' | b'+' => {
                let is_addition = marker == b'+';
                if !matches!(hunk.last, LastLine::Deletion | LastLine::Addition) {
                    hunk.hunk.blocks.push(Block::Change {
                        old_index: self.old_lines.len(),
                        new_index: self.new_lines.len(),
                        old_line: hunk.old_line,
                        new_line: hunk.new_line,
                        deletions: 0,
                        additions: 0,
                    });
                }
                let Some(Block::Change {
                    deletions,
                    additions,
                    ..
                }) = hunk.hunk.blocks.last_mut()
                else {
                    return;
                };
                if is_addition {
                    *additions += 1;
                    self.new_lines.push(text());
                    self.additions += 1;
                    hunk.new_line += 1;
                    hunk.new_seen += 1;
                    hunk.last = LastLine::Addition;
                } else {
                    *deletions += 1;
                    self.old_lines.push(text());
                    self.deletions += 1;
                    hunk.old_line += 1;
                    hunk.old_seen += 1;
                    hunk.last = LastLine::Deletion;
                }
            }
            b'\\' => match hunk.last {
                LastLine::Context => {
                    hunk.hunk.old_missing_newline = true;
                    hunk.hunk.new_missing_newline = true;
                }
                LastLine::Deletion => hunk.hunk.old_missing_newline = true,
                LastLine::Addition => hunk.hunk.new_missing_newline = true,
                LastLine::None => {}
            },
            // Anything else (e.g. the `[truncated]` marker) is not part of the hunk.
            _ => {}
        }
    }

    fn finish(mut self) -> FileDiff {
        self.close_hunk();
        let kind = match self.kind {
            Some(kind) if kind.is_rename() => {
                // A rename that git did not mark with a similarity index line still counts
                // as changed when hunks follow.
                if kind == ChangeKind::RenamedPure && !self.hunks.is_empty() {
                    ChangeKind::RenamedChanged
                } else {
                    kind
                }
            }
            Some(kind) => kind,
            None if self.old_is_null && !self.new_is_null => ChangeKind::Added,
            None if self.new_is_null && !self.old_is_null => ChangeKind::Deleted,
            None => match (&self.previous_name, &self.name) {
                (Some(old), Some(new)) if old != new => {
                    if self.hunks.is_empty() {
                        ChangeKind::RenamedPure
                    } else {
                        ChangeKind::RenamedChanged
                    }
                }
                _ => ChangeKind::Modified,
            },
        };
        let previous_path = if kind.is_rename() {
            self.previous_name.clone()
        } else {
            None
        };
        let path = self
            .name
            .or(self.previous_name)
            .map(|name| strip_ab_prefix(&name).to_owned())
            .unwrap_or_default();
        FileDiff {
            path,
            previous_path,
            kind,
            old_mode: self.old_mode,
            new_mode: self.new_mode,
            binary: self.binary,
            hunks: self.hunks,
            old_lines: self.old_lines,
            new_lines: self.new_lines,
            additions: self.additions,
            deletions: self.deletions,
        }
    }
}

enum HeaderPath {
    Null,
    /// A path with its `a/` or `b/` prefix removed.
    Prefixed(String),
    /// Anything else, which git headers never contain.
    Other,
}

/// Parses the path of a git `---`/`+++` header line.
fn parse_header_path(raw: &str) -> HeaderPath {
    let raw = raw.trim_end_matches(['\t', ' ']);
    if raw == "/dev/null" {
        return HeaderPath::Null;
    }
    let path = unquote(raw);
    match path.strip_prefix("a/").or_else(|| path.strip_prefix("b/")) {
        Some(stripped) => HeaderPath::Prefixed(stripped.to_owned()),
        None => HeaderPath::Other,
    }
}

/// Splits `diff --git a/OLD b/NEW` into its two paths.
///
/// Quoted paths are unescaped. For unquoted paths containing spaces the split is ambiguous; an
/// exact `a/X b/X` split wins (the common no-rename case), otherwise the old path ends at the
/// first ` b/`, like Pierre's lazy regex.
fn parse_git_names(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    let (old, rest) = if rest.starts_with('"') {
        let (path, rest) = take_quoted(rest)?;
        (path.strip_prefix("a/")?.to_owned(), rest.strip_prefix(' ')?)
    } else {
        let body = rest.strip_prefix("a/")?;
        let split = split_unquoted_git_names(body)?;
        (body[..split].to_owned(), &body[split + 1..])
    };
    let new = if rest.starts_with('"') {
        let (path, tail) = take_quoted(rest)?;
        if !tail.trim().is_empty() {
            return None;
        }
        path.strip_prefix("b/")?.to_owned()
    } else {
        rest.strip_prefix("b/")?.to_owned()
    };
    Some((old, new))
}

/// Byte index of the space that separates `OLD` from ` b/NEW` in `OLD b/NEW`.
fn split_unquoted_git_names(body: &str) -> Option<usize> {
    if let Some(half) = body
        .len()
        .checked_sub(3)
        .filter(|len| len % 2 == 0)
        .map(|len| len / 2)
        && body.get(half..half + 3) == Some(" b/")
        && body
            .get(..half)
            .is_some_and(|old| body.get(half + 3..) == Some(old))
    {
        return Some(half);
    }
    body.match_indices(" b/")
        .map(|(ix, _)| ix)
        .find(|&ix| ix > 0)
        .or_else(|| body.find(" \"b/"))
}

/// Reads a C-quoted string (`"…"`) at the front of `text` and returns it unescaped.
fn take_quoted(text: &str) -> Option<(String, &str)> {
    let inner = text.strip_prefix('"')?;
    let mut end = None;
    let mut escaped = false;
    for (ix, byte) in inner.bytes().enumerate() {
        match byte {
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            b'"' => {
                end = Some(ix);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    Some((unescape_c(&inner[..end]), &inner[end + 1..]))
}

/// Unquotes a path if git C-quoted it (it does for spaces at ends, quotes, and non-ASCII
/// bytes, which it writes as octal escapes).
fn unquote(raw: &str) -> String {
    match take_quoted(raw) {
        Some((path, rest)) if rest.trim().is_empty() => path,
        _ => raw.to_owned(),
    }
}

fn unescape_c(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        let byte = bytes[ix];
        ix += 1;
        if byte != b'\\' || ix >= bytes.len() {
            out.push(byte);
            continue;
        }
        let escape = bytes[ix];
        ix += 1;
        match escape {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'v' => out.push(0x0b),
            b'0'..=b'7' => {
                let mut value = u32::from(escape - b'0');
                for _ in 0..2 {
                    match bytes.get(ix) {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            ix += 1;
                        }
                        _ => break,
                    }
                }
                out.push(value as u8);
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn strip_ab_prefix(path: &str) -> &str {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
}

fn trim_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

/// Orders strings like `localeCompare(a, b, undefined, { numeric: true, sensitivity: "base" })`:
/// case-insensitive, digit runs compared by value, punctuation before digits before letters.
pub fn compare_natural(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                let l_digits = take_digits(&mut left);
                let r_digits = take_digits(&mut right);
                let l_trim = l_digits.trim_start_matches('0');
                let r_trim = r_digits.trim_start_matches('0');
                let ordering = l_trim
                    .len()
                    .cmp(&r_trim.len())
                    .then_with(|| l_trim.cmp(r_trim));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(l), Some(r)) => {
                let ordering = collation_key(l).cmp(&collation_key(r));
                if ordering != Ordering::Equal {
                    return ordering;
                }
                left.next();
                right.next();
            }
        }
    }
}

fn take_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(&ch) = chars.peek() {
        if !ch.is_ascii_digit() {
            break;
        }
        digits.push(ch);
        chars.next();
    }
    digits
}

/// Common punctuation in ICU root-collation order. Anything else sorts after these, by code point.
const PUNCTUATION_ORDER: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// Primary collation weight: (class, rank, lowercase char). Whitespace sorts first, then
/// punctuation, digits and letters, as in ICU's root collation.
fn collation_key(ch: char) -> (u8, usize, char) {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    if ch.is_whitespace() {
        (0, 0, ch)
    } else if ch.is_numeric() {
        (2, 0, ch)
    } else if ch.is_alphanumeric() {
        (3, 0, lower)
    } else {
        let rank = PUNCTUATION_ORDER
            .chars()
            .position(|candidate| candidate == ch)
            .unwrap_or(usize::MAX);
        (1, rank, ch)
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes the parser must handle (each has a test below):
    //!  1. Empty or whitespace-only input renders as `Empty`, not a raw block.
    //!  2. Text without any file header falls back to `Raw` with the unsupported reason.
    //!  3. Several files in one git patch; text before the first `diff --git` is ignored.
    //!  4. `a/` / `b/` prefixes stripped; quoted and octal-escaped (non-ASCII) paths unescaped;
    //!     unquoted paths with spaces split correctly.
    //!  5. New files (`new file mode`, `--- /dev/null`) and deleted files (`+++ /dev/null`,
    //!     path taken from the old side).
    //!  6. Pure renames (no hunks) and renames with changes; `previous_path` only on renames.
    //!  7. Mode-only changes produce a file with modes and no hunks.
    //!  8. Binary files (`Binary files … differ`, `GIT binary patch`) produce no hunks.
    //!  9. Hunk headers: omitted counts default to 1, `-0,0` for new files, malformed headers
    //!     skipped without losing the file, trailing function context captured.
    //! 10. Line numbers: context advances both sides, deletions old only, additions new only.
    //! 11. `\ No newline at end of file` flags the side of the line before it, both for context,
    //!     and never counts as a line.
    //! 12. Truncated patches (cut mid-hunk, `[truncated]` marker) keep the lines that arrived,
    //!     count only those, and ignore the marker and blank lines.
    //! 13. Lines after a hunk's counts are exhausted (e.g. a `-- ` signature) are ignored and
    //!     a deletion line that looks like `--- x` inside a hunk stays a deletion.
    //! 14. Collapsed lines between hunks ("N unmodified lines") follow Pierre's formula.
    //! 15. CRLF patches: `\r` stripped from headers and line text.
    //! 16. Plain unified diffs without `diff --git` still parse.
    //! 17. Files sort by path naturally and case-insensitively.
    //! 18. Unicode content never splits a character.
    use super::*;

    const MULTI: &str = include_str!("../fixtures/multi-file.patch");
    const RENAME_BINARY: &str = include_str!("../fixtures/rename-binary.patch");
    const TRUNCATED: &str = include_str!("../fixtures/truncated.patch");

    fn files(patch: &str) -> Vec<FileDiff> {
        match renderable_patch(patch) {
            RenderablePatch::Files(files) => files,
            other => panic!("expected files, got {other:?}"),
        }
    }

    fn file<'a>(files: &'a [FileDiff], path: &str) -> &'a FileDiff {
        files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("no file {path}"))
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(renderable_patch(""), RenderablePatch::Empty);
        assert_eq!(renderable_patch(" \n\t\n"), RenderablePatch::Empty);
    }

    #[test]
    fn unrecognized_text_is_raw() {
        assert_eq!(
            renderable_patch("  just some text\nnot a diff  "),
            RenderablePatch::Raw {
                text: "just some text\nnot a diff".into(),
                reason: UNSUPPORTED_REASON,
            }
        );
    }

    #[test]
    fn multi_file_patch_parses_every_file_sorted() {
        let files = files(&format!("commit abc\nAuthor: someone\n\n{MULTI}"));
        let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "apps/server/src/legacy/checkpointPaths.ts",
                "apps/web/src/components/ChatView.tsx",
                "apps/web/src/lib/diffStats.ts",
                "README.md",
                "scripts/release.sh",
            ]
        );
    }

    #[test]
    fn paths_are_unprefixed_and_unquoted() {
        let patch = "diff --git \"a/docs/caf\\303\\251 notes.md\" \"b/docs/caf\\303\\251 notes.md\"\n\
                     index 1..2 100644\n\
                     --- \"a/docs/caf\\303\\251 notes.md\"\n\
                     +++ \"b/docs/caf\\303\\251 notes.md\"\n\
                     @@ -1 +1 @@\n-a\n+b\n\
                     diff --git a/my file.txt b/my file.txt\n\
                     index 1..2 100644\n\
                     @@ -1 +1 @@\n-a\n+b\n";
        let files = parse_patch(patch);
        assert_eq!(files[0].path, "docs/café notes.md");
        assert_eq!(files[0].previous_path, None);
        assert_eq!(files[1].path, "my file.txt");
        assert_eq!(files[1].kind, ChangeKind::Modified);
    }

    #[test]
    fn new_and_deleted_files() {
        let files = files(MULTI);
        let added = file(&files, "apps/web/src/lib/diffStats.ts");
        assert_eq!(added.kind, ChangeKind::Added);
        assert_eq!(added.new_mode.as_deref(), Some("100644"));
        assert_eq!((added.additions, added.deletions), (14, 0));
        assert_eq!(added.new_lines.len(), 14);
        assert!(added.old_lines.is_empty());

        let deleted = file(&files, "apps/server/src/legacy/checkpointPaths.ts");
        assert_eq!(deleted.kind, ChangeKind::Deleted);
        assert_eq!((deleted.additions, deleted.deletions), (0, 7));
        assert_eq!(deleted.previous_path, None);
    }

    #[test]
    fn renames_keep_previous_path() {
        let files = files(RENAME_BINARY);
        let pure = file(&files, "apps/web/src/components/diff/DiffPanel.tsx");
        assert_eq!(pure.kind, ChangeKind::RenamedPure);
        assert_eq!(
            pure.previous_path.as_deref(),
            Some("apps/web/src/components/DiffPanel.tsx")
        );
        assert!(pure.hunks.is_empty());

        let changed = file(&files, "apps/web/src/components/diff/rendering.ts");
        assert_eq!(changed.kind, ChangeKind::RenamedChanged);
        assert_eq!(
            changed.previous_path.as_deref(),
            Some("apps/web/src/lib/diffRendering.ts")
        );
        assert_eq!((changed.additions, changed.deletions), (1, 1));
    }

    #[test]
    fn mode_only_change_has_no_hunks() {
        let files = files(MULTI);
        let script = file(&files, "scripts/release.sh");
        assert_eq!(script.kind, ChangeKind::Modified);
        assert_eq!(script.old_mode.as_deref(), Some("100644"));
        assert_eq!(script.new_mode.as_deref(), Some("100755"));
        assert!(script.hunks.is_empty());
    }

    #[test]
    fn binary_files_have_no_hunks() {
        let files = files(RENAME_BINARY);
        let icon = file(&files, "apps/web/public/icon.png");
        assert!(icon.binary);
        assert_eq!(icon.kind, ChangeKind::Modified);
        assert!(icon.hunks.is_empty());
        let card = file(&files, "apps/web/public/social-card.png");
        assert!(card.binary);
        assert_eq!(card.kind, ChangeKind::Added);

        let git_binary = parse_patch(
            "diff --git a/x.bin b/x.bin\nindex 1..2 100644\nGIT binary patch\nliteral 3\nKcmZ?\n\nliteral 0\nHcmV?d00001\n",
        );
        assert!(git_binary[0].binary);
        assert!(git_binary[0].hunks.is_empty());
    }

    #[test]
    fn hunk_headers() {
        let files = files(MULTI);
        let chat = file(&files, "apps/web/src/components/ChatView.tsx");
        let hunk = &chat.hunks[0];
        assert_eq!(
            (
                hunk.old_start,
                hunk.old_count,
                hunk.new_start,
                hunk.new_count
            ),
            (12, 7, 12, 8)
        );
        assert_eq!(
            hunk.context.as_deref(),
            Some("import { useStore } from \"../store\";")
        );
        let added = file(&files, "apps/web/src/lib/diffStats.ts");
        assert_eq!((added.hunks[0].old_start, added.hunks[0].old_count), (0, 0));

        let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -3 +3 @@\n-x\n+y\n@@ broken @@\n z\n@@ -9,2 +9,2 @@\n a\n-b\n+c\n";
        let parsed = parse_patch(patch);
        assert_eq!(parsed.len(), 1);
        let hunks = &parsed[0].hunks;
        assert_eq!(hunks.len(), 2);
        assert_eq!((hunks[0].old_count, hunks[0].new_count), (1, 1));
        assert_eq!(hunks[0].context, None);
        assert_eq!(hunks[1].old_start, 9);
    }

    #[test]
    fn line_numbers_follow_sides() {
        let files = files(MULTI);
        let chat = file(&files, "apps/web/src/components/ChatView.tsx");
        assert_eq!(
            chat.hunks[1].blocks,
            vec![
                Block::Context {
                    old_index: 7,
                    new_index: 8,
                    old_line: 140,
                    new_line: 141,
                    len: 3
                },
                Block::Change {
                    old_index: 10,
                    new_index: 11,
                    old_line: 143,
                    new_line: 144,
                    deletions: 3,
                    additions: 1
                },
                Block::Context {
                    old_index: 13,
                    new_index: 12,
                    old_line: 146,
                    new_line: 145,
                    len: 3
                },
            ]
        );
        assert_eq!(
            chat.old_lines[10],
            "  const isDiffOpen = panel?.activeSurfaceId === \"diff\";"
        );
        assert_eq!(
            chat.new_lines[11],
            "  const activeSurface = panel?.isOpen ? panel.activeSurfaceId : null;"
        );
        assert_eq!((chat.additions, chat.deletions), (8, 4));
    }

    #[test]
    fn missing_newline_markers() {
        let files = files(MULTI);
        let added = file(&files, "apps/web/src/lib/diffStats.ts");
        assert!(added.hunks[0].new_missing_newline);
        assert!(!added.hunks[0].old_missing_newline);
        assert_eq!(added.new_lines.len(), 14);

        let context = parse_patch(
            "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-a\n+b\n c\n\\ No newline at end of file\n",
        );
        assert!(context[0].hunks[0].old_missing_newline);
        assert!(context[0].hunks[0].new_missing_newline);

        let deletion =
            parse_patch("--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+a\n");
        let hunk = &deletion[0].hunks[0];
        assert!(hunk.old_missing_newline);
        assert!(!hunk.new_missing_newline);
        assert_eq!((deletion[0].additions, deletion[0].deletions), (1, 1));
    }

    #[test]
    fn truncated_patch_keeps_what_arrived() {
        let files = files(TRUNCATED);
        assert_eq!(files.len(), 1);
        let server = &files[0];
        assert_eq!(server.hunks.len(), 1);
        assert_eq!((server.additions, server.deletions), (2, 1));
        assert_eq!(server.new_lines.len(), 3);
        assert!(
            server
                .new_lines
                .iter()
                .all(|line| !line.contains("truncated"))
        );
        // Cut in the middle of a line, mid-character-free.
        let cut =
            parse_patch("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n-old\n+new li");
        assert_eq!(cut[0].new_lines, ["new li"]);
    }

    #[test]
    fn junk_after_hunk_is_ignored() {
        let patch = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n--- not a header\n+++ also content\n context\n-- \n2.44.0\n";
        let parsed = parse_patch(patch);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].old_lines, ["-- not a header", "context"]);
        assert_eq!(parsed[0].new_lines, ["++ also content", "context"]);
    }

    #[test]
    fn collapsed_lines_between_hunks() {
        let files = files(MULTI);
        let chat = file(&files, "apps/web/src/components/ChatView.tsx");
        let collapsed: Vec<_> = chat
            .hunks
            .iter()
            .map(|hunk| hunk.collapsed_before)
            .collect();
        // 11 lines before line 12; 141 - 1 - (12 + 8 - 1); 197 - 1 - (141 + 7 - 1).
        assert_eq!(collapsed, [11, 121, 49]);
        let readme = file(&files, "README.md");
        assert_eq!(readme.hunks[0].collapsed_before, 0);
        assert_eq!(chat.total_lines(), 207);
    }

    #[test]
    fn crlf_patches() {
        let patch = MULTI.replace('\n', "\r\n");
        let files = files(&patch);
        assert_eq!(files.len(), 5);
        let chat = file(&files, "apps/web/src/components/ChatView.tsx");
        assert!(chat.old_lines.iter().all(|line| !line.ends_with('\r')));
        assert_eq!(
            chat.hunks[0].context.as_deref(),
            Some("import { useStore } from \"../store\";")
        );
    }

    #[test]
    fn plain_unified_diff() {
        let patch = "--- a/src/lib.rs\t2024-01-01 00:00:00\n+++ b/src/lib.rs\t2024-01-02 00:00:00\n@@ -1,2 +1,2 @@\n-fn a() {}\n+fn b() {}\n x\n--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1 @@\n+fn c() {}\n";
        let parsed = parse_patch(patch);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, "src/lib.rs");
        assert_eq!(parsed[0].kind, ChangeKind::Modified);
        assert_eq!(parsed[1].path, "new.rs");
        assert_eq!(parsed[1].kind, ChangeKind::Added);
    }

    #[test]
    fn natural_path_order() {
        let mut paths = vec![
            "b10.ts",
            "B2.ts",
            "a/z.ts",
            "a.ts",
            "_x.ts",
            "README.md",
            "a/b.ts",
        ];
        paths.sort_by(|a, b| compare_natural(a, b));
        assert_eq!(
            paths,
            [
                "_x.ts",
                "a.ts",
                "a/b.ts",
                "a/z.ts",
                "B2.ts",
                "b10.ts",
                "README.md"
            ]
        );
    }

    #[test]
    fn unicode_content() {
        let patch = "diff --git a/é.txt b/é.txt\n--- a/é.txt\n+++ b/é.txt\n@@ -1 +1 @@\n-héllo wörld\n+日本語のテキスト\n";
        let parsed = parse_patch(patch);
        assert_eq!(parsed[0].path, "é.txt");
        assert_eq!(parsed[0].old_lines, ["héllo wörld"]);
        assert_eq!(parsed[0].new_lines, ["日本語のテキスト"]);
    }
}
