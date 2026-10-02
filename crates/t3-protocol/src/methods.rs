//! Every RPC the client calls, as typed method descriptors (protocol.md 4.1).
//!
//! ```ignore
//! let config = env.request::<ServerGetConfig>(&Empty {}).await?;
//! let mut status = env.subscribe::<SubscribeVcsStatus>(&VcsCwdInput { cwd })?;
//! ```
//!
//! All methods fail with [`ServerError`] (the union always includes
//! `EnvironmentAuthorizationError`).

use serde::{Serialize, de::IgnoredAny};

use crate::{
    commands::ClientCommand,
    errors::ServerError,
    orchestration::{
        DispatchResult, GetFullThreadDiffInput, GetTurnDiffInput, OrchestrationShellSnapshot,
        SearchThreadsInput, SearchThreadsResult, ShellStreamItem, SubscribeShellInput,
        SubscribeThreadInput, ThreadStreamItem, ThreadTurnDiff,
    },
    projects::{
        AssetCreateUrlInput, AssetCreateUrlResult, AttachmentCreateUploadUrlInput,
        AttachmentCreateUploadUrlResult, AttachmentDeleteInput, FilesystemBrowseInput,
        FilesystemBrowseResult, LaunchEditorInput, ProjectEntries, ProjectListEntriesInput,
        ProjectReadFileInput, ProjectReadFileResult, ProjectSearchContentsInput,
        ProjectSearchContentsResult, ProjectSearchEntriesInput, ProjectWriteFileInput,
        ProjectWriteFileResult,
    },
    server::{
        AuthAccessStreamEvent, KeybindingsUpdated, ProvidersUpdated, RefreshProvidersInput,
        RemoveKeybindingInput, ServerConfig, ServerConfigStreamEvent, ServerLifecycleStreamEvent,
        ServerSettings, SubscribeServerConfigInput, UpdateProviderInput, UpdateSettingsInput,
        UpsertKeybindingInput,
    },
    stream,
    terminal::{
        TerminalAttachInput, TerminalCloseInput, TerminalEvent, TerminalMetadataEvent,
        TerminalOpenInput, TerminalRef, TerminalResizeInput, TerminalRestartInput,
        TerminalSessionSnapshot, TerminalWriteInput,
    },
    unary,
    vcs::{
        GitActionProgressEvent, GitPreparePullRequestThreadInput,
        GitPreparePullRequestThreadResult, GitPullRequestRefInput, GitResolvePullRequestResult,
        GitRunStackedActionInput, ReviewDiffPreviewInput, ReviewDiffPreviewResult,
        VcsCreateRefInput, VcsCreateRefResult, VcsCreateWorktreeInput, VcsCreateWorktreeResult,
        VcsCwdInput, VcsInitInput, VcsListRefsInput, VcsListRefsResult, VcsPullResult,
        VcsRemoveWorktreeInput, VcsStatusResult, VcsStatusStreamEvent, VcsSwitchRefInput,
        VcsSwitchRefResult,
    },
};

/// The `{}` payload of methods without input.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Empty {}

// Server
unary!(
    /// Liveness probe when `capabilities.connection_probe`.
    ServerProbe, "server.probe", Empty => IgnoredAny, ServerError
);
unary!(ServerGetConfig, "server.getConfig", Empty => ServerConfig, ServerError);
unary!(ServerRefreshProviders, "server.refreshProviders", RefreshProvidersInput => ProvidersUpdated, ServerError);
unary!(ServerUpdateProvider, "server.updateProvider", UpdateProviderInput => ProvidersUpdated, ServerError);
unary!(ServerUpsertKeybinding, "server.upsertKeybinding", UpsertKeybindingInput => KeybindingsUpdated, ServerError);
unary!(ServerRemoveKeybinding, "server.removeKeybinding", RemoveKeybindingInput => KeybindingsUpdated, ServerError);
unary!(ServerGetSettings, "server.getSettings", Empty => ServerSettings, ServerError);
unary!(ServerUpdateSettings, "server.updateSettings", UpdateSettingsInput => ServerSettings, ServerError);
stream!(
    /// Server config; the first item is the snapshot.
    SubscribeServerConfig, "subscribeServerConfig", SubscribeServerConfigInput => ServerConfigStreamEvent, ServerError
);
stream!(SubscribeServerLifecycle, "subscribeServerLifecycle", Empty => ServerLifecycleStreamEvent, ServerError);
stream!(
    /// Pairing links and client sessions. Needs `access:read`.
    SubscribeAuthAccess, "subscribeAuthAccess", Empty => AuthAccessStreamEvent, ServerError
);

// Projects, files, assets
unary!(ProjectsListEntries, "projects.listEntries", ProjectListEntriesInput => ProjectEntries, ServerError);
unary!(ProjectsReadFile, "projects.readFile", ProjectReadFileInput => ProjectReadFileResult, ServerError);
unary!(ProjectsSearchContents, "projects.searchContents", ProjectSearchContentsInput => ProjectSearchContentsResult, ServerError);
unary!(ProjectsSearchEntries, "projects.searchEntries", ProjectSearchEntriesInput => ProjectEntries, ServerError);
unary!(ProjectsWriteFile, "projects.writeFile", ProjectWriteFileInput => ProjectWriteFileResult, ServerError);
unary!(ShellOpenInEditor, "shell.openInEditor", LaunchEditorInput => (), ServerError);
unary!(FilesystemBrowse, "filesystem.browse", FilesystemBrowseInput => FilesystemBrowseResult, ServerError);
unary!(AssetsCreateUrl, "assets.createUrl", AssetCreateUrlInput => AssetCreateUrlResult, ServerError);
unary!(AttachmentsCreateUploadUrl, "attachments.createUploadUrl", AttachmentCreateUploadUrlInput => AttachmentCreateUploadUrlResult, ServerError);
unary!(AttachmentsDelete, "attachments.delete", AttachmentDeleteInput => (), ServerError);

// VCS and git
stream!(SubscribeVcsStatus, "subscribeVcsStatus", VcsCwdInput => VcsStatusStreamEvent, ServerError);
unary!(VcsPull, "vcs.pull", VcsCwdInput => VcsPullResult, ServerError);
unary!(VcsRefreshStatus, "vcs.refreshStatus", VcsCwdInput => VcsStatusResult, ServerError);
unary!(VcsListRefs, "vcs.listRefs", VcsListRefsInput => VcsListRefsResult, ServerError);
unary!(VcsCreateWorktree, "vcs.createWorktree", VcsCreateWorktreeInput => VcsCreateWorktreeResult, ServerError);
unary!(VcsRemoveWorktree, "vcs.removeWorktree", VcsRemoveWorktreeInput => (), ServerError);
unary!(VcsCreateRef, "vcs.createRef", VcsCreateRefInput => VcsCreateRefResult, ServerError);
unary!(VcsSwitchRef, "vcs.switchRef", VcsSwitchRefInput => VcsSwitchRefResult, ServerError);
unary!(VcsInit, "vcs.init", VcsInitInput => (), ServerError);
stream!(GitRunStackedAction, "git.runStackedAction", GitRunStackedActionInput => GitActionProgressEvent, ServerError);
unary!(GitResolvePullRequest, "git.resolvePullRequest", GitPullRequestRefInput => GitResolvePullRequestResult, ServerError);
unary!(GitPreparePullRequestThread, "git.preparePullRequestThread", GitPreparePullRequestThreadInput => GitPreparePullRequestThreadResult, ServerError);
unary!(ReviewGetDiffPreview, "review.getDiffPreview", ReviewDiffPreviewInput => ReviewDiffPreviewResult, ServerError);

// Terminal
unary!(TerminalOpen, "terminal.open", TerminalOpenInput => TerminalSessionSnapshot, ServerError);
stream!(TerminalAttach, "terminal.attach", TerminalAttachInput => TerminalEvent, ServerError);
unary!(TerminalWrite, "terminal.write", TerminalWriteInput => (), ServerError);
unary!(TerminalResize, "terminal.resize", TerminalResizeInput => (), ServerError);
unary!(TerminalClear, "terminal.clear", TerminalRef => (), ServerError);
unary!(TerminalRestart, "terminal.restart", TerminalRestartInput => TerminalSessionSnapshot, ServerError);
unary!(TerminalClose, "terminal.close", TerminalCloseInput => (), ServerError);
stream!(SubscribeTerminalEvents, "subscribeTerminalEvents", Empty => TerminalEvent, ServerError);
stream!(SubscribeTerminalMetadata, "subscribeTerminalMetadata", Empty => TerminalMetadataEvent, ServerError);

// Orchestration
unary!(DispatchCommand, "orchestration.dispatchCommand", ClientCommand => DispatchResult, ServerError);
unary!(GetTurnDiff, "orchestration.getTurnDiff", GetTurnDiffInput => ThreadTurnDiff, ServerError);
unary!(GetFullThreadDiff, "orchestration.getFullThreadDiff", GetFullThreadDiffInput => ThreadTurnDiff, ServerError);
unary!(SearchThreads, "orchestration.searchThreads", SearchThreadsInput => SearchThreadsResult, ServerError);
unary!(
    /// All projects plus archived threads, one-shot.
    GetArchivedShellSnapshot, "orchestration.getArchivedShellSnapshot", Empty => OrchestrationShellSnapshot, ServerError
);
stream!(SubscribeShell, "orchestration.subscribeShell", SubscribeShellInput => ShellStreamItem, ServerError);
stream!(SubscribeThread, "orchestration.subscribeThread", SubscribeThreadInput => ThreadStreamItem, ServerError);
