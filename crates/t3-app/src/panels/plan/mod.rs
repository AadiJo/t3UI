//! The plan surface (spec 4.1, `PlanSidebar.tsx` with `mode="embedded"`): the active plan's
//! explanation and steps, the proposed plan behind a disclosure, and a menu to copy, download
//! or save the plan markdown.
//!
//! - [`logic`]: what to show, derived from the thread (`session-logic.ts`, `proposedPlan.ts`).
//! - [`render`]: element builders for labels, step rows and the empty state.

mod logic;
mod render;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use gpui_kit::{
    AnyElement, AppContext as _, ClipboardItem, Context, ElementId, Entity, FontFeatures,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div,
    prelude::FluentBuilder as _, px,
};
use t3_logic::{
    ThreadRef,
    timeline::{
        format_timestamp,
        plan::{displayed_plan_markdown, plan_title},
    },
};
use t3_markdown::{Markdown, MarkdownEvent, MarkdownOptions};
use t3_protocol::{ThreadId, methods::ProjectsWriteFile, projects::ProjectWriteFileInput};
use t3_ui::{
    ActiveColors as _, Align, Badge, BadgeSize, BadgeVariant, Button, ButtonSize, ButtonVariant,
    DropdownMenu, Icon, IconName, MenuItem, ScrollArea,
};

use self::logic::{PlanView, normalize_plan_markdown_for_export, plan_markdown_filename};
use super::{context::PanelContext, thread_detail::ThreadDetail};
use crate::toast::{self, Toast};

/// How long "Copied!" replaces "Copy to clipboard" (`useCopyToClipboard`'s timeout).
const COPIED_FOR: Duration = Duration::from_secs(2);

/// The `plan` tab: the thread's active plan and its proposed plan. Re-derives what it shows
/// whenever the thread detail changes.
pub struct PlanSurface {
    context: PanelContext,
    detail: Entity<ThreadDetail>,
    /// The thread whose plan a running implementation turn follows, when it is another thread.
    source: Option<SourceThread>,
    view: PlanView,
    /// The proposed plan's body, rendered while the disclosure is open.
    markdown: Option<(Entity<Markdown>, Subscription)>,
    /// The proposed plan disclosure; collapsed by default.
    expanded: bool,
    saving: bool,
    /// Set while "Copied!" shows; the task clears it.
    copied: Option<Task<()>>,
    _observe: Subscription,
}

struct SourceThread {
    thread_id: ThreadId,
    detail: Entity<ThreadDetail>,
    _observe: Subscription,
}

impl PlanSurface {
    pub fn new(
        context: PanelContext,
        detail: Entity<ThreadDetail>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&detail, |this, _, cx| this.sync(cx));
        let mut this = Self {
            context,
            detail,
            source: None,
            view: PlanView::default(),
            markdown: None,
            expanded: false,
            saving: false,
            copied: None,
            _observe: observe,
        };
        this.sync(cx);
        this
    }

    /// Opens or closes the proposed plan disclosure.
    pub fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        self.expanded = expanded;
        cx.notify();
    }

    /// Re-derives the view after this thread (or its plan's source thread) changed.
    fn sync(&mut self, cx: &mut Context<Self>) {
        self.follow_source(cx);
        let source = self
            .source
            .as_ref()
            .and_then(|source| source.detail.read(cx).thread());
        self.view = self
            .detail
            .read(cx)
            .thread()
            .map(|thread| {
                PlanView::derive(
                    thread,
                    source.map(|source| source.proposed_plans.as_slice()),
                )
            })
            .unwrap_or_default();
        self.sync_markdown(cx);
        cx.notify();
    }

    /// Subscribes to the thread named by `latestTurn.sourceProposedPlan` while it is another
    /// thread, like the fork's `useThreadProposedPlans(sourcePlanThreadRef)`.
    fn follow_source(&mut self, cx: &mut Context<Self>) {
        let wanted = self.detail.read(cx).thread().and_then(|thread| {
            let source = thread.latest_turn.as_ref()?.source_proposed_plan.as_ref()?;
            (source.thread_id != thread.id).then(|| source.thread_id.clone())
        });
        if self.source.as_ref().map(|source| &source.thread_id) == wanted.as_ref() {
            return;
        }
        self.source = wanted.map(|thread_id| {
            let context = PanelContext::new(
                self.context.app_state.clone(),
                ThreadRef::new(
                    self.context.thread.environment_id.clone(),
                    thread_id.clone(),
                ),
            );
            let detail = cx.new(|cx| ThreadDetail::live(&context, cx));
            let observe = cx.observe(&detail, |this, _, cx| this.sync(cx));
            SourceThread {
                thread_id,
                detail,
                _observe: observe,
            }
        });
    }

    /// Keeps the markdown view on the proposed plan's displayed body.
    fn sync_markdown(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.view.proposed_plan.clone() else {
            self.markdown = None;
            return;
        };
        let text = displayed_plan_markdown(&plan.plan_markdown);
        if let Some((markdown, _)) = &self.markdown {
            markdown.update(cx, |markdown, cx| markdown.set_text(text, false, cx));
            return;
        }
        let options = MarkdownOptions {
            cwd: self.context.cwd(cx),
            word_wrap: self.context.app_state.read(cx).settings().word_wrap,
            ..MarkdownOptions::default()
        };
        let markdown = cx.new(|cx| Markdown::new(text, options, cx));
        let subscription = cx.subscribe(&markdown, Self::on_markdown_event);
        self.markdown = Some((markdown, subscription));
    }

    /// Links in the plan: URLs open in the browser, workspace files in a file tab.
    fn on_markdown_event(
        &mut self,
        _: Entity<Markdown>,
        event: &MarkdownEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            MarkdownEvent::OpenUrl(url) => cx.open_url(url),
            MarkdownEvent::OpenFile(link) => match &link.workspace_relative_path {
                Some(path) => {
                    let thread = self.context.thread.clone();
                    let line = link.line;
                    self.context
                        .panels(cx)
                        .update(cx, |panels, cx| panels.open_file(&thread, path, line, cx));
                }
                None => cx.open_with_system(Path::new(&link.file_path)),
            },
        }
    }

    fn copy_plan(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = &self.view.proposed_plan else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(plan.plan_markdown.clone()));
        self.copied = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// "Download as markdown": a native save dialog in Downloads, named after the plan title.
    fn download_plan(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = &self.view.proposed_plan else {
            return;
        };
        let filename = plan_markdown_filename(&plan.plan_markdown);
        let contents = normalize_plan_markdown_for_export(&plan.plan_markdown);
        let directory = std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Downloads"))
            .unwrap_or_default();
        let chosen = cx.prompt_for_new_path(&directory, Some(&filename));
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let written = cx
                .background_executor()
                .spawn(async move { std::fs::write(&path, contents) })
                .await;
            if let Err(error) = written {
                tracing::error!("could not write the plan: {error}");
            }
        })
        .detach();
    }

    /// "Save to workspace": `projects.writeFile` into the workspace root, then a toast.
    fn save_plan(&mut self, cx: &mut Context<Self>) {
        let (Some(plan), Some(cwd)) = (&self.view.proposed_plan, self.context.cwd(cx)) else {
            return;
        };
        let input = ProjectWriteFileInput {
            cwd,
            relative_path: plan_markdown_filename(&plan.plan_markdown),
            contents: normalize_plan_markdown_for_export(&plan.plan_markdown),
        };
        let request = self.context.request::<ProjectsWriteFile>(input, cx);
        self.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| {
                this.saving = false;
                cx.notify();
                let toast = match result {
                    Ok(saved) => Toast::success("Plan saved").description(saved.relative_path),
                    Err(error) => Toast::error("Could not save plan")
                        .description(error.to_string())
                        .stacked(),
                };
                toast::show(toast, cx);
            })
            .ok();
        })
        .detach();
    }

    /// 48px header: the Plan/Tasks badge, the plan's time, and the actions menu.
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let format = self.context.app_state.read(cx).settings().timestamp_format;
        let timestamp = self
            .view
            .active_plan
            .as_ref()
            .map(|plan| format_timestamp(&plan.created_at, format));
        let actions = self
            .view
            .proposed_plan
            .is_some()
            .then(|| self.render_actions(cx));
        div()
            .flex()
            .flex_none()
            .h(px(48.))
            .items_center()
            .justify_between()
            .px(px(12.))
            .border_b_1()
            .border_color(colors.border_60)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        Badge::new(self.view.label().to_uppercase())
                            .variant(BadgeVariant::Info)
                            .size(BadgeSize::Sm)
                            .rounded(px(8.))
                            .px(px(6.))
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .when_some(timestamp, |row, timestamp| {
                        row.child(
                            div()
                                .text_size(px(11.))
                                .line_height(px(16.5))
                                .text_color(colors.muted_foreground_60)
                                .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
                                .child(timestamp),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .when_some(actions, |row, actions| row.child(actions)),
            )
    }

    /// The `Plan actions` ellipsis menu, end-aligned under its trigger.
    fn render_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity().downgrade();
        let copied = self.copied.is_some();
        let can_save = !self.saving && self.context.cwd(cx).is_some();
        DropdownMenu::new("plan-actions")
            .align(Align::End)
            .trigger(|open| {
                Button::new("plan-actions-trigger")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .icon(IconName::Ellipsis)
                    .pressed(open)
                    .into_any_element()
            })
            .items(move |_, _| {
                let item = |id: &'static str,
                            label: &'static str,
                            action: fn(&mut Self, &mut Context<Self>)| {
                    let this = this.clone();
                    MenuItem::new(id, label).on_click(move |_, _, cx| {
                        this.update(cx, action).ok();
                    })
                };
                let copy_label = if copied {
                    "Copied!"
                } else {
                    "Copy to clipboard"
                };
                vec![
                    item("plan-copy", copy_label, Self::copy_plan).into_any_element(),
                    item("plan-download", "Download as markdown", Self::download_plan)
                        .into_any_element(),
                    item("plan-save", "Save to workspace", Self::save_plan)
                        .disabled(!can_save)
                        .into_any_element(),
                ]
            })
            .into_any_element()
    }

    /// The proposed plan: a disclosure titled by the plan's first heading, and when open, a
    /// card with the plan body.
    fn render_proposed_plan(&self, markdown: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let muted = colors.muted_foreground;
        let expanded = self.expanded;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .id("plan-disclosure")
                    .group("plan-disclosure")
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(px(6.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let expanded = !this.expanded;
                        this.set_expanded(expanded, cx);
                    }))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(12.))
                        .color(muted.opacity(0.4)),
                    )
                    .child(
                        render::section_label(plan_title(markdown).unwrap_or("Full Plan"))
                            .flex_1()
                            .min_w_0()
                            .text_color(muted.opacity(0.4))
                            .group_hover("plan-disclosure", move |style| {
                                style.text_color(muted.opacity(0.6))
                            }),
                    ),
            )
            .when(expanded, |section| {
                section.child(
                    div()
                        .rounded(px(10.))
                        .border_1()
                        .border_color(colors.border.opacity(0.5))
                        .bg(colors.background.opacity(0.5))
                        .p(px(12.))
                        .when_some(self.markdown.as_ref(), |card, (markdown, _)| {
                            card.child(markdown.clone())
                        }),
                )
            })
    }
}

impl Render for PlanSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let mut sections: Vec<AnyElement> = Vec::new();
        let active_plan = self.view.active_plan.clone();
        if let Some(explanation) = active_plan
            .as_ref()
            .and_then(|plan| plan.explanation.as_deref())
            .filter(|explanation| !explanation.is_empty())
        {
            sections.push(
                div()
                    .text_size(px(13.))
                    .line_height(px(13. * 1.625))
                    .text_color(colors.muted_foreground_80)
                    .child(SharedString::from(explanation.to_owned()))
                    .into_any_element(),
            );
        }
        if let Some(plan) = active_plan.as_ref().filter(|plan| !plan.steps.is_empty()) {
            let rows: Vec<AnyElement> = plan
                .steps
                .iter()
                .map(|step| {
                    let id =
                        ElementId::from(SharedString::from(format!("plan-step:{}", step.step)));
                    render::step_row(id, step, colors, window, cx).into_any_element()
                })
                .collect();
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        render::section_label("Steps")
                            .mb(px(8.))
                            .text_color(colors.muted_foreground.opacity(0.4)),
                    )
                    .child(div().flex().flex_col().gap(px(4.)).children(rows))
                    .into_any_element(),
            );
        }
        if let Some(plan) = self.view.proposed_plan.clone() {
            sections.push(
                self.render_proposed_plan(&plan.plan_markdown, cx)
                    .into_any_element(),
            );
        }
        if active_plan.is_none() && self.view.proposed_plan.is_none() {
            sections.push(render::empty_state(colors).into_any_element());
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(colors.card.opacity(0.5))
            .child(self.render_header(cx))
            .child(
                div().flex_1().min_h_0().child(
                    ScrollArea::new("plan-scroll").child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .p(px(12.))
                            .children(sections),
                    ),
                ),
            )
    }
}
