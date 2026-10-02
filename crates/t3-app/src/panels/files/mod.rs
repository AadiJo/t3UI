//! Files surfaces (spec 4.2): the workspace file browser and the file preview.
//! STUB: replaced by the files implementation.

use gpui_kit::{Context, IntoElement, Render, Window, div};

use super::context::PanelContext;

/// The `files` tab: browse and search workspace files.
pub struct FilesSurface {
    _context: PanelContext,
}

impl FilesSurface {
    pub fn new(context: PanelContext, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { _context: context }
    }
}

impl Render for FilesSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A `file:{path}` tab: one file, read-only, scrolled to `reveal_line`.
pub struct FilePreview {
    _context: PanelContext,
    _path: String,
}

impl FilePreview {
    pub fn new(
        context: PanelContext,
        path: String,
        _reveal_line: Option<u32>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _context: context,
            _path: path,
        }
    }

    /// Scrolls to `line` again (a new `openFile` for an open path bumps `request`).
    pub fn reveal(&mut self, _line: Option<u32>, _request: u64, _cx: &mut Context<Self>) {}
}

impl Render for FilePreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
