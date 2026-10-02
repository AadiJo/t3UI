//! The chat body under the header (spec 2.2-2.3): the timeline viewport, the scroll-to-end
//! pill, and the composer overlay whose measured size sets the timeline's insets. Also the
//! actions rows trigger (folds, groups, entries, revert).

use gpui_kit::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    PromptLevel, StatefulInteractiveElement as _, Styled as _, Window, div, list,
    prelude::FluentBuilder as _, px,
};
use t3_protocol::TurnId;
use t3_ui::{ActiveColors as _, Icon, IconName};

use super::{ChatTarget, ChatView, OverlayGeometry};
use crate::toast::Toast;

/// The list's top and bottom spacer (`h-3 sm:h-4`).
const LIST_SPACER: f32 = 16.;
/// The composer slot's stand-in: the 1px frame around a 110px single-line surface.
const COMPOSER_PLACEHOLDER_HEIGHT: f32 = 112.;
/// The branch toolbar's stand-in: an xs row with 4px above and 12px below.
const TOOLBAR_PLACEHOLDER_HEIGHT: f32 = 40.;

impl ChatView {
    pub(super) fn render_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let overlay = self.overlay;
        let empty = self.timeline.rows().is_empty() && !self.is_working();
        let timeline: AnyElement = if empty {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(colors.muted_foreground.opacity(0.3))
                .child("Send a message to start the conversation.")
                .into_any_element()
        } else {
            // `height: calc(100% - viewportBottomInset)`; the end padding lets the last row
            // scroll above the composer.
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom(overlay.viewport_bottom_inset)
                .child(
                    list(
                        self.timeline.list.clone(),
                        cx.processor(|this, index: usize, window, cx| {
                            this.render_row(index, window, cx)
                        }),
                    )
                    .size_full()
                    .pt(px(LIST_SPACER))
                    .pb(px(LIST_SPACER) + overlay.content_inset_end),
                )
                .into_any_element()
        };
        let show_pill = !empty && self.timeline.show_scroll_to_end();

        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(timeline)
                    .when(show_pill, |this| {
                        this.child(self.render_scroll_pill(overlay.height, cx))
                    }),
            )
            .child(self.render_composer_overlay(window, cx))
    }

    /// "Scroll to end", centered 4px above the composer overlay.
    fn render_scroll_pill(&self, overlay_height: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(overlay_height + px(4.))
            .py(px(6.))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("scroll-to-end")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.card)
                    .px(px(12.))
                    .py(px(4.))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(colors.muted_foreground)
                    .shadow_sm()
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .border_color(colors.border)
                            .text_color(colors.foreground)
                    })
                    .child(Icon::new(IconName::ChevronDown).size(px(14.)))
                    .child("Scroll to end")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.timeline.follow_end();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The overlay pinned to the bottom: the composer slot (max 768px) and the lower chrome
    /// strip with the branch toolbar. Its measured size feeds [`OverlayGeometry`].
    fn render_composer_overlay(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let composer = match &self.composer {
            Some(view) => view.clone().into_any_element(),
            None => {
                // Stand-in: the composer frame (rounded 22, 1px padding) around its surface.
                div()
                    .w_full()
                    .h(px(COMPOSER_PLACEHOLDER_HEIGHT))
                    .rounded(px(22.))
                    .p(px(1.))
                    .child(
                        div()
                            .size_full()
                            .rounded(px(20.))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.composer_glass),
                    )
                    .into_any_element()
            }
        };
        let toolbar = match &self.branch_toolbar {
            Some(view) => view.clone().into_any_element(),
            None => div().h(px(TOOLBAR_PLACEHOLDER_HEIGHT)).into_any_element(),
        };
        let view = cx.weak_entity();
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .pt(px(8.))
            .flex()
            .flex_col()
            .on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, _, cx| {
                let [composer, chrome] = bounds[..] else {
                    return;
                };
                let overlay = Bounds::from_corners(
                    gpui_kit::point(composer.left(), composer.top() - px(8.)),
                    chrome.bottom_right(),
                );
                let geometry = OverlayGeometry::measure(overlay, composer);
                view.update(cx, |this, cx| this.set_overlay(geometry, cx))
                    .ok();
            })
            .child(
                div().px(px(20.)).child(
                    div()
                        .relative()
                        .mx_auto()
                        .w_full()
                        .max_w(px(768.))
                        .child(composer),
                ),
            )
            // `.chat-composer-lower-chrome`: card at 45% (light 20%), clear of the scrollbar.
            .child(
                div()
                    .mt(px(-1.))
                    .mr(px(6.))
                    .pt(px(1.))
                    .pb(px(4.))
                    .px(px(20.))
                    .bg(colors.card.opacity(if colors.is_dark { 0.45 } else { 0.2 }))
                    .child(toolbar),
            )
    }

    pub(super) fn toggle_turn_fold(&mut self, turn: &TurnId, cx: &mut Context<Self>) {
        self.timeline.anchor_disclosure();
        if !self.timeline.expanded_turns.remove(turn) {
            self.timeline.expanded_turns.insert(turn.clone());
        }
        self.timeline.rederive();
        cx.notify();
    }

    pub(super) fn toggle_work_group(&mut self, group_id: &str, cx: &mut Context<Self>) {
        self.timeline.anchor_disclosure();
        if !self.timeline.expanded_groups.remove(group_id) {
            self.timeline.expanded_groups.insert(group_id.to_owned());
        }
        self.timeline.rederive();
        cx.notify();
    }

    pub(super) fn toggle_entry(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.timeline.expanded_entries.remove(key) {
            self.timeline.expanded_entries.insert(key.to_owned());
        }
        self.timeline.remeasure_entry(key);
        cx.notify();
    }

    /// "Revert to this message" (`onRevertToTurnCount`): refused while working or offline,
    /// confirmed natively, then `thread.checkpoint.revert`.
    pub(super) fn revert_to_turn_count(
        &mut self,
        turn_count: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ChatTarget::Thread(thread) = &self.target else {
            return;
        };
        if self.is_working() {
            self.local_error =
                Some("Interrupt the current turn before reverting checkpoints.".to_owned());
            cx.notify();
            return;
        }
        let Some(environment) = self.environment.clone() else {
            return;
        };
        if environment.read(cx).client().is_none() {
            let label = environment.read(cx).label().clone();
            self.local_error = Some(format!("Reconnect {label} before reverting checkpoints."));
            cx.notify();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Revert this thread to checkpoint {turn_count}?"),
            Some(
                "This will discard newer messages and turn diffs in this thread.\nThis action cannot be undone.",
            ),
            &["Revert", "Cancel"],
            cx,
        );
        let thread_id = thread.thread_id.clone();
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let dispatch = this.update(cx, |this, cx| {
                this.reverting = true;
                this.refresh_rows(cx);
                environment.read(cx).dispatch(
                    t3_client::commands::revert_checkpoint(thread_id, turn_count),
                    cx,
                )
            });
            let Ok(dispatch) = dispatch else { return };
            let result = dispatch.await;
            this.update(cx, |this, cx| {
                this.reverting = false;
                if let Err(error) = result {
                    crate::toast::show(
                        Toast::error("Failed to revert thread.").description(error.to_string()),
                        cx,
                    );
                }
                this.refresh_rows(cx);
            })
            .ok();
        })
        .detach();
    }
}
