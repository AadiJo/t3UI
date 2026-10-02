//! The 52px chat header (`ChatHeader.tsx`, `PanelLayoutControls.tsx`, spec 2.1): thread title
//! and project on the left, the header actions slot on the right, and the terminal / right
//! panel toggles pinned 12px from the window edge.

use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, prelude::FluentBuilder as _, px, relative,
};
use t3_logic::keybindings::Command;
use t3_ui::{ActiveColors as _, IconName, TooltipExt as _, tokens::layout};

use super::{ChatTarget, ChatView, controls};
use crate::{
    chrome::drag_region, keybindings::shortcut_label, workspace::collapsed_titlebar_inset,
};

/// `--workspace-chat-header-control-reserve`: room for the two panel toggles (28 * 2 + 4).
const CONTROL_RESERVE: f32 = 60.;

impl ChatView {
    /// The thread title and its project name.
    fn titles(&self, cx: &gpui_kit::App) -> (SharedString, Option<SharedString>) {
        let environment = self.environment.as_ref().map(|e| e.read(cx));
        match &self.target {
            ChatTarget::Thread(_) => {
                let thread = self.orchestration_thread();
                let title = thread.map_or_else(String::new, |t| t.title.clone());
                let project = thread
                    .zip(environment)
                    .and_then(|(thread, env)| env.project(&thread.project_id))
                    .map(|project| SharedString::from(project.title.clone()));
                (title.into(), project)
            }
            ChatTarget::Draft { project, .. } => {
                let project = project
                    .as_ref()
                    .zip(environment)
                    .and_then(|(project, env)| env.project(&project.project_id))
                    .map(|project| SharedString::from(project.title.clone()));
                ("New thread".into(), project)
            }
        }
    }

    /// The workspace root for path display: the thread's worktree, else the project root.
    pub(super) fn workspace_root(&self, cx: &gpui_kit::App) -> Option<String> {
        let thread = self.orchestration_thread()?;
        if let Some(worktree) = &thread.worktree_path {
            return Some(worktree.clone());
        }
        let environment = self.environment.as_ref()?.read(cx);
        Some(
            environment
                .project(&thread.project_id)?
                .workspace_root
                .clone(),
        )
    }

    pub(super) fn render_header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let (title, project) = self.titles(cx);
        let inset = collapsed_titlebar_inset(cx);
        let has_project = project.is_some();
        let actions = match &self.header_actions {
            Some(view) => view.clone().into_any_element(),
            None => controls::header_actions_placeholder(cx),
        };
        let tooltip = |command: &Command, label: &str, unavailable: &str| {
            if !has_project {
                return unavailable.to_owned();
            }
            match shortcut_label(command, cx) {
                Some(shortcut) => format!("{label} ({shortcut})"),
                None => label.to_owned(),
            }
        };
        let terminal_tooltip = tooltip(
            &Command::TerminalToggle,
            "Toggle terminal drawer",
            "Terminal drawer is unavailable",
        );
        let panel_tooltip = tooltip(
            &Command::RightPanelToggle,
            "Toggle right panel",
            "Right panel is unavailable",
        );
        // The terminal drawer and right panel owners handle these commands.
        let dispatch = |command: Command| {
            let app_state = self.app_state.clone();
            move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut gpui_kit::App| {
                app_state.update(cx, |state, cx| state.dispatch_command(command.clone(), cx));
            }
        };
        let on_terminal = dispatch(Command::TerminalToggle);
        let on_panel = dispatch(Command::RightPanelToggle);

        drag_region("chat-header", window, cx)
            .relative()
            .h(layout::TOPBAR_HEIGHT)
            .flex_shrink_0()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(colors.border)
            .px(px(20.))
            .when_some(inset, |this, inset| this.pl(inset))
            .child(
                div()
                    .flex()
                    .h_full()
                    .min_w_0()
                    .flex_1()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .min_w_0()
                            .flex_1()
                            .items_center()
                            .overflow_hidden()
                            .child(
                                div()
                                    .id("chat-title")
                                    .flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_baseline()
                                    .gap(px(8.))
                                    .tooltip_text(title.clone())
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(14.))
                                            .line_height(px(14.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child(title),
                                    )
                                    .when_some(project, |this, project| {
                                        this.child(
                                            div()
                                                .max_w(relative(0.4))
                                                .flex_shrink(1.)
                                                .truncate()
                                                .text_size(px(12.))
                                                .line_height(px(12.))
                                                .text_color(colors.muted_foreground)
                                                .child(project),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .flex_shrink_0()
                            .items_center()
                            .justify_end()
                            .gap(px(12.))
                            .pr(px(CONTROL_RESERVE))
                            .child(actions),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(12.))
                    .h(layout::TOPBAR_HEIGHT)
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        controls::panel_toggle(
                            "toggle-terminal",
                            IconName::PanelBottomOpen,
                            false,
                            terminal_tooltip,
                        )
                        .disabled(!has_project)
                        .on_click(on_terminal),
                    )
                    .child(
                        controls::panel_toggle(
                            "toggle-right-panel",
                            IconName::PanelRightOpen,
                            false,
                            panel_tooltip,
                        )
                        .disabled(!has_project)
                        .on_click(on_panel),
                    ),
            )
    }
}
