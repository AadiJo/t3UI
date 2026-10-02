//! Shared chrome for full-page workspace views (Usage, Pull Requests, settings, the empty
//! states). Mirrors `WorkspacePageHeader.tsx`, `WorkspacePageContainer.tsx` and
//! `WorkspaceBreadcrumb.tsx` in the fork; see `docs/spec/pages.md` section 1.
//!
//! Each piece is a builder returning a `Div` so callers add children and override style the
//! same way the web passes `className`:
//!
//! ```ignore
//! div().size_full().flex().flex_col()
//!     .child(
//!         page_header("usage-header", window, cx).child(
//!             breadcrumb()
//!                 .child(breadcrumb_item(true, cx).child("Usage")),
//!         ),
//!     )
//!     .child(
//!         div().relative().flex_1().min_h_0()
//!             .child(ScrollArea::new("usage-scroll").child(page_container(PageWidth::Wide)))
//!             .child(topbar_scroll_fade(cx)),
//!     )
//! ```

use gpui_kit::{
    App, Div, ElementId, FontWeight, ParentElement as _, Stateful, Styled as _, Window, div,
    linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px,
};
use t3_ui::{
    ActiveColors as _,
    tokens::{layout, text},
};

use crate::{
    chrome::{TypeScale as _, drag_region},
    workspace::collapsed_titlebar_inset,
};

/// Horizontal gutter of the page header (`--workspace-gutter`, 1.25rem at `sm` and up; the
/// window's minimum width is always past `sm`).
const GUTTER: gpui_kit::Pixels = px(20.);
/// Height of the topbar scroll fade (`--workspace-titlebar-scroll-fade-height`, 1.5rem). Pages
/// start their content this far down (`pt-6`), so nothing is faded at rest.
pub const SCROLL_FADE_HEIGHT: gpui_kit::Pixels = px(24.);

/// `WorkspacePageContainer`'s `width` prop: the content column's max width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageWidth {
    /// `max-w-4xl`, 896px (settings).
    #[default]
    Readable,
    /// `max-w-5xl`, 1024px (Usage).
    Wide,
    /// `max-w-6xl`, 1152px (Pull Requests).
    Expanded,
}

impl PageWidth {
    pub fn max_width(self) -> gpui_kit::Pixels {
        match self {
            Self::Readable => px(896.),
            Self::Wide => px(1024.),
            Self::Expanded => px(1152.),
        }
    }
}

/// `WorkspacePageHeader`: the 52px topbar strip of a page. A window drag region (double click
/// zooms), `flex items-center gap-3`, 20px gutters, and the collapsed-sidebar inset on the left
/// so content clears the traffic lights and the sidebar toggle.
///
/// No border by default; the index and empty states add `.border_b_1().border_color(border)`.
/// Pages whose header wraps (Usage) replace the fixed height with `.h_auto()` but keep the 52px
/// minimum. Interactive children must stop mouse-down propagation (buttons do) to stay
/// clickable. `reserveNativeControls` only applies to Windows Controls Overlay, so it has no
/// equivalent on macOS.
pub fn page_header(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let inset = collapsed_titlebar_inset(cx);
    drag_region(id, window, cx)
        .h(layout::TOPBAR_HEIGHT)
        .min_h(layout::TOPBAR_HEIGHT)
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_3()
        .pl(inset.unwrap_or(GUTTER))
        .pr(GUTTER)
}

/// `WorkspacePageContainer`: the centered content column inside a page's scroll area.
/// `mx-auto w-full flex-col gap-6 px-6 pt-6 pb-12` with `width`'s max width. Override the gap
/// with `.gap_4()` etc. like the web's `className`.
pub fn page_container(width: PageWidth) -> Div {
    div()
        .mx_auto()
        .w_full()
        .max_w(width.max_width())
        .flex()
        .flex_col()
        .gap_6()
        .px_6()
        .pt_6()
        .pb_12()
}

/// `WorkspaceBreadcrumb`'s `<ol>`: `flex min-w-0 items-center gap-3 text-sm`. Children are
/// [`breadcrumb_item`]s and [`breadcrumb_separator`]s.
pub fn breadcrumb() -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap_3()
        .type_scale(text::SM)
}

/// `WorkspaceBreadcrumbItem`: `font-medium`; the `current` item is `foreground` and may shrink,
/// earlier items are `muted-foreground` and keep their width.
pub fn breadcrumb_item(current: bool, cx: &App) -> Div {
    let colors = cx.colors();
    div()
        .min_w_0()
        .flex()
        .items_center()
        .font_weight(FontWeight::MEDIUM)
        .map(|this| {
            if current {
                this.text_color(colors.foreground)
            } else {
                this.flex_shrink_0().text_color(colors.muted_foreground)
            }
        })
}

/// `WorkspaceBreadcrumbSeparator`: a "/" in `icon-muted` (muted-foreground at the default
/// contrast).
pub fn breadcrumb_separator(cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .text_color(cx.colors().muted_foreground)
        .child("/")
}

/// The `topbar-scroll-fade` utility, approximated. The web masks the top 24px of a page's
/// scroll area so content fades out under the header. GPUI has no masks, so this paints the
/// main column's glass color fading to transparent over the same band. Put it last inside a
/// `relative()` wrapper around the scroll area; it ignores the mouse.
pub fn topbar_scroll_fade(cx: &App) -> Div {
    let glass = cx.colors().app_main_glass;
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(SCROLL_FADE_HEIGHT + px(1.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(glass, 0.),
            linear_color_stop(glass.opacity(0.), 1.),
        ))
}
