//! What the list shows instead of rows: the loading ghost, the empty states, the unavailable
//! state, and the banners under a list (`PullRequestGhosts.tsx`, `PullRequestListEmptyState.tsx`,
//! `PullRequestsUnavailableState.tsx`). Spec sections 4.3-4.5.

use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, Hsla, IntoElement, LineFragment, ParentElement as _,
    PathBuilder, Pixels, Point, SharedString, Styled as _, Window, canvas, div, point,
    prelude::FluentBuilder as _, px, relative,
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Colors, Icon, IconName, tokens::text,
};

use super::style;
use crate::chrome::TypeScale as _;

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A button in an empty or unavailable state.
pub struct StateAction {
    pub id: &'static str,
    pub label: SharedString,
    pub icon: Option<IconName>,
    pub variant: ButtonVariant,
    pub disabled: bool,
    pub on_click: Handler,
}

impl StateAction {
    pub fn outline(
        id: &'static str,
        label: impl Into<SharedString>,
        icon: Option<IconName>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            icon,
            variant: ButtonVariant::Outline,
            disabled: false,
            on_click: Box::new(on_click),
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    fn into_button(self) -> AnyElement {
        Button::new(self.id)
            .variant(self.variant)
            .size(ButtonSize::Sm)
            .label(self.label)
            .when_some(self.icon, |this, icon| this.icon(icon))
            .disabled(self.disabled)
            .on_click(self.on_click)
            .into_any_element()
    }
}

/// Widths the ghost's title and meta bars cycle through, so it renders the same every pass.
const TITLE_WIDTHS: [f32; 7] = [0.6, 0.4, 0.5, 0.666, 0.4, 0.6, 0.5];
const META_WIDTHS: [f32; 7] = [0.4, 0.333, 0.4, 0.25, 0.333, 0.4, 0.333];

fn bar(colors: &Colors) -> gpui_kit::Div {
    div().h_3().rounded(px(4.)).bg(style::ghost_bar(colors))
}

/// Rows in the list's own grid, with an optional caption where group headers speak. Drawn
/// still: the fork's stepped pulse would repaint for as long as the list loads.
pub fn list_ghost(rows: usize, caption: Option<SharedString>, cx: &App) -> AnyElement {
    let colors = cx.colors();
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .when_some(caption, |this, caption| {
            this.child(
                div()
                    .px_3()
                    .pb_1()
                    .type_scale(text::XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground_70)
                    .child(caption),
            )
        })
        .children((0..rows).map(|index| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(6.))
                .px_3()
                .py(px(10.))
                .child(bar(colors).size_4().rounded_full().flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(bar(colors).h_4().w(relative(TITLE_WIDTHS[index % 7])))
                        .child(bar(colors).h(px(14.)).w(relative(META_WIDTHS[index % 7]))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(6.))
                        .child(bar(colors).w_12())
                        .child(bar(colors).w_16()),
                )
        }))
        .into_any_element()
}

/// The `Empty` frame: centered column, 24px gaps, 48px padding.
fn empty_frame() -> gpui_kit::Div {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_6()
        .p_12()
}

fn header(title: SharedString, description: SharedString, cx: &App) -> impl IntoElement {
    let colors = cx.colors();
    let description_width = balanced_width(&description, text::SM.0, HEADER_WIDTH, cx);
    div()
        .flex()
        .flex_col()
        .items_center()
        .max_w(HEADER_WIDTH)
        .text_center()
        .child(
            div()
                .type_scale(text::XL)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.foreground)
                .child(title),
        )
        .child(
            div()
                .mt_1()
                .max_w(description_width)
                .type_scale(text::SM)
                .text_color(colors.muted_foreground)
                .child(description),
        )
}

/// `EmptyHeader`'s `max-w-sm`.
const HEADER_WIDTH: Pixels = px(384.);

/// The width CSS `text-balance` settles on: the narrowest width that still wraps `text` into as
/// many lines as `max` does, so the lines come out even instead of one long and one short.
fn balanced_width(text: &str, size: Pixels, max: Pixels, cx: &App) -> Pixels {
    let mut wrapper = cx
        .text_system()
        .line_wrapper(gpui_kit::font(t3_ui::tokens::font::SANS), size);
    let fragments = [LineFragment::text(text)];
    let mut lines_at = |width: Pixels| wrapper.wrap_line(&fragments, width).count() + 1;
    let lines = lines_at(max);
    if lines <= 1 {
        return max;
    }
    let (mut narrow, mut wide) = (max / lines as f32 * 0.8, max);
    for _ in 0..12 {
        let middle = (narrow + wide) / 2.;
        if lines_at(middle) <= lines {
            wide = middle;
        } else {
            narrow = middle;
        }
    }
    wide + px(1.)
}

fn actions(actions: Vec<StateAction>) -> Option<impl IntoElement> {
    (!actions.is_empty()).then(|| {
        div()
            .flex()
            .flex_wrap()
            .justify_center()
            .gap_2()
            .children(actions.into_iter().map(StateAction::into_button))
    })
}

/// An empty list with the branch mark (`PullRequestListEmptyState`).
pub fn empty_state(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    buttons: Vec<StateAction>,
    cx: &App,
) -> AnyElement {
    let colors = cx.colors();
    empty_frame()
        .child(branch_mark(colors))
        .child(header(title.into(), description.into(), cx))
        .children(actions(buttons))
        .into_any_element()
}

/// A failure in place of the list (`PullRequestsUnavailableState`): the stacked pull request
/// card, the title, the message, and Retry where retrying can help.
pub fn unavailable_state(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    buttons: Vec<StateAction>,
    cx: &App,
) -> AnyElement {
    let colors = cx.colors();
    empty_frame()
        .child(media_card(colors))
        .child(header(title.into(), message.into(), cx))
        .children(actions(buttons))
        .into_any_element()
}

/// `EmptyMedia variant="icon"`: a 36px card with two ghost copies fanned behind it.
fn media_card(colors: &Colors) -> impl IntoElement {
    let card = || {
        div()
            .absolute()
            .size_9()
            .rounded(px(8.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.card)
    };
    // GPUI cannot rotate a div, so the ghost copies are offset rather than fanned.
    div()
        .relative()
        .size_9()
        .mb_6()
        .child(card().left(px(-5.)).top(px(2.)).opacity(0.6))
        .child(card().left(px(5.)).top(px(2.)).opacity(0.6))
        .child(
            card()
                .left_0()
                .top_0()
                .flex()
                .items_center()
                .justify_center()
                .shadow(t3_ui::tokens::shadow::SM_5.to_vec())
                .child(
                    Icon::new(IconName::GitPullRequestArrow)
                        .size(px(18.))
                        .color(colors.foreground),
                ),
        )
}

/// The page's own empty drawing: a base line and a branch that leaves it and does not come
/// back (`BranchMark joined={false}`), 120×72 viewBox drawn at 128×80.
fn branch_mark(colors: &Colors) -> impl IntoElement {
    let tone = colors.muted_foreground.opacity(0.6);
    let base = colors.muted_foreground.opacity(0.3);
    let dashed = colors.muted_foreground.opacity(0.5);
    let node = colors.muted_foreground.opacity(0.45);
    div().w(px(128.)).h(px(80.)).child(
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let scale = (bounds.size.width / px(120.)).min(bounds.size.height / px(72.));
                let origin = point(
                    bounds.origin.x + (bounds.size.width - px(120.) * scale) / 2.,
                    bounds.origin.y + (bounds.size.height - px(72.) * scale) / 2.,
                );
                let at = |x: f32, y: f32| -> Point<Pixels> {
                    point(origin.x + px(x) * scale, origin.y + px(y) * scale)
                };
                let width = px(2.) * scale;
                let stroke = |build: &dyn Fn(&mut PathBuilder),
                              color: Hsla,
                              dash: Option<&[Pixels]>,
                              window: &mut Window| {
                    let mut path = PathBuilder::stroke(width);
                    if let Some(dash) = dash {
                        path = path.dash_array(dash);
                    }
                    build(&mut path);
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                };
                let circle = |cx: f32, cy: f32, r: f32| {
                    move |path: &mut PathBuilder| {
                        path.move_to(at(cx - r, cy));
                        path.arc_to(
                            point(px(r) * scale, px(r) * scale),
                            px(0.),
                            false,
                            true,
                            at(cx + r, cy),
                        );
                        path.arc_to(
                            point(px(r) * scale, px(r) * scale),
                            px(0.),
                            false,
                            true,
                            at(cx - r, cy),
                        );
                        path.close();
                    }
                };
                let fill_circle = |cx: f32, cy: f32, r: f32, color: Hsla, window: &mut Window| {
                    let mut path = PathBuilder::fill();
                    circle(cx, cy, r)(&mut path);
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                };
                stroke(
                    &|path| {
                        path.move_to(at(10., 58.));
                        path.line_to(at(110., 58.));
                    },
                    base,
                    None,
                    window,
                );
                for x in [10., 110.] {
                    fill_circle(x, 58., 5., tone.opacity(0.25), window);
                    stroke(&circle(x, 58., 5.), tone, None, window);
                }
                stroke(
                    &|path| {
                        path.move_to(at(30., 58.));
                        path.cubic_bezier_to(at(54., 32.), at(30., 40.), at(38., 32.));
                        path.line_to(at(58., 32.));
                    },
                    tone,
                    None,
                    window,
                );
                stroke(
                    &|path| {
                        path.move_to(at(90., 58.));
                        path.cubic_bezier_to(at(66., 32.), at(90., 40.), at(82., 32.));
                        path.line_to(at(62., 32.));
                    },
                    dashed,
                    Some(&[px(2.) * scale, px(7.) * scale]),
                    window,
                );
                stroke(&circle(60., 32., 4.), node, None, window);
            },
        )
        .size_full(),
    )
}

/// The warning under a list whose refresh failed while rows were showing.
pub fn error_banner(message: &str, retry: StateAction, cx: &App) -> AnyElement {
    let colors = cx.colors();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .rounded(px(10.))
        .border_1()
        .border_color(colors.warning.opacity(0.3))
        .bg(colors.warning_surface)
        .px_3()
        .py_2()
        .type_scale(text::XS)
        .text_color(colors.foreground)
        .child(format!("{message} Showing the last pull requests loaded."))
        .child(
            Button::new(retry.id)
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .label(retry.label)
                .on_click(retry.on_click),
        )
        .into_any_element()
}

/// The footer under a truncated list: Load more, or the cap message.
pub fn load_more_footer(content: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .justify_center()
        .py_3()
        .type_scale(text::XS)
        .text_color(cx.colors().muted_foreground)
        .child(content)
        .into_any_element()
}
