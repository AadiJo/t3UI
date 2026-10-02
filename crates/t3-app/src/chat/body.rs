//! The chat body under the header: the timeline (full height under the composer overlay,
//! reserving the overlay with end padding), the scroll-to-end pill, the composer overlay whose
//! measured height sets that padding, and the file drop overlay (`chat.md` 2, 6, 8). Also the
//! actions rows trigger (folds, groups, entries, revert).

use std::time::{Duration, Instant};

use gpui_kit::{
    AnyElement, Bounds, Context, ExternalPaths, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, PromptLevel, ScrollWheelEvent, StatefulInteractiveElement as _,
    Styled as _, Window, component::scroll::ScrollableElement as _, div, list,
    prelude::FluentBuilder as _, px,
};
use t3_protocol::TurnId;
use t3_ui::{ActiveColors as _, Icon, IconName};

use super::{ChatEvent, ChatTarget, ChatView};
use crate::toast::Toast;

/// The list's top spacer (`TIMELINE_LIST_HEADER`, `h-3 sm:h-4`) and the end padding added to
/// the composer inset (`MessagesTimeline.tsx:978`).
const LIST_SPACER: f32 = 16.;
/// The composer slot's stand-in until the composer registers: the 1px frame around a 110px
/// single-line surface.
const COMPOSER_PLACEHOLDER_HEIGHT: f32 = 112.;
/// `--chat-max-width` at the default `chatWidth` ("comfortable").
pub(super) const CHAT_MAX_WIDTH: f32 = 768.;
/// `--workspace-gutter` on desktop widths.
const WORKSPACE_GUTTER: f32 = 20.;
/// A wheel gesture this far away from the end rests the composer (`composerScrollGesture.ts`).
const COLLAPSE_GESTURE_PX: f32 = 24.;
/// A wheel gesture ends after this long without events.
const COLLAPSE_GESTURE_GAP: Duration = Duration::from_millis(120);
/// The end band that re-arms following (`TIMELINE_FOLLOW_REARM_THRESHOLD_PX`).
pub(super) const FOLLOW_REARM_PX: f32 = 40.;

/// One wheel gesture over the timeline, for [`ChatEvent::CollapseComposer`].
#[derive(Clone, Copy, Debug)]
pub(super) struct WheelGesture {
    last: Instant,
    accumulated: f32,
    fired: bool,
}

impl ChatView {
    pub(super) fn render_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let empty = self.timeline.rows().is_empty() && !self.is_working();
        let timeline: AnyElement = if empty {
            // An empty timeline (a draft, or a thread without messages yet).
            div().size_full().into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .child(
                    list(
                        self.timeline.list.clone(),
                        cx.processor(|this, index: usize, window, cx| {
                            this.render_row(index, window, cx)
                        }),
                    )
                    .size_full()
                    .pt(px(LIST_SPACER))
                    .pb(self.timeline_inset + px(LIST_SPACER)),
                )
                .on_scroll_wheel(cx.listener(Self::on_timeline_wheel))
                // The 6px overlay scrollbar; dragging it stops following the end.
                .id("timeline-viewport")
                .vertical_scrollbar(&self.timeline.list)
                .into_any_element()
        };
        let show_pill = !empty && self.timeline.show_scroll_to_end();
        self.notice_timeline_end(cx);

        div()
            .id("chat-column")
            .group("chat-column")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(colors.background)
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                cx.emit(ChatEvent::FilesDropped(paths.paths().to_vec()));
            }))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(timeline)
                    .when(show_pill, |this| this.child(self.render_scroll_pill(cx))),
            )
            .child(self.render_composer_overlay(window, cx))
            .child(self.render_drop_overlay(cx))
    }

    /// Wheel events over the timeline: a gesture away from the end rests the composer.
    fn on_timeline_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height()).y;
        let now = Instant::now();
        // Positive y scrolls toward the start, away from the end.
        let can_scroll_up = -self.timeline.list.scroll_px_offset_for_scrollbar().y > px(1.);
        if delta <= px(0.) || !can_scroll_up {
            self.wheel_gesture = None;
            return;
        }
        let gesture = match self.wheel_gesture {
            Some(gesture) if now - gesture.last <= COLLAPSE_GESTURE_GAP => gesture,
            _ => WheelGesture {
                last: now,
                accumulated: 0.,
                fired: false,
            },
        };
        let mut gesture = WheelGesture {
            last: now,
            accumulated: gesture.accumulated + f32::from(delta),
            ..gesture
        };
        if !gesture.fired && gesture.accumulated >= COLLAPSE_GESTURE_PX {
            gesture.fired = true;
            cx.emit(ChatEvent::CollapseComposer);
        }
        self.wheel_gesture = Some(gesture);
    }

    /// Emits [`ChatEvent::RestoreComposer`] when the timeline comes back within the 40px end
    /// band after the user left it.
    fn notice_timeline_end(&mut self, cx: &mut Context<Self>) {
        let list = &self.timeline.list;
        let distance = list.max_offset_for_scrollbar().y + list.scroll_px_offset_for_scrollbar().y;
        let at_end = distance <= px(FOLLOW_REARM_PX);
        if at_end && !self.timeline_at_end {
            cx.emit(ChatEvent::RestoreComposer);
        }
        self.timeline_at_end = at_end;
    }

    /// "Scroll to end" (`chat.md` 6.4): centered, 4px + 6px above the composer overlay.
    fn render_scroll_pill(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(self.overlay_height + px(4.))
            .py(px(6.))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("scroll-to-end")
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    // `surface-glass`: solid background without backdrop blur.
                    .bg(colors.background)
                    .px(px(7.))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .shadow_sm()
                    .cursor_pointer()
                    .hover(|style| style.border_color(colors.border))
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(px(14.))
                            .text_color(colors.muted_foreground),
                    )
                    .child("Scroll to end")
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.emit(ChatEvent::RestoreComposer);
                        this.timeline.follow_end();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The overlay pinned to the bottom (`composer.md` 1): 8px top padding, the 20px gutter,
    /// the composer stack in the chat column, and a 20px spacer. Its measured height sets the
    /// timeline's end inset.
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
                            .bg(colors.card),
                    )
                    .into_any_element()
            }
        };
        let view = cx.weak_entity();
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, _, cx| {
                let Some(overlay) = bounds.first() else {
                    return;
                };
                let height = overlay.size.height;
                view.update(cx, |this, cx| this.set_overlay_height(height, cx))
                    .ok();
            })
            .child(
                div().w_full().pt(px(8.)).px(px(WORKSPACE_GUTTER)).child(
                    div()
                        .relative()
                        .mx_auto()
                        .w_full()
                        .max_w(px(CHAT_MAX_WIDTH))
                        .child(composer)
                        .when_some(self.branch_toolbar.clone(), |this, toolbar| {
                            this.child(toolbar)
                        })
                        .child(div().h(px(20.))),
                ),
            )
    }

    /// "Drop files to attach" while files are dragged over the chat column (`chat.md` 8).
    fn render_drop_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .absolute()
            .inset(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(18.))
            .border_2()
            .border_dashed()
            .border_color(colors.primary.opacity(0.6))
            .bg(colors.primary.opacity(0.035))
            .opacity(0.)
            .group_drag_over::<ExternalPaths>("chat-column", |style| style.opacity(1.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.primary.opacity(0.25))
                    .bg(colors.background.opacity(0.95))
                    .px(px(16.))
                    .py(px(10.))
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .shadow_lg()
                    .child(
                        Icon::new(IconName::Paperclip)
                            .size(px(16.))
                            .text_color(colors.primary),
                    )
                    .child("Drop files to attach"),
            )
    }

    /// Opens or closes a "Worked for" fold, keeping its row where it is on screen.
    pub(super) fn toggle_turn_fold(
        &mut self,
        turn: &TurnId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.timeline
            .anchor_disclosure(&format!("turn-fold:{turn}"));
        if !self.timeline.expanded_turns.remove(turn) {
            self.timeline.expanded_turns.insert(turn.clone());
        }
        self.timeline.rederive();
        self.settle_disclosure(window, cx);
    }

    /// Opens or closes a work group or tool stack; `anchor_row` (the clicked row) stays put.
    pub(super) fn toggle_work_group(
        &mut self,
        group_id: &str,
        anchor_row: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.timeline.anchor_disclosure(anchor_row);
        if !self.timeline.expanded_groups.remove(group_id) {
            self.timeline.expanded_groups.insert(group_id.to_owned());
        }
        self.timeline.rederive();
        self.settle_disclosure(window, cx);
    }

    /// Runs [`Timeline::settle_anchor`] once per frame until the anchor holds still.
    fn settle_disclosure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        let view = cx.entity();
        window.on_next_frame(move |window, cx| {
            view.update(cx, |this, cx| {
                if this.timeline.settle_anchor() {
                    this.settle_disclosure(window, cx);
                } else {
                    cx.notify();
                }
            })
        });
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
