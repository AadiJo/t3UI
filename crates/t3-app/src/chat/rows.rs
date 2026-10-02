//! Timeline row renderers (`MessagesTimeline.tsx` `TimelineRowContent` and the row components,
//! spec 3.3-3.10). Each takes the derived [`TimelineRow`] and draws it with the fork's
//! geometry; clicks route back to [`ChatView`].

use std::{f32::consts::PI, sync::Arc};

use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Transformation, Window, div, linear_color_stop, linear_gradient, prelude::FluentBuilder as _,
    px, radians, relative,
};
use t3_logic::{
    settings::TimestampFormat,
    timeline::{
        self, MessageRow, RequestKind, RowKind, TimelineRow, ToolItemType, ToolStatus,
        WorkLogEntry, WorkTone, format_short_timestamp, format_timestamp_tooltip,
        format_working_timer, format_workspace_relative_path,
    },
};
use t3_protocol::{
    TurnId,
    orchestration::{MessageRole, OrchestrationCheckpointSummary, OrchestrationProposedPlan},
};
use t3_ui::{
    ActiveColors as _, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Colors, Icon,
    IconName, Theme, TooltipExt as _,
};

use super::{ChatView, controls::small_icon, markdown::TextKind};
use crate::chrome::TypeScale as _;

/// User messages longer than this collapse behind "Show full message".
const COLLAPSE_CHARS: usize = 600;
const COLLAPSE_LINES: usize = 8;

/// Values every row reads, gathered once per row render.
pub(super) struct RowEnv {
    colors: &'static Colors,
    mono: SharedString,
    workspace_root: Option<String>,
    timestamp_format: TimestampFormat,
    now: i64,
    /// Work or an unsettled latest turn: work rows show live status.
    turn_in_progress: bool,
    is_working: bool,
    reverting: bool,
}

fn rotated(icon: Icon, open: bool) -> Icon {
    if open {
        icon.transform(Transformation::rotate(radians(PI)))
    } else {
        icon
    }
}

impl ChatView {
    fn row_env(&self, cx: &App) -> RowEnv {
        let app_state = self.app_state.read(cx);
        RowEnv {
            colors: cx.colors(),
            mono: Theme::global(cx).mono_family().clone(),
            workspace_root: self.workspace_root(cx),
            timestamp_format: app_state.settings().timestamp_format,
            now: app_state.now_millis(),
            turn_in_progress: self.turn_in_progress(),
            is_working: self.is_working(),
            reverting: self.reverting,
        }
    }

    /// The row at `index` in the list.
    pub(super) fn render_row(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self.timeline.rows().get(index).cloned() else {
            return div().into_any_element();
        };
        let env = self.row_env(cx);
        let colors = env.colors;
        let content = match &row.kind {
            RowKind::Message(message) => match message.message.role {
                MessageRole::User => self.user_message(&row, message, &env, cx),
                _ => self.assistant_message(message, &env, cx),
            },
            RowKind::Work { entries } => self.work_group(entries, &env, cx),
            RowKind::ToolStack {
                group_id,
                entries,
                expanded,
            } => self.tool_stack(&row.id, group_id, entries, *expanded, &env, cx),
            RowKind::WorkToggle {
                group_id,
                hidden_count,
                expanded,
                only_tool_entries,
            } => self.work_toggle(
                &row.id,
                group_id,
                *hidden_count,
                *expanded,
                *only_tool_entries,
                &env,
                cx,
            ),
            RowKind::TurnFold {
                turn_id,
                label,
                expanded,
            } => self.turn_fold(turn_id, label, *expanded, &env, cx),
            RowKind::ProposedPlan(plan) => self.plan_row(plan, &env, cx),
            RowKind::ChangedFiles(summary) => self.changed_files(summary, &env, cx),
            RowKind::Working { started_at } => working_row(started_at.as_deref(), &env),
        };
        // `mx-auto w-full max-w-3xl` inside the list's 20px side padding; the row's own bottom
        // padding and turn-fold detail rule.
        div()
            .w_full()
            .px(px(20.))
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(768.))
                    .min_w_0()
                    .pb(px(if row.is_compact() { 8. } else { 16. }))
                    .when(row.fold_detail.is_some(), |this| {
                        this.border_l_2()
                            .border_color(colors.border.opacity(0.7))
                            .bg(colors.muted.opacity(0.2))
                            .pl(px(12.))
                            .pr(px(8.))
                    })
                    .child(content),
            )
            .into_any_element()
    }

    // --- Messages ------------------------------------------------------------------------

    fn user_message(
        &mut self,
        row: &TimelineRow,
        message: &MessageRow,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let text = message.message.text.as_str();
        let id = message.message.id.to_string();
        let can_collapse = !text.trim().is_empty()
            && (text.chars().count() > COLLAPSE_CHARS || text.split('\n').count() > COLLAPSE_LINES);
        let expanded = self.timeline.expanded_messages.contains(&id);
        let collapsed = can_collapse && !expanded;
        let group = SharedString::from(format!("user-row:{id}"));
        let markdown = self.timeline.markdown.view(
            &id,
            text,
            false,
            TextKind::User,
            env.workspace_root.as_deref(),
            cx,
        );

        let body = div()
            .relative()
            .when(collapsed, |this| this.max_h(px(176.)).overflow_hidden())
            .child(markdown)
            .when(collapsed, |this| {
                // The web masks the last 28px to transparent; fade into the bubble instead.
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(28.))
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(colors.secondary.opacity(0.), 0.),
                            linear_color_stop(colors.secondary, 1.),
                        )),
                )
            });
        let toggle_id = id.clone();
        let row_id = row.id.clone();
        let bubble = div()
            .relative()
            .max_w(relative(0.8))
            .rounded(px(18.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.secondary)
            .p(px(12.))
            .child(body)
            .when(can_collapse, |this| {
                this.child(
                    div().mt(px(6.)).flex().child(
                        div()
                            .id(SharedString::from(format!("show-full:{id}")))
                            .ml(px(-4.))
                            .h(px(24.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .type_scale((px(12.), px(16.)))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.muted_foreground.opacity(0.72))
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(colors.muted.opacity(0.55))
                                    .text_color(colors.foreground.opacity(0.85))
                            })
                            .child(if expanded {
                                "Show less"
                            } else {
                                "Show full message"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.timeline.expanded_messages.remove(&toggle_id) {
                                    this.timeline.expanded_messages.insert(toggle_id.clone());
                                }
                                this.timeline.remeasure_row(&row_id);
                                cx.notify();
                            })),
                    ),
                )
            });

        let created_at = message.message.created_at.clone();
        let revert = message.revert_turn_count.map(|count| {
            let disabled = env.reverting || env.is_working;
            ghost_icon_button(format!("revert:{id}"), IconName::Undo2, colors)
                .disabled(disabled)
                .tooltip("Revert to this message")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.revert_to_turn_count(count, window, cx)
                }))
                .into_any_element()
        });
        let meta = div()
            .flex()
            .w_full()
            .max_w(relative(0.8))
            .justify_end()
            .pr(px(4.))
            .type_scale((px(12.), px(16.)))
            .opacity(0.)
            .group_hover(group.clone(), |style| style.opacity(1.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(timestamp(&created_at, &id, env))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .children(revert)
                            .child(self.copy_button(
                                format!("copy:{id}"),
                                text.to_owned(),
                                env,
                                cx,
                            )),
                    ),
            );

        div()
            .group(group)
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.))
            .child(bubble)
            .child(meta)
            .into_any_element()
    }

    fn assistant_message(
        &mut self,
        message: &MessageRow,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = &message.message;
        let id = source.id.to_string();
        let text = if source.text.is_empty() && !source.streaming {
            "(empty response)"
        } else {
            source.text.as_str()
        };
        let group = SharedString::from(format!("assistant-row:{id}"));
        let copy_visible = !message.assistant_copy_streaming && !source.text.trim().is_empty();
        let markdown = self.timeline.markdown.view(
            &id,
            text,
            source.streaming,
            TextKind::Assistant,
            env.workspace_root.as_deref(),
            cx,
        );
        div()
            .group(group.clone())
            .relative()
            .min_w_0()
            .px(px(4.))
            .py(px(2.))
            .child(markdown)
            .when(message.show_assistant_meta, |this| {
                this.child(
                    div()
                        .mt(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .type_scale((px(12.), px(16.)))
                        .opacity(0.)
                        .group_hover(group, |style| style.opacity(1.))
                        .when(copy_visible, |this| {
                            this.child(self.copy_button(
                                format!("copy:{id}"),
                                source.text.clone(),
                                env,
                                cx,
                            ))
                        })
                        .when(!source.streaming, |this| {
                            this.child(timestamp(&source.updated_at, &id, env))
                        }),
                )
            })
            .into_any_element()
    }

    /// `MessageCopyButton`: ghost xs, copy icon that turns into a check for a second.
    fn copy_button(
        &mut self,
        key: String,
        text: String,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let copied = self.timeline.copied.as_deref() == Some(key.as_str());
        ghost_icon_button(
            key.clone(),
            if copied {
                IconName::Check
            } else {
                IconName::Copy
            },
            colors,
        )
        .when(copied, |button| button.text_color(colors.primary))
        .disabled(copied)
        .tooltip("Copy to clipboard")
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            this.timeline.copied = Some(key.clone());
            cx.notify();
            let key = key.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1000))
                    .await;
                this.update(cx, |this, cx| {
                    if this.timeline.copied.as_deref() == Some(key.as_str()) {
                        this.timeline.copied = None;
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }))
        .into_any_element()
    }

    // --- Work log --------------------------------------------------------------------------

    /// `WorkGroupSection`: entries under a "Work Log" label unless every entry is tool-like.
    fn work_group(
        &mut self,
        entries: &[Arc<WorkLogEntry>],
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visible: Vec<&Arc<WorkLogEntry>> =
            entries.iter().filter(|e| !e.indicates_neutral()).collect();
        let only_tools = visible.iter().all(|e| e.is_tool_like());
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .py(px(2.))
            .when(!only_tools, |this| {
                this.child(
                    div()
                        .px(px(2.))
                        .pb(px(2.))
                        .text_size(px(11.))
                        .line_height(px(16.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(env.colors.muted_foreground.opacity(0.65))
                        .child("Work Log"),
                )
            })
            .child(
                div().flex().flex_col().gap(px(1.)).children(
                    visible
                        .into_iter()
                        .map(|entry| self.work_entry(entry, env, cx)),
                ),
            )
            .into_any_element()
    }

    /// `ToolCallStack`: the latest call, the history when expanded, and a hover toggle.
    fn tool_stack(
        &mut self,
        row_id: &str,
        group_id: &str,
        entries: &[Arc<WorkLogEntry>],
        expanded: bool,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let Some((latest, previous)) = entries.split_last() else {
            return div().into_any_element();
        };
        let group = SharedString::from(format!("tool-stack:{row_id}"));
        let toggle_group = group_id.to_owned();
        let count = entries.len();
        div()
            .group(group.clone())
            .relative()
            .py(px(2.))
            .child(
                div()
                    .relative()
                    .min_h(px(24.))
                    .overflow_hidden()
                    .child(self.work_entry(latest, env, cx)),
            )
            .when(expanded && !previous.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .pt(px(4.))
                        .children(previous.iter().map(|entry| self.work_entry(entry, env, cx))),
                )
            })
            .when(!previous.is_empty(), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("stack-toggle:{row_id}")))
                        .absolute()
                        .top(px(4.))
                        .left(px(4.))
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(colors.border.opacity(0.4))
                        .bg(colors.background.opacity(0.9))
                        .text_color(colors.muted_foreground)
                        .cursor_pointer()
                        .when(!expanded, |this| {
                            this.opacity(0.)
                                .group_hover(group, |style| style.opacity(1.))
                        })
                        .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                        .tooltip_text(if expanded {
                            "Collapse tool call history".to_owned()
                        } else {
                            format!("Show all {count} tool calls")
                        })
                        .child(rotated(
                            Icon::new(IconName::ChevronDown).size(px(14.)),
                            expanded,
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_work_group(&toggle_group, cx)
                        })),
                )
            })
            .into_any_element()
    }

    /// `WorkGroupToggleTimelineRow`: "+N previous log entries" / "Show fewer log entries".
    #[allow(clippy::too_many_arguments)]
    fn work_toggle(
        &mut self,
        row_id: &str,
        group_id: &str,
        hidden_count: usize,
        expanded: bool,
        only_tool_entries: bool,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let noun = if only_tool_entries {
            ("tool call", "tool calls")
        } else {
            ("log entry", "log entries")
        };
        let label = if expanded {
            format!("Show fewer {}", noun.1)
        } else if hidden_count == 1 {
            format!("+1 previous {}", noun.0)
        } else {
            format!("+{hidden_count} previous {}", noun.1)
        };
        let group_id = group_id.to_owned();
        div()
            .id(SharedString::from(row_id.to_owned()))
            .flex()
            .w_full()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .px(px(2.))
            .py(px(2.))
            .text_size(px(12.))
            .line_height(px(20.))
            .cursor_pointer()
            .hover(|style| style.bg(colors.accent.opacity(0.2)))
            .child(
                div()
                    .size(px(20.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(rotated(
                        Icon::new(IconName::ChevronDown)
                            .size(px(14.))
                            .color(colors.muted_foreground.opacity(0.65 * 0.7)),
                        expanded,
                    )),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground.opacity(0.82))
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_work_group(&group_id, cx)))
            .into_any_element()
    }

    /// `TurnFoldTimelineRow`: "Worked for 1.9s ›" over a 60% border.
    fn turn_fold(
        &mut self,
        turn_id: &TurnId,
        label: &str,
        expanded: bool,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let turn = turn_id.clone();
        div()
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .pb(px(8.))
            .pt(px(4.))
            .flex()
            .child(
                div()
                    .id(SharedString::from(format!("turn-fold:{turn_id}")))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(8.))
                    .px(px(4.))
                    .type_scale((px(12.), px(16.)))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .hover(|style| style.text_color(colors.foreground))
                    .child(label.to_owned())
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(14.)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_turn_fold(&turn, cx))),
            )
            .into_any_element()
    }

    /// `SimpleWorkEntryRow`: icon, heading, preview, and status; click to show the command and
    /// output.
    fn work_entry(
        &mut self,
        entry: &WorkLogEntry,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let key = entry.presentation_key();
        let heading = entry.heading();
        let preview = work_entry_preview(entry, env.workspace_root.as_deref()).filter(|preview| {
            timeline::normalize_compact_tool_label(preview).to_lowercase()
                != timeline::normalize_compact_tool_label(&heading).to_lowercase()
        });
        let body = expanded_body(entry, env.workspace_root.as_deref());
        let can_expand = body.is_some();
        let expanded = can_expand && self.timeline.expanded_entries.contains(&key);

        let warning = entry.source_activity_kind == "runtime.warning";
        let failed = entry.indicates_failure();
        let destructive =
            failed && (entry.source_activity_kind == "runtime.error" || !entry.is_tool_like());
        let icon_color = if warning || destructive {
            colors.destructive
        } else if entry.tone == WorkTone::Tool || failed {
            colors.muted_foreground.opacity(0.65)
        } else {
            match entry.tone {
                WorkTone::Info => colors.muted_foreground,
                _ => colors.foreground.opacity(0.92),
            }
        };
        let heading_color = if warning {
            colors.warning
        } else if destructive {
            colors.destructive
        } else {
            colors.foreground.opacity(0.82)
        };
        let settled = !env.turn_in_progress;
        let status = if failed {
            Some((
                Icon::new(IconName::X)
                    .size(px(12.))
                    .color(colors.destructive)
                    .into_any_element(),
                "Failed",
            ))
        } else if entry.indicates_success() || (settled && entry.indicates_neutral()) {
            Some((
                Icon::new(IconName::Check).size(px(12.)).into_any_element(),
                "Completed",
            ))
        } else if !settled && entry.lifecycle_status == Some(ToolStatus::InProgress) {
            Some((
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(colors.muted_foreground.opacity(0.65))
                    .into_any_element(),
                "Running",
            ))
        } else if !settled && entry.indicates_neutral() {
            Some((
                Icon::new(IconName::Minus)
                    .size(px(12.))
                    .opacity(0.7)
                    .into_any_element(),
                "Empty",
            ))
        } else {
            None
        };

        let toggle_key = key.clone();
        let line = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .size(px(20.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(if warning {
                            IconName::X
                        } else {
                            work_entry_icon(entry)
                        })
                        .size(px(14.))
                        .color(icon_color.opacity(icon_color.a * 0.8)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div().flex_1().min_w_0().overflow_hidden().child(
                            div()
                                .flex()
                                .w_full()
                                .min_w_0()
                                .items_baseline()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .line_height(px(20.))
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_shrink(1.)
                                        .truncate()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(heading_color)
                                        .child(heading),
                                )
                                .when_some(preview, |this, preview| {
                                    this.child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .truncate()
                                            .text_color(colors.muted_foreground.opacity(0.55))
                                            .child(preview),
                                    )
                                }),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(1.))
                            .text_color(colors.muted_foreground.opacity(0.55))
                            .child(
                                div()
                                    .size(px(16.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(can_expand, |this| {
                                        this.child(rotated(
                                            Icon::new(IconName::ChevronDown)
                                                .size(px(12.))
                                                .opacity(0.7),
                                            expanded,
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("status:{key}")))
                                    .size(px(16.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when_some(status, |this, (indicator, tooltip)| {
                                        this.child(indicator).tooltip_text(tooltip)
                                    }),
                            ),
                    ),
            );

        div()
            .id(SharedString::from(format!("entry:{key}")))
            .flex()
            .flex_col()
            .rounded(px(8.))
            .px(px(2.))
            .py(px(2.))
            .when(can_expand, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(colors.accent.opacity(0.2)))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_entry(&toggle_key, cx)))
            })
            .child(line)
            .when_some(body.filter(|_| expanded), |this, body| {
                this.child(
                    div()
                        .id(SharedString::from(format!("entry-body:{key}")))
                        .mt(px(4.))
                        .ml(px(28.))
                        .border_l_1()
                        .border_color(colors.border.opacity(0.45))
                        .pl(px(12.))
                        .pt(px(2.))
                        .cursor_default()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .id(SharedString::from(format!("entry-pre:{key}")))
                                .max_h(px(256.))
                                .overflow_y_scroll()
                                .font_family(env.mono.clone())
                                .text_size(px(11.))
                                .line_height(px(11. * 1.625))
                                .text_color(colors.muted_foreground)
                                .child(body),
                        ),
                )
            })
            .into_any_element()
    }

    // --- Plan and changed files ------------------------------------------------------------

    /// `ProposedPlanCard` in its row (`min-w-0 px4 py2`).
    fn plan_row(
        &mut self,
        plan: &Arc<OrchestrationProposedPlan>,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let markdown_text = plan.plan_markdown.as_str();
        let id = plan.id.to_string();
        let title = timeline::plan::plan_title(markdown_text)
            .unwrap_or("Proposed plan")
            .to_owned();
        let can_collapse = timeline::plan::plan_can_collapse(markdown_text);
        let expanded = self.timeline.expanded_plans.contains(&id);
        let collapsed = can_collapse && !expanded;
        let body_text = if collapsed {
            timeline::plan::collapsed_plan_preview(markdown_text, 10)
        } else {
            timeline::plan::displayed_plan_markdown(markdown_text)
        };
        let markdown = self.timeline.markdown.view(
            &format!("{}:{id}", if collapsed { "plan-preview" } else { "plan" }),
            &body_text,
            false,
            TextKind::Assistant,
            env.workspace_root.as_deref(),
            cx,
        );
        let toggle_id = id.clone();
        let row_id = id.clone();
        let card = div()
            .rounded(px(24.))
            .border_1()
            .border_color(colors.border.opacity(0.8))
            .bg(colors.card.opacity(0.7))
            .p(px(20.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.))
                            .child(Badge::new("Plan").variant(BadgeVariant::Secondary))
                            .child(
                                div()
                                    .truncate()
                                    .type_scale((px(14.), px(20.)))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(title),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("plan-actions:{id}")))
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::IconXs)
                            .child(Icon::new(IconName::Ellipsis).size(px(16.)).opacity(0.8)),
                    ),
            )
            .child(
                div()
                    .mt(px(16.))
                    .child(
                        div()
                            .relative()
                            .when(collapsed, |this| this.max_h(px(416.)).overflow_hidden())
                            .child(markdown)
                            .when(collapsed, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .right_0()
                                        .bottom_0()
                                        .h(px(96.))
                                        .bg(linear_gradient(
                                            180.,
                                            linear_color_stop(colors.card.opacity(0.), 0.),
                                            linear_color_stop(colors.card.opacity(0.95), 1.),
                                        )),
                                )
                            }),
                    )
                    .when(can_collapse, |this| {
                        this.child(
                            div().mt(px(16.)).flex().justify_center().child(
                                Button::new(SharedString::from(format!("plan-toggle:{id}")))
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .label(if expanded {
                                        "Collapse plan"
                                    } else {
                                        "Expand plan"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.timeline.expanded_plans.remove(&toggle_id) {
                                            this.timeline.expanded_plans.insert(toggle_id.clone());
                                        }
                                        this.timeline.remeasure_row(&row_id);
                                        cx.notify();
                                    })),
                            ),
                        )
                    }),
            );
        div()
            .min_w_0()
            .px(px(4.))
            .py(px(2.))
            .child(card)
            .into_any_element()
    }

    /// The changed-files card after a turn (`AssistantChangedFilesSection`).
    fn changed_files(
        &mut self,
        summary: &Arc<OrchestrationCheckpointSummary>,
        env: &RowEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = env.colors;
        let tree = self.timeline.tree(summary, cx);
        let all_expanded = self
            .timeline
            .changed_files_expanded
            .get(&summary.turn_id)
            .copied()
            .unwrap_or_else(|| tree.read(cx).all_expanded_by_default());
        let (additions, deletions) = summary.files.iter().fold((0, 0), |(a, d), file| {
            (a + file.additions, d + file.deletions)
        });
        let turn = summary.turn_id.clone();
        let stat_column = |text: String, color: Hsla| {
            div()
                .w(px(10. * 0.6 * 4.))
                .flex()
                .justify_end()
                .whitespace_nowrap()
                .text_color(color)
                .child(text)
        };
        div()
            .mt(px(8.))
            .rounded(px(10.))
            .border_1()
            .border_color(colors.border.opacity(0.8))
            .bg(colors.card.opacity(0.45))
            .p(px(10.))
            .child(
                div()
                    .mb(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .text_size(px(10.))
                            .line_height(px(15.))
                            .text_color(colors.muted_foreground.opacity(0.65))
                            .child(format!("CHANGED FILES ({})", summary.files.len()))
                            .when(additions > 0 || deletions > 0, |this| {
                                this.child(div().mx(px(4.)).child("•")).child(
                                    div()
                                        .flex()
                                        .gap(px(8.))
                                        .font_family(env.mono.clone())
                                        .child(stat_column(
                                            format!("+{}", t3_diff::tree::format_count(additions)),
                                            colors.success,
                                        ))
                                        .child(stat_column(
                                            format!("-{}", t3_diff::tree::format_count(deletions)),
                                            colors.destructive,
                                        )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                Button::new(SharedString::from(format!("files-expand:{turn}")))
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Xs)
                                    .label(if all_expanded {
                                        "Collapse all"
                                    } else {
                                        "Expand all"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.timeline
                                            .changed_files_expanded
                                            .insert(turn.clone(), !all_expanded);
                                        let tree = this.timeline.tree_for(&turn);
                                        if let Some(tree) = tree {
                                            tree.update(cx, |tree, cx| {
                                                tree.set_all_expanded(Some(!all_expanded), cx)
                                            });
                                        }
                                        this.timeline
                                            .remeasure_row(&format!("changed-files:{turn}"));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "files-diff:{}",
                                    summary.turn_id
                                )))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Xs)
                                .label("View diff"),
                            ),
                    ),
            )
            .child(tree)
            .into_any_element()
    }
}

/// "Working for 2m 3s" (`WorkingTimelineRow`). The web shimmers the text; it is static here
/// (the reference captures render it without animation).
fn working_row(started_at: Option<&str>, env: &RowEnv) -> AnyElement {
    let label = match started_at {
        Some(started_at) => format!("Working for {}", format_working_timer(started_at, env.now)),
        None => "Working".to_owned(),
    };
    div()
        .py(px(2.))
        .pl(px(6.))
        .child(
            div()
                .pt(px(4.))
                .text_size(px(11.))
                .line_height(px(16.5))
                .text_color(env.colors.muted_foreground)
                .child(label),
        )
        .into_any_element()
}

/// A message timestamp with its long-form tooltip.
fn timestamp(iso: &str, key: &str, env: &RowEnv) -> AnyElement {
    div()
        .id(SharedString::from(format!("time:{key}")))
        .text_color(env.colors.muted_foreground)
        .child(format_short_timestamp(iso, env.timestamp_format))
        .tooltip_text(format_timestamp_tooltip(iso, env.timestamp_format))
        .into_any_element()
}

/// Ghost xs button holding a 12px icon, muted until hovered (copy, revert).
fn ghost_icon_button(id: impl Into<SharedString>, icon: IconName, colors: &Colors) -> Button {
    Button::new(id.into())
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Xs)
        .text_color(colors.muted_foreground)
        .child(small_icon(icon, px(12.)))
}

/// The icon of a work entry (`workEntryIconName`).
fn work_entry_icon(entry: &WorkLogEntry) -> IconName {
    if matches!(
        entry.source_activity_kind.as_str(),
        "user-input.requested" | "user-input.resolved"
    ) {
        return IconName::MessageCircle;
    }
    match entry.request_kind {
        Some(RequestKind::Command) => return IconName::Terminal,
        Some(RequestKind::FileRead) => return IconName::Eye,
        Some(RequestKind::FileChange) => return IconName::SquarePen,
        None => {}
    }
    if entry.item_type == Some(ToolItemType::CommandExecution) || entry.command.is_some() {
        return IconName::Terminal;
    }
    if entry.item_type == Some(ToolItemType::FileChange) || !entry.changed_files.is_empty() {
        return IconName::SquarePen;
    }
    match entry.item_type {
        Some(ToolItemType::WebSearch) => IconName::Globe,
        Some(ToolItemType::ImageView) => IconName::Eye,
        Some(ToolItemType::McpToolCall) => IconName::Wrench,
        Some(ToolItemType::DynamicToolCall | ToolItemType::CollabAgentToolCall) => IconName::Hammer,
        _ => match entry.tone {
            WorkTone::Error => IconName::CircleAlert,
            WorkTone::Thinking => IconName::Bot,
            WorkTone::Info => IconName::Check,
            WorkTone::Tool => IconName::Zap,
        },
    }
}

/// Command, else detail, else the first changed file (`workEntryPreview`).
fn work_entry_preview(entry: &WorkLogEntry, workspace_root: Option<&str>) -> Option<String> {
    if let Some(command) = &entry.command {
        return Some(command.clone());
    }
    if let Some(detail) = &entry.detail {
        return Some(detail.clone());
    }
    let first = entry.changed_files.first()?;
    let path = format_workspace_relative_path(first, workspace_root);
    Some(match entry.changed_files.len() {
        1 => path,
        count => format!("{path} +{} more", count - 1),
    })
}

/// The expanded body: MCP call JSON, raw command, detail, changed files
/// (`buildToolCallExpandedBody`).
fn expanded_body(entry: &WorkLogEntry, workspace_root: Option<&str>) -> Option<String> {
    let mut blocks: Vec<String> = Vec::new();
    if entry.item_type == Some(ToolItemType::McpToolCall)
        && let Some(data) = &entry.tool_data
    {
        let json = serde_json::to_string_pretty(data).unwrap_or_default();
        blocks.push(format!("MCP call\n{json}"));
    }
    let raw = entry
        .raw_command
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty() && entry.command.as_deref().map(str::trim) != Some(*raw));
    if let Some(raw) = raw {
        blocks.push(raw.to_owned());
    } else if let Some(command) = entry
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        blocks.push(command.to_owned());
    }
    if let Some(detail) = entry
        .detail
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        blocks.push(detail.to_owned());
    }
    if !entry.changed_files.is_empty() {
        blocks.push(
            entry
                .changed_files
                .iter()
                .map(|path| format_workspace_relative_path(path, workspace_root))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}
