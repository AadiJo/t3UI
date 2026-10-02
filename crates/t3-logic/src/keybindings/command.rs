/// A keybinding command (`ResolvedKeybindingRule.command`), typed so the dispatcher can `match`.
///
/// Covers the fork's commands plus the upstream ones the native client may handle. Anything else,
/// including commands from a newer server, decodes into [`Command::Other`] and is ignored by the
/// dispatcher (the settings editor still shows it).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    SidebarToggle,
    NavigationBack,
    NavigationForward,
    TerminalToggle,
    TerminalSplit,
    TerminalSplitVertical,
    TerminalNew,
    TerminalClose,
    RightPanelToggle,
    RightPanelToggleMaximized,
    RightPanelClose,
    DiffToggle,
    PreviewToggle,
    PreviewRefresh,
    PreviewFocusUrl,
    PreviewZoomIn,
    PreviewZoomOut,
    PreviewResetZoom,
    CommandPaletteToggle,
    ChatNew,
    ChatNewLocal,
    ModelPickerToggle,
    EditorOpenFavorite,
    ThreadPrevious,
    ThreadNext,
    /// `thread.jump.1` .. `thread.jump.9`.
    ThreadJump(u8),
    /// `modelPicker.jump.1` .. `modelPicker.jump.9`.
    ModelPickerJump(u8),
    /// `script.<id>.run`: runs a project script.
    ScriptRun(String),
    /// A command this client does not handle.
    Other(String),
}

impl Command {
    /// Parses the wire string.
    pub fn parse(value: &str) -> Self {
        let jump = |prefix: &str| {
            value
                .strip_prefix(prefix)
                .and_then(|digit| digit.parse::<u8>().ok())
                .filter(|index| (1..=9).contains(index))
        };
        if let Some(index) = jump("thread.jump.") {
            return Self::ThreadJump(index);
        }
        if let Some(index) = jump("modelPicker.jump.") {
            return Self::ModelPickerJump(index);
        }
        if let Some(id) = value
            .strip_prefix("script.")
            .and_then(|rest| rest.strip_suffix(".run"))
            .filter(|id| !id.is_empty())
        {
            return Self::ScriptRun(id.to_owned());
        }
        match value {
            "sidebar.toggle" => Self::SidebarToggle,
            "navigation.back" => Self::NavigationBack,
            "navigation.forward" => Self::NavigationForward,
            "terminal.toggle" => Self::TerminalToggle,
            "terminal.split" => Self::TerminalSplit,
            "terminal.splitVertical" => Self::TerminalSplitVertical,
            "terminal.new" => Self::TerminalNew,
            "terminal.close" => Self::TerminalClose,
            "rightPanel.toggle" => Self::RightPanelToggle,
            "rightPanel.toggleMaximized" => Self::RightPanelToggleMaximized,
            "rightPanel.close" => Self::RightPanelClose,
            "diff.toggle" => Self::DiffToggle,
            "preview.toggle" => Self::PreviewToggle,
            "preview.refresh" => Self::PreviewRefresh,
            "preview.focusUrl" => Self::PreviewFocusUrl,
            "preview.zoomIn" => Self::PreviewZoomIn,
            "preview.zoomOut" => Self::PreviewZoomOut,
            "preview.resetZoom" => Self::PreviewResetZoom,
            "commandPalette.toggle" => Self::CommandPaletteToggle,
            "chat.new" => Self::ChatNew,
            "chat.newLocal" => Self::ChatNewLocal,
            "modelPicker.toggle" => Self::ModelPickerToggle,
            "editor.openFavorite" => Self::EditorOpenFavorite,
            "thread.previous" => Self::ThreadPrevious,
            "thread.next" => Self::ThreadNext,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The wire string.
    pub fn as_str(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow::{Borrowed, Owned};
        match self {
            Self::SidebarToggle => Borrowed("sidebar.toggle"),
            Self::NavigationBack => Borrowed("navigation.back"),
            Self::NavigationForward => Borrowed("navigation.forward"),
            Self::TerminalToggle => Borrowed("terminal.toggle"),
            Self::TerminalSplit => Borrowed("terminal.split"),
            Self::TerminalSplitVertical => Borrowed("terminal.splitVertical"),
            Self::TerminalNew => Borrowed("terminal.new"),
            Self::TerminalClose => Borrowed("terminal.close"),
            Self::RightPanelToggle => Borrowed("rightPanel.toggle"),
            Self::RightPanelToggleMaximized => Borrowed("rightPanel.toggleMaximized"),
            Self::RightPanelClose => Borrowed("rightPanel.close"),
            Self::DiffToggle => Borrowed("diff.toggle"),
            Self::PreviewToggle => Borrowed("preview.toggle"),
            Self::PreviewRefresh => Borrowed("preview.refresh"),
            Self::PreviewFocusUrl => Borrowed("preview.focusUrl"),
            Self::PreviewZoomIn => Borrowed("preview.zoomIn"),
            Self::PreviewZoomOut => Borrowed("preview.zoomOut"),
            Self::PreviewResetZoom => Borrowed("preview.resetZoom"),
            Self::CommandPaletteToggle => Borrowed("commandPalette.toggle"),
            Self::ChatNew => Borrowed("chat.new"),
            Self::ChatNewLocal => Borrowed("chat.newLocal"),
            Self::ModelPickerToggle => Borrowed("modelPicker.toggle"),
            Self::EditorOpenFavorite => Borrowed("editor.openFavorite"),
            Self::ThreadPrevious => Borrowed("thread.previous"),
            Self::ThreadNext => Borrowed("thread.next"),
            Self::ThreadJump(index) => Owned(format!("thread.jump.{index}")),
            Self::ModelPickerJump(index) => Owned(format!("modelPicker.jump.{index}")),
            Self::ScriptRun(id) => Owned(format!("script.{id}.run")),
            Self::Other(value) => Owned(value.clone()),
        }
    }
}
