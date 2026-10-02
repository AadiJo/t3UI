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
    device::{
        DeviceActionInput, DeviceCloseInput, DeviceConfigureInput, DeviceDetail, DeviceHostSummary,
        DeviceListInput, DeviceOpenInput, DeviceRef, DeviceServiceState, DeviceSession,
        DeviceShutdownInput, SshDeviceHostConfig,
    },
    errors::ServerError,
    orchestration::{
        DispatchResult, GetFullThreadDiffInput, GetTurnDiffInput, OrchestrationShellSnapshot,
        SearchThreadsInput, SearchThreadsResult, ShellStreamItem, SubscribeShellInput,
        SubscribeThreadInput, ThreadStreamItem, ThreadTurnDiff,
    },
    preview::{
        DiscoveredLocalServerList, DiscoveredLocalServersInput, PreviewAutomationHost,
        PreviewAutomationHostFocus, PreviewAutomationResponse, PreviewAutomationStreamEvent,
        PreviewCloseInput, PreviewEvent, PreviewListInput, PreviewListResult, PreviewNavigateInput,
        PreviewOpenInput, PreviewReportStatusInput, PreviewResizeInput, PreviewSessionSnapshot,
        PreviewTabRef,
    },
    projects::{
        AssetCreateUrlInput, AssetCreateUrlResult, AttachmentCreateUploadUrlInput,
        AttachmentCreateUploadUrlResult, AttachmentDeleteInput, FilesystemBrowseInput,
        FilesystemBrowseResult, LaunchEditorInput, ProjectEntries, ProjectListEntriesInput,
        ProjectReadFileInput, ProjectReadFileResult, ProjectSearchContentsInput,
        ProjectSearchContentsResult, ProjectSearchEntriesInput, ProjectWriteFileInput,
        ProjectWriteFileResult,
    },
    providers::{
        ChatGptHandoffInput, ChatGptHandoffState, ChatGptImportProfileInput,
        ChatGptReconnectProfileInput, CodexAuthCallbackInput, CodexAuthCallbackState,
        ProviderAuthCancelInput, ProviderAuthCompleteInput, ProviderAuthRespondInput,
        ProviderAuthStartInput, ProviderAuthState, ProviderInstallCancelInput,
        ProviderInstallState, ProviderSetupInput, ProviderUploadFeedbackInput,
        ProviderUploadFeedbackResult,
    },
    pull_requests::{
        PullRequestActionInput, PullRequestActivity, PullRequestCommentInput,
        PullRequestCommentUpdateInput, PullRequestDetail, PullRequestDiffFileContentsInput,
        PullRequestDiffFileContentsResult, PullRequestFilesViewedResult,
        PullRequestInvalidateInput, PullRequestLabelCandidateList, PullRequestLabelChangeInput,
        PullRequestLinkedThreadsResult, PullRequestListInput, PullRequestListResult,
        PullRequestListStatsInput, PullRequestListStatsResult, PullRequestPreview,
        PullRequestReactionInput, PullRequestRef, PullRequestReviewerCandidateList,
        PullRequestReviewerRequestInput, PullRequestRoutingIdentityInput,
        PullRequestRoutingIdentityResult, PullRequestRoutingResult, PullRequestSetFilesViewedInput,
        PullRequestStack, PullRequestSubmitReviewInput, PullRequestSummary,
        PullRequestThreadCommentsInput, PullRequestThreadCommentsResult,
        PullRequestThreadReplyInput, PullRequestThreadResolutionInput, PullRequestUpdateInput,
    },
    server::{
        AuthAccessStreamEvent, KeybindingsUpdated, ProvidersUpdated, RefreshProvidersInput,
        RemoveKeybindingInput, ServerConfig, ServerConfigStreamEvent, ServerLifecycleStreamEvent,
        ServerSettings, SubscribeServerConfigInput, UpdateProviderInput, UpdateSettingsInput,
        UpsertKeybindingInput,
    },
    server_ops::{
        ClientActivityReportInput, DesktopUpdateCommitInput, HostResourcesSnapshot,
        RelayClientInstallProgress, RelayClientStatus, ResourceHistoryInput,
        ResourceTelemetryHistory, ResourceTelemetryRetryResult, ResourceTelemetrySnapshot,
        ServerProcessDiagnostics, ServerProcessResourceHistory, ServerSelfUpdateInput,
        ServerSelfUpdateProgressEvent, ServerSelfUpdateResult, ServerSignalProcessInput,
        ServerSignalProcessResult, ServerTraceDiagnostics, SourceControlDiscoveryResult,
    },
    stream,
    terminal::{
        TerminalAttachInput, TerminalCloseInput, TerminalEvent, TerminalMetadataEvent,
        TerminalOpenInput, TerminalRef, TerminalResizeInput, TerminalRestartInput,
        TerminalSessionSnapshot, TerminalWriteInput,
    },
    unary,
    usage::{
        ConsumeResetCreditInput, ConsumeResetCreditResult, UsagePricing, UsageSummary,
        UsageSummaryInput,
    },
    vcs::{
        GitActionProgressEvent, GitPreparePullRequestThreadInput,
        GitPreparePullRequestThreadResult, GitPullRequestRefInput, GitResolvePullRequestResult,
        GitRunStackedActionInput, ReviewDiffPreviewInput, ReviewDiffPreviewResult,
        VcsCreateRefInput, VcsCreateRefResult, VcsCreateWorktreeInput, VcsCreateWorktreeResult,
        VcsCwdInput, VcsInitInput, VcsListRefsInput, VcsListRefsResult, VcsPullResult,
        VcsRemoveWorktreeInput, VcsStatusResult, VcsStatusStreamEvent, VcsSwitchRefInput,
        VcsSwitchRefResult,
    },
    workspace::{
        AgentSessionImportInput, AgentSessionImportResult, AgentSessionScanResult,
        FileContentsPair, GetWorkflowScriptInput, ProjectCloneActionInput,
        ProjectCloneActionResult, ProjectCloneSnapshot, ProjectCloneStartInput,
        ProjectCloneStartResult, ProjectCreateNewInput, ProjectCreateNewResult,
        ProjectEnsureScratchResult, ReviewDiffFileContentsInput, SourceControlCloneRepositoryInput,
        SourceControlCloneRepositoryResult, SourceControlPublishRepositoryInput,
        SourceControlPublishRepositoryResult, SourceControlRepositoryInfo,
        SourceControlRepositoryLookupInput, WorkflowScript, WorktreeSetupCancelResult,
        WorktreeSetupSnapshot, WorktreeSetupThreadInput,
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

// Pull requests (the `/pull-requests` route). The diff itself is HTTP:
// `EnvironmentHttp::pull_request_diff`.
unary!(PullRequestsList, "pullRequests.list", PullRequestListInput => PullRequestListResult, ServerError);
unary!(PullRequestsListStats, "pullRequests.listStats", PullRequestListStatsInput => PullRequestListStatsResult, ServerError);
unary!(PullRequestsSummary, "pullRequests.summary", PullRequestRef => PullRequestSummary, ServerError);
unary!(PullRequestsRouting, "pullRequests.routing", PullRequestRef => PullRequestRoutingResult, ServerError);
unary!(PullRequestsRoutingIdentity, "pullRequests.routingIdentity", PullRequestRoutingIdentityInput => PullRequestRoutingIdentityResult, ServerError);
unary!(
    /// `None` when the pull request is not part of a stack.
    PullRequestsStack, "pullRequests.stack", PullRequestRef => Option<PullRequestStack>, ServerError
);
unary!(PullRequestsLinkedThreads, "pullRequests.linkedThreads", PullRequestRef => PullRequestLinkedThreadsResult, ServerError);
unary!(PullRequestsDetail, "pullRequests.detail", PullRequestRef => PullRequestDetail, ServerError);
unary!(PullRequestsPreview, "pullRequests.preview", PullRequestRef => PullRequestPreview, ServerError);
unary!(PullRequestsActivity, "pullRequests.activity", PullRequestRef => PullRequestActivity, ServerError);
unary!(PullRequestsThreadComments, "pullRequests.threadComments", PullRequestThreadCommentsInput => PullRequestThreadCommentsResult, ServerError);
unary!(PullRequestsDiffFileContents, "pullRequests.diffFileContents", PullRequestDiffFileContentsInput => PullRequestDiffFileContentsResult, ServerError);
unary!(PullRequestsFilesViewed, "pullRequests.filesViewed", PullRequestRef => PullRequestFilesViewedResult, ServerError);
unary!(PullRequestsSetFilesViewed, "pullRequests.setFilesViewed", PullRequestSetFilesViewedInput => (), ServerError);
unary!(PullRequestsRunAction, "pullRequests.runAction", PullRequestActionInput => (), ServerError);
unary!(PullRequestsUpdate, "pullRequests.update", PullRequestUpdateInput => (), ServerError);
unary!(PullRequestsComment, "pullRequests.comment", PullRequestCommentInput => (), ServerError);
unary!(PullRequestsUpdateComment, "pullRequests.updateComment", PullRequestCommentUpdateInput => (), ServerError);
unary!(PullRequestsSubmitReview, "pullRequests.submitReview", PullRequestSubmitReviewInput => (), ServerError);
unary!(PullRequestsReplyToThread, "pullRequests.replyToThread", PullRequestThreadReplyInput => (), ServerError);
unary!(PullRequestsSetThreadResolution, "pullRequests.setThreadResolution", PullRequestThreadResolutionInput => (), ServerError);
unary!(PullRequestsSetReaction, "pullRequests.setReaction", PullRequestReactionInput => (), ServerError);
unary!(PullRequestsInvalidate, "pullRequests.invalidate", PullRequestInvalidateInput => (), ServerError);
stream!(
    /// A revision counter that ticks whenever cached pull request data changed; refetch what
    /// is on screen.
    PullRequestsSubscribeRefreshes, "pullRequests.subscribeRefreshes", Empty => u64, ServerError
);
unary!(PullRequestsReviewerCandidates, "pullRequests.reviewerCandidates", PullRequestRef => PullRequestReviewerCandidateList, ServerError);
unary!(PullRequestsRequestReviewers, "pullRequests.requestReviewers", PullRequestReviewerRequestInput => (), ServerError);
unary!(PullRequestsLabelCandidates, "pullRequests.labelCandidates", PullRequestRef => PullRequestLabelCandidateList, ServerError);
unary!(PullRequestsSetLabels, "pullRequests.setLabels", PullRequestLabelChangeInput => (), ServerError);

// Usage (the `/usage` route)
unary!(ServerGetUsageSummary, "server.getUsageSummary", UsageSummaryInput => UsageSummary, ServerError);
unary!(ServerRefreshUsageRates, "server.refreshUsageRates", Empty => UsagePricing, ServerError);
unary!(ProviderConsumeResetCredit, "provider.consumeResetCredit", ConsumeResetCreditInput => ConsumeResetCreditResult, ServerError);

// Provider setup (Settings > Providers, onboarding)
unary!(ProviderAuthStart, "provider.auth.start", ProviderAuthStartInput => ProviderAuthState, ServerError);
unary!(ProviderAuthComplete, "provider.auth.complete", ProviderAuthCompleteInput => ProviderAuthState, ServerError);
unary!(ProviderAuthRespond, "provider.auth.respond", ProviderAuthRespondInput => ProviderAuthState, ServerError);
unary!(ProviderAuthCancel, "provider.auth.cancel", ProviderAuthCancelInput => ProviderAuthState, ServerError);
unary!(ProviderAuthLogout, "provider.auth.logout", ProviderSetupInput => ProviderAuthState, ServerError);
stream!(ProviderAuthSubscribe, "provider.auth.subscribe", ProviderSetupInput => ProviderAuthState, ServerError);
unary!(ChatGptReconnectProfile, "provider.chatgpt.reconnect-profile", ChatGptReconnectProfileInput => Option<crate::providers::ChatGptReconnectProfile>, ServerError);
unary!(ChatGptImportProfile, "provider.chatgpt.import-profile", ChatGptImportProfileInput => ProviderAuthState, ServerError);
stream!(ChatGptHandoffSubscribe, "provider.chatgpt.handoff.subscribe", ChatGptHandoffInput => ChatGptHandoffState, ServerError);
stream!(CodexAuthCallbackSubscribe, "provider.codex.auth-callback.subscribe", CodexAuthCallbackInput => CodexAuthCallbackState, ServerError);
unary!(ProviderInstallStart, "provider.install.start", ProviderSetupInput => ProviderInstallState, ServerError);
unary!(ProviderInstallCancel, "provider.install.cancel", ProviderInstallCancelInput => ProviderInstallState, ServerError);
unary!(ProviderInstallRemove, "provider.install.remove", ProviderSetupInput => ProviderInstallState, ServerError);
stream!(ProviderInstallSubscribe, "provider.install.subscribe", ProviderSetupInput => ProviderInstallState, ServerError);
unary!(ProviderUploadFeedback, "provider.uploadFeedback", ProviderUploadFeedbackInput => ProviderUploadFeedbackResult, ServerError);

// Server operations (Settings > Diagnostics, About)
unary!(ServerUpdateServer, "server.updateServer", ServerSelfUpdateInput => ServerSelfUpdateResult, ServerError);
stream!(ServerUpdateServerWithProgress, "server.updateServerWithProgress", ServerSelfUpdateInput => ServerSelfUpdateProgressEvent, ServerError);
unary!(ServerCommitDesktopUpdate, "server.commitDesktopUpdate", DesktopUpdateCommitInput => ServerSelfUpdateResult, ServerError);
unary!(ServerDiscoverSourceControl, "server.discoverSourceControl", Empty => SourceControlDiscoveryResult, ServerError);
unary!(ServerGetTraceDiagnostics, "server.getTraceDiagnostics", Empty => ServerTraceDiagnostics, ServerError);
unary!(ServerGetProcessDiagnostics, "server.getProcessDiagnostics", Empty => ServerProcessDiagnostics, ServerError);
unary!(ServerGetHostResources, "server.getHostResources", Empty => HostResourcesSnapshot, ServerError);
unary!(ServerGetProcessResourceHistory, "server.getProcessResourceHistory", ResourceHistoryInput => ServerProcessResourceHistory, ServerError);
unary!(ServerGetResourceTelemetryHistory, "server.getResourceTelemetryHistory", ResourceHistoryInput => ResourceTelemetryHistory, ServerError);
unary!(ServerRetryResourceTelemetry, "server.retryResourceTelemetry", Empty => ResourceTelemetryRetryResult, ServerError);
stream!(SubscribeResourceTelemetry, "subscribeResourceTelemetry", Empty => ResourceTelemetrySnapshot, ServerError);
unary!(ServerSignalProcess, "server.signalProcess", ServerSignalProcessInput => ServerSignalProcessResult, ServerError);
unary!(
    /// Keeps the server's background work (git fetch, provider health) alive for this client.
    ServerReportClientActivity, "server.reportClientActivity", ClientActivityReportInput => (), ServerError
);
unary!(CloudGetRelayClientStatus, "cloud.getRelayClientStatus", Empty => RelayClientStatus, ServerError);
stream!(CloudInstallRelayClient, "cloud.installRelayClient", Empty => RelayClientInstallProgress, ServerError);

// Workspaces: source control, clones, new projects, session import, worktree setup
unary!(SourceControlLookupRepository, "sourceControl.lookupRepository", SourceControlRepositoryLookupInput => SourceControlRepositoryInfo, ServerError);
unary!(SourceControlCloneRepository, "sourceControl.cloneRepository", SourceControlCloneRepositoryInput => SourceControlCloneRepositoryResult, ServerError);
unary!(SourceControlPublishRepository, "sourceControl.publishRepository", SourceControlPublishRepositoryInput => SourceControlPublishRepositoryResult, ServerError);
unary!(ProjectCloneStart, "projectClone.start", ProjectCloneStartInput => ProjectCloneStartResult, ServerError);
unary!(ProjectCloneCancel, "projectClone.cancel", ProjectCloneActionInput => ProjectCloneActionResult, ServerError);
unary!(ProjectCloneRetry, "projectClone.retry", ProjectCloneActionInput => ProjectCloneActionResult, ServerError);
stream!(
    /// The full list of tracked clones on every change.
    SubscribeProjectClones, "subscribeProjectClones", Empty => Vec<ProjectCloneSnapshot>, ServerError
);
unary!(ProjectsEnsureScratch, "projects.ensureScratch", Empty => ProjectEnsureScratchResult, ServerError);
unary!(ProjectsCreateNew, "projects.createNew", ProjectCreateNewInput => ProjectCreateNewResult, ServerError);
unary!(AgentSessionsScan, "agentSessions.scan", Empty => AgentSessionScanResult, ServerError);
unary!(AgentSessionsImport, "agentSessions.import", AgentSessionImportInput => AgentSessionImportResult, ServerError);
stream!(
    /// `None` items mean no setup runs for the thread.
    SubscribeWorktreeSetup, "subscribeWorktreeSetup", WorktreeSetupThreadInput => Option<WorktreeSetupSnapshot>, ServerError
);
unary!(WorktreeSetupCancel, "worktreeSetup.cancel", WorktreeSetupThreadInput => WorktreeSetupCancelResult, ServerError);
unary!(ReviewGetDiffFileContents, "review.getDiffFileContents", ReviewDiffFileContentsInput => FileContentsPair, ServerError);
unary!(GetWorkflowScript, "orchestration.getWorkflowScript", GetWorkflowScriptInput => WorkflowScript, ServerError);

// In-app browser preview
unary!(PreviewOpen, "preview.open", PreviewOpenInput => PreviewSessionSnapshot, ServerError);
unary!(PreviewNavigate, "preview.navigate", PreviewNavigateInput => PreviewSessionSnapshot, ServerError);
unary!(PreviewResize, "preview.resize", PreviewResizeInput => PreviewSessionSnapshot, ServerError);
unary!(PreviewRefresh, "preview.refresh", PreviewTabRef => (), ServerError);
unary!(PreviewClose, "preview.close", PreviewCloseInput => (), ServerError);
unary!(PreviewList, "preview.list", PreviewListInput => PreviewListResult, ServerError);
unary!(PreviewReportStatus, "preview.reportStatus", PreviewReportStatusInput => (), ServerError);
stream!(SubscribePreviewEvents, "subscribePreviewEvents", Empty => PreviewEvent, ServerError);
stream!(PreviewAutomationConnect, "previewAutomation.connect", PreviewAutomationHost => PreviewAutomationStreamEvent, ServerError);
unary!(PreviewAutomationRespond, "previewAutomation.respond", PreviewAutomationResponse => (), ServerError);
unary!(PreviewAutomationFocusHost, "previewAutomation.focusHost", PreviewAutomationHostFocus => (), ServerError);
stream!(SubscribeDiscoveredLocalServers, "subscribeDiscoveredLocalServers", DiscoveredLocalServersInput => DiscoveredLocalServerList, ServerError);

// Devices (simulators and emulators)
unary!(DeviceConfigure, "device.configure", DeviceConfigureInput => DeviceServiceState, ServerError);
unary!(DeviceList, "device.list", DeviceListInput => DeviceServiceState, ServerError);
unary!(DeviceTestHost, "device.testHost", SshDeviceHostConfig => DeviceHostSummary, ServerError);
unary!(DeviceOpen, "device.open", DeviceOpenInput => DeviceSession, ServerError);
unary!(DeviceClose, "device.close", DeviceCloseInput => (), ServerError);
unary!(DeviceShutdown, "device.shutdown", DeviceShutdownInput => (), ServerError);
unary!(DeviceGetDetail, "device.detail", DeviceRef => DeviceDetail, ServerError);
unary!(DeviceRunAction, "device.action", DeviceActionInput => DeviceDetail, ServerError);
stream!(SubscribeDeviceState, "subscribeDeviceState", Empty => DeviceServiceState, ServerError);
