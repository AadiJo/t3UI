//! Terminal RPC shapes (`packages/contracts/src/terminal.ts`, protocol.md 8.1). Terminals are
//! keyed by `(thread_id, terminal_id)`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ids::{ProviderInstanceId, TerminalId, ThreadId},
    open_enum,
};

open_enum! {
    pub enum TerminalStatus {
        Starting = "starting",
        Running = "running",
        Exited = "exited",
        Error = "error",
    }
}

/// `terminal.open`. Returns a [`TerminalSessionSnapshot`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenInput {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<ProviderInstanceId>,
}

/// `terminal.attach` (stream). The first item is a snapshot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachInput {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<ProviderInstanceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_if_not_running: Option<bool>,
}

/// `terminal.write`. `data` is raw input, e.g. `"ls\r"`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalWriteInput {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResizeInput {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    pub cols: u16,
    pub rows: u16,
}

/// `terminal.clear`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRef {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRestartInput {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    pub cols: u16,
    pub rows: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<ProviderInstanceId>,
}

/// `terminal.close`. No `terminal_id` closes all of the thread's terminals.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCloseInput {
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<TerminalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_history: Option<bool>,
}

/// A terminal session. Replay `history` into the VT parser before live output.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionSnapshot {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    pub cwd: String,
    pub worktree_path: Option<String>,
    pub status: TerminalStatus,
    pub pid: Option<u32>,
    #[serde(default)]
    pub history: String,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    #[serde(default)]
    pub label: String,
    pub updated_at: String,
    pub sequence: Option<u64>,
}

/// An item of `terminal.attach` (first item `Snapshot`) or `subscribeTerminalEvents`
/// (`Started` instead of `Snapshot`). `data` is raw terminal output as UTF-8.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    Snapshot {
        snapshot: TerminalSessionSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    Started {
        thread_id: ThreadId,
        terminal_id: TerminalId,
        snapshot: TerminalSessionSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    Output {
        thread_id: ThreadId,
        terminal_id: TerminalId,
        sequence: Option<u64>,
        data: String,
    },
    #[serde(rename_all = "camelCase")]
    Exited {
        thread_id: ThreadId,
        terminal_id: TerminalId,
        exit_code: Option<i32>,
        exit_signal: Option<i32>,
    },
    #[serde(rename_all = "camelCase")]
    Closed {
        thread_id: ThreadId,
        terminal_id: TerminalId,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        thread_id: ThreadId,
        terminal_id: TerminalId,
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Cleared {
        thread_id: ThreadId,
        terminal_id: TerminalId,
    },
    #[serde(rename_all = "camelCase")]
    Restarted {
        thread_id: ThreadId,
        terminal_id: TerminalId,
        snapshot: TerminalSessionSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    Activity {
        thread_id: ThreadId,
        terminal_id: TerminalId,
        has_running_subprocess: bool,
        label: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSummary {
    pub thread_id: ThreadId,
    pub terminal_id: TerminalId,
    pub cwd: String,
    pub worktree_path: Option<String>,
    pub status: TerminalStatus,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    #[serde(default)]
    pub has_running_subprocess: bool,
    #[serde(default)]
    pub label: String,
    pub updated_at: String,
}

/// An item of `subscribeTerminalMetadata`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalMetadataEvent {
    Snapshot {
        terminals: Vec<TerminalSummary>,
    },
    Upsert {
        terminal: TerminalSummary,
    },
    #[serde(rename_all = "camelCase")]
    Remove {
        thread_id: ThreadId,
        terminal_id: TerminalId,
    },
    #[serde(other)]
    Unknown,
}
