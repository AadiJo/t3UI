/// A keybinding command (`ResolvedKeybindingRule.command`), typed so the dispatcher can `match`.
///
/// One variant per fork command (`contracts/keybindings.ts` `STATIC_KEYBINDING_COMMANDS`).
/// Anything else, including commands from a newer server, decodes into [`Command::Other`] and is
/// ignored by the dispatcher (the settings editor still shows it).
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
    PullRequestCopyNumber,
    DiffToggle,
    PreviewToggle,
    PreviewRefresh,
    PreviewFocusUrl,
    PreviewZoomIn,
    PreviewZoomOut,
    PreviewResetZoom,
    CommandPaletteToggle,
    FilePickerToggle,
    ProjectSearchToggle,
    UsageOpen,
    ThemeSelect,
    AppearanceCycle,
    ThemeEditorToggle,
    ComposerStash,
    ComposerHost,
    ComposerEffort,
    ComposerMode,
    ComposerWorkspace,
    ComposerPreviousWorktree,
    ComposerBranch,
    ChatNew,
    ChatNewLocal,
    ChatNewWithoutProject,
    EditorOpenFavorite,
    UsageCost,
    UsageTokens,
    UsageLimits,
    UsagePeriodDay,
    UsagePeriodWeek,
    UsagePeriodMonth,
    UsagePeriodQuarter,
    ModelPickerToggle,
    ModelPickerPreviousProvider,
    ModelPickerNextProvider,
    ThreadStop,
    ThreadSteerQueuedMessage,
    ThreadPrevious,
    ThreadNext,
    ThreadCopyReference,
    ThreadSettle,
    ThreadPin,
    ThreadUndo,
    /// `thread.jump.1` .. `thread.jump.9`.
    ThreadJump(u8),
    /// `modelPicker.jump.1` .. `modelPicker.jump.9`.
    ModelPickerJump(u8),
    /// `script.<id>.run`: runs a project script.
    ScriptRun(String),
    /// A command this client does not handle.
    Other(String),
}

/// Wire names of the fixed commands, in `STATIC_KEYBINDING_COMMANDS` order.
const NAMES: &[(&str, Command)] = &[
    ("sidebar.toggle", Command::SidebarToggle),
    ("navigation.back", Command::NavigationBack),
    ("navigation.forward", Command::NavigationForward),
    ("terminal.toggle", Command::TerminalToggle),
    ("terminal.split", Command::TerminalSplit),
    ("terminal.splitVertical", Command::TerminalSplitVertical),
    ("terminal.new", Command::TerminalNew),
    ("terminal.close", Command::TerminalClose),
    ("rightPanel.toggle", Command::RightPanelToggle),
    (
        "rightPanel.toggleMaximized",
        Command::RightPanelToggleMaximized,
    ),
    ("rightPanel.close", Command::RightPanelClose),
    ("pullRequest.copyNumber", Command::PullRequestCopyNumber),
    ("diff.toggle", Command::DiffToggle),
    ("preview.toggle", Command::PreviewToggle),
    ("preview.refresh", Command::PreviewRefresh),
    ("preview.focusUrl", Command::PreviewFocusUrl),
    ("preview.zoomIn", Command::PreviewZoomIn),
    ("preview.zoomOut", Command::PreviewZoomOut),
    ("preview.resetZoom", Command::PreviewResetZoom),
    ("commandPalette.toggle", Command::CommandPaletteToggle),
    ("filePicker.toggle", Command::FilePickerToggle),
    ("projectSearch.toggle", Command::ProjectSearchToggle),
    ("usage.open", Command::UsageOpen),
    ("theme.select", Command::ThemeSelect),
    ("appearance.cycle", Command::AppearanceCycle),
    ("themeEditor.toggle", Command::ThemeEditorToggle),
    ("composer.stash", Command::ComposerStash),
    ("composer.host", Command::ComposerHost),
    ("composer.effort", Command::ComposerEffort),
    ("composer.mode", Command::ComposerMode),
    ("composer.workspace", Command::ComposerWorkspace),
    (
        "composer.previousWorktree",
        Command::ComposerPreviousWorktree,
    ),
    ("composer.branch", Command::ComposerBranch),
    ("chat.new", Command::ChatNew),
    ("chat.newLocal", Command::ChatNewLocal),
    ("chat.newWithoutProject", Command::ChatNewWithoutProject),
    ("editor.openFavorite", Command::EditorOpenFavorite),
    ("usage.cost", Command::UsageCost),
    ("usage.tokens", Command::UsageTokens),
    ("usage.limits", Command::UsageLimits),
    ("usage.period.day", Command::UsagePeriodDay),
    ("usage.period.week", Command::UsagePeriodWeek),
    ("usage.period.month", Command::UsagePeriodMonth),
    ("usage.period.quarter", Command::UsagePeriodQuarter),
    ("modelPicker.toggle", Command::ModelPickerToggle),
    (
        "modelPicker.previousProvider",
        Command::ModelPickerPreviousProvider,
    ),
    ("modelPicker.nextProvider", Command::ModelPickerNextProvider),
    ("thread.stop", Command::ThreadStop),
    (
        "thread.steerQueuedMessage",
        Command::ThreadSteerQueuedMessage,
    ),
    ("thread.previous", Command::ThreadPrevious),
    ("thread.next", Command::ThreadNext),
    ("thread.copyReference", Command::ThreadCopyReference),
    ("thread.settle", Command::ThreadSettle),
    ("thread.pin", Command::ThreadPin),
    ("thread.undo", Command::ThreadUndo),
];

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
        NAMES.iter().find(|(name, _)| *name == value).map_or_else(
            || Self::Other(value.to_owned()),
            |(_, command)| command.clone(),
        )
    }

    /// The wire string.
    pub fn as_str(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow::{Borrowed, Owned};
        match self {
            Self::ThreadJump(index) => Owned(format!("thread.jump.{index}")),
            Self::ModelPickerJump(index) => Owned(format!("modelPicker.jump.{index}")),
            Self::ScriptRun(id) => Owned(format!("script.{id}.run")),
            Self::Other(value) => Owned(value.clone()),
            fixed => Borrowed(
                NAMES
                    .iter()
                    .find(|(_, command)| command == fixed)
                    .map(|(name, _)| *name)
                    .expect("every fixed command has a wire name"),
            ),
        }
    }
}
