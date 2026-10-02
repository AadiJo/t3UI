//! The Open-in picker (panels.md 4.4, `chat/OpenInPicker.tsx`, `editorPreferences.ts`): opens
//! the thread's cwd in the preferred editor, with a menu of every editor the server found.

use gpui_kit::{
    AnyElement, App, AppContext as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    div, prelude::FluentBuilder as _, px,
};
use t3_protocol::{
    EnvironmentId, methods::ShellOpenInEditor, projects::LaunchEditorInput, server::EditorId,
};
use t3_ui::{
    ActiveColors as _, Align, Button, ButtonSize, ButtonVariant, DropdownMenu, Icon, IconName,
    Logo, MenuItem, MenuShortcut, logo,
};

use super::{
    prefs::HeaderPrefs,
    widgets::{close_menu, group_separator, join_left, join_right, menu_row},
};
use crate::{
    keybindings::shortcut_label,
    state::AppState,
    terminal_drawer::{environment_client, rpc_error_text},
};
use t3_logic::keybindings::Command;

/// How an editor's icon is drawn.
#[derive(Clone, Copy, Debug)]
pub enum EditorIcon {
    Logo(Logo),
    Lucide(IconName),
}

/// One row of the picker.
#[derive(Clone, Debug)]
pub struct EditorOption {
    pub id: EditorId,
    pub label: &'static str,
    pub icon: EditorIcon,
}

macro_rules! editors {
    ($( $id:ident, $label:literal, $icon:expr; )*) => {
        /// Every editor in the picker's order (`OpenInPicker.tsx:38-152`); the server's
        /// `availableEditors` filters it.
        pub static EDITORS: &[EditorOption] = &[
            $( EditorOption { id: EditorId::$id, label: $label, icon: $icon }, )*
        ];
    };
}

editors! {
    Cursor, "Cursor", EditorIcon::Logo(Logo::CursorIcon);
    Trae, "Trae", EditorIcon::Logo(Logo::TraeIcon);
    Kiro, "Kiro", EditorIcon::Logo(Logo::KiroIcon);
    Vscode, "VS Code", EditorIcon::Logo(Logo::VisualStudioCode);
    VscodeInsiders, "VS Code Insiders", EditorIcon::Logo(Logo::VisualStudioCodeInsiders);
    Vscodium, "VSCodium", EditorIcon::Logo(Logo::VSCodium);
    Zed, "Zed", EditorIcon::Logo(Logo::Zed);
    Antigravity, "Antigravity", EditorIcon::Logo(Logo::AntigravityIcon);
    Idea, "IntelliJ IDEA", EditorIcon::Logo(Logo::IntelliJIdeaIcon);
    Aqua, "Aqua", EditorIcon::Logo(Logo::AquaIcon);
    Clion, "CLion", EditorIcon::Logo(Logo::CLionIcon);
    Datagrip, "DataGrip", EditorIcon::Logo(Logo::DataGripIcon);
    Dataspell, "DataSpell", EditorIcon::Logo(Logo::DataSpellIcon);
    Goland, "GoLand", EditorIcon::Logo(Logo::GoLandIcon);
    Phpstorm, "PhpStorm", EditorIcon::Logo(Logo::PhpStormIcon);
    Pycharm, "PyCharm", EditorIcon::Logo(Logo::PyCharmIcon);
    Rider, "Rider", EditorIcon::Logo(Logo::RiderIcon);
    Rubymine, "RubyMine", EditorIcon::Logo(Logo::RubyMineIcon);
    Rustrover, "RustRover", EditorIcon::Logo(Logo::RustRoverIcon);
    Webstorm, "WebStorm", EditorIcon::Logo(Logo::WebStormIcon);
    FileManager, "Files", EditorIcon::Lucide(IconName::FolderClosed);
}

/// The file manager's name on this platform.
fn label(option: &EditorOption) -> &'static str {
    if option.id == EditorId::FileManager {
        if cfg!(target_os = "macos") {
            "Finder"
        } else if cfg!(target_os = "windows") {
            "Explorer"
        } else {
            "Files"
        }
    } else {
        option.label
    }
}

/// Editors the environment's server can launch, in picker order.
pub fn available_editors(environment_id: &EnvironmentId, cx: &App) -> Vec<EditorId> {
    AppState::global(cx)
        .read(cx)
        .environment(environment_id, cx)
        .and_then(|environment| environment.read(cx).config().cloned())
        .map(|config| config.available_editors.clone())
        .unwrap_or_default()
}

/// `shell.openInEditor`.
pub fn open_in_editor(
    environment_id: &EnvironmentId,
    path: String,
    editor: EditorId,
    cx: &App,
) -> Task<Result<(), String>> {
    let Some(client) = environment_client(environment_id, cx) else {
        return Task::ready(Err("Environment is not connected.".into()));
    };
    let input = LaunchEditorInput {
        cwd: path,
        editor,
        reveal: None,
    };
    cx.background_spawn(async move {
        client
            .request::<ShellOpenInEditor>(&input)
            .await
            .map_err(|error| rpc_error_text(&error))
    })
}

/// Opens `path` in the preferred editor and remembers it (`useOpenInPreferredEditor`), for
/// terminal path links and the commit dialog's file list.
pub fn open_in_preferred_editor(
    environment_id: &EnvironmentId,
    path: String,
    cx: &mut App,
) -> Task<Result<(), String>> {
    let available = available_editors(environment_id, cx);
    let prefs = HeaderPrefs::global(cx);
    let Some(editor) = prefs.read(cx).preferred_editor(&available) else {
        return Task::ready(Err(format!(
            "No available editor can open {path} in environment {environment_id}."
        )));
    };
    prefs.update(cx, |prefs, cx| prefs.set_last_editor(editor.clone(), cx));
    open_in_editor(environment_id, path, editor, cx)
}

fn editor_icon(icon: EditorIcon, size: Pixels, cx: &App) -> AnyElement {
    let colors = cx.colors();
    match icon {
        EditorIcon::Logo(logo_name) => logo(logo_name, colors.is_dark, size).into_any_element(),
        EditorIcon::Lucide(name) => Icon::new(name)
            .size(size)
            .color(colors.muted_foreground)
            .into_any_element(),
    }
}

/// What the picker opens.
#[derive(Clone, Debug)]
pub struct OpenInTarget {
    pub environment_id: EnvironmentId,
    pub cwd: Option<String>,
}

/// The joined Open / editors group. `labels` shows "Open" (header at least 768px wide).
pub fn render_open_in(
    target: &OpenInTarget,
    labels: bool,
    prefs: &Entity<HeaderPrefs>,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let colors = cx.colors();
    let available = available_editors(&target.environment_id, cx);
    let preferred = prefs.read(cx).preferred_editor(&available);
    let options: Vec<EditorOption> = EDITORS
        .iter()
        .filter(|option| available.contains(&option.id))
        .cloned()
        .collect();
    let primary = preferred
        .as_ref()
        .and_then(|id| options.iter().find(|option| option.id == *id))
        .cloned();
    let favorite_shortcut = shortcut_label(&Command::EditorOpenFavorite, cx);

    let open_target = target.clone();
    let open_prefs = prefs.clone();
    let open_editor = preferred.clone();
    let mut open_button = Button::new("open-in-preferred")
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Xs)
        .disabled(preferred.is_none() || target.cwd.is_none())
        .on_click(move |_, _, cx| {
            if let (Some(editor), Some(cwd)) = (open_editor.clone(), open_target.cwd.clone()) {
                open_prefs.update(cx, |prefs, cx| prefs.set_last_editor(editor.clone(), cx));
                open_in_editor(&open_target.environment_id, cwd, editor, cx).detach();
            }
        });
    if let Some(primary) = &primary {
        // `[&_svg]:-mx-0.5`, 80% like every button svg.
        open_button = open_button.child(
            div()
                .relative()
                .flex_none()
                .w(px(10.))
                .h(px(14.))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(-2.))
                        .opacity(0.8)
                        .child(editor_icon(primary.icon, px(14.), cx)),
                ),
        );
    }
    if labels {
        open_button = open_button.child(div().ml(px(2.)).child("Open"));
    }

    let menu_target = target.clone();
    let menu_prefs = prefs.clone();
    let menu = DropdownMenu::new("open-in-menu")
        .align(Align::End)
        .trigger(move |open| {
            join_right(
                Button::new("open-in-options")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::IconXs)
                    .pressed(open)
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(px(16.))
                            .color(colors.muted_foreground)
                            .opacity(0.8),
                    ),
            )
            .into_any_element()
        })
        .items(move |_, cx| {
            if options.is_empty() {
                return vec![
                    MenuItem::new("open-in-none", "No installed editors found")
                        .disabled(true)
                        .into_any_element(),
                ];
            }
            options
                .iter()
                .map(|option| {
                    let target = menu_target.clone();
                    let prefs = menu_prefs.clone();
                    let editor = option.id.clone();
                    let is_preferred = preferred.as_ref() == Some(&option.id);
                    menu_row(
                        SharedString::from(format!("open-in-{}", option.id)),
                        false,
                        cx,
                    )
                    .child(
                        div()
                            .flex_none()
                            .opacity(0.8)
                            .child(editor_icon(option.icon, px(16.), cx)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label(option)))
                    .when_some(
                        favorite_shortcut.clone().filter(|_| is_preferred),
                        |this, shortcut| this.child(MenuShortcut::new(shortcut)),
                    )
                    .on_click(move |_, window, cx| {
                        if let Some(cwd) = target.cwd.clone() {
                            prefs.update(cx, |prefs, cx| prefs.set_last_editor(editor.clone(), cx));
                            open_in_editor(&target.environment_id, cwd, editor.clone(), cx)
                                .detach();
                        }
                        close_menu(window, cx);
                    })
                    .into_any_element()
                })
                .collect()
        });

    div()
        .flex()
        .items_center()
        .child(join_left(open_button))
        .when(labels, |this| this.child(group_separator(cx)))
        .child(menu)
}

/// `editor.openFavorite` (⌘O): opens the cwd in the preferred editor.
pub fn open_favorite(target: &OpenInTarget, cx: &mut App) {
    let Some(cwd) = target.cwd.clone() else {
        return;
    };
    let available = available_editors(&target.environment_id, cx);
    if let Some(editor) = HeaderPrefs::global(cx).read(cx).preferred_editor(&available) {
        open_in_editor(&target.environment_id, cwd, editor, cx).detach();
    }
}
