//! iOS Simulators and Android Emulators the agent can drive (`packages/contracts/src/device.ts`;
//! the Device panel and Settings > Devices). Hosts are the local machine or SSH targets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ThreadId;

/// `device.configure`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfigureInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_access_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboarding_completed: Option<bool>,
}

/// `device.list`: refresh the device list, optionally updating a tool or retrying a host.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListInput {
    /// `hub` or `agent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspect_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_host_id: Option<String>,
}

/// Everything the Device panel shows. Returned by `device.configure` / `device.list` and
/// streamed by `subscribeDeviceState` (newest `revision` wins).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceServiceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_host_retry: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_tool_update: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_tool_inspection: Option<bool>,
    #[serde(default)]
    pub hosts: Vec<DeviceHostSummary>,
    /// `disabled`, `idle`, `installing`, `starting`, `ready`, or `failed`.
    pub host_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_status_detail: Option<String>,
    #[serde(default)]
    pub host_statuses: BTreeMap<String, DeviceHostStatus>,
    #[serde(default)]
    pub devices: Vec<DeviceSummary>,
    #[serde(default)]
    pub sessions: Vec<DeviceSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub booting_devices: Option<Vec<BootingDevice>>,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub agent_access_enabled: bool,
    pub hub_base_path: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceHostStatus {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceHostSummary {
    pub id: String,
    /// `local` or `ssh`.
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub platforms: Vec<DevicePlatformAvailability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<DeviceToolVersions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_inspection_error: Option<String>,
    #[serde(default)]
    pub hub_installed: bool,
    #[serde(default)]
    pub agent_device_installed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevicePlatformAvailability {
    /// `ios` or `android`.
    pub platform: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceToolVersions {
    pub hub: DeviceToolVersion,
    pub agent: DeviceToolVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceToolVersion {
    pub required_version: String,
    #[serde(default)]
    pub installed_versions: Vec<String>,
    pub running_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub host_id: String,
    pub id: String,
    pub platform: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub booted: bool,
    #[serde(default)]
    pub physical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootingDevice {
    #[serde(flatten)]
    pub device: DeviceSummary,
    pub thread_id: ThreadId,
}

/// A device open in a thread's Device panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSession {
    pub thread_id: ThreadId,
    pub host_id: String,
    pub device_id: String,
    pub platform: String,
    pub opened_at: String,
}

/// An SSH device host (`device.testHost`; also `ServerSettings.deviceHosts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshDeviceHostConfig {
    pub id: String,
    pub label: String,
    /// `user@host`.
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// `device.open`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOpenInput {
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    pub device_id: String,
    pub platform: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot: Option<bool>,
}

/// `device.close`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCloseInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// Also power the device off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutdown: Option<bool>,
}

/// `device.shutdown`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceShutdownInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    pub device_id: String,
    pub platform: String,
}

/// `device.detail`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    pub device_id: String,
}

/// A device's settings and foreground app (`device.detail`, `device.action`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetail {
    pub host_id: String,
    pub device_id: String,
    pub settings: DeviceSettings,
    pub foreground_app: Option<DeviceForegroundApp>,
    pub read_at: String,
}

/// Current values; `None` where the platform does not report one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_motion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub increase_contrast: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_transparency: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_borders: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_over: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquid_glass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<DeviceLocation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeviceLocation {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceForegroundApp {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// `device.action`: change a setting or act on an app.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceActionInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    pub device_id: String,
    #[serde(flatten)]
    pub action: DeviceAction,
}

/// Value strings follow the contract literals (`light`/`dark`, `small`..`extra-large`,
/// `portrait`/`landscape_left`/..., `grant`/`revoke`/`reset`).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DeviceAction {
    SetAppearance {
        value: String,
    },
    SetTextSize {
        value: String,
    },
    /// `reduceMotion`, `increaseContrast`, `reduceTransparency`, `showBorders`, `voiceOver`,
    /// or `networkEnabled`.
    SetToggle {
        setting: String,
        value: bool,
    },
    SetLiquidGlass {
        value: String,
    },
    SetColorFilter {
        value: String,
    },
    SetOrientation {
        value: String,
    },
    SetLocation {
        latitude: f64,
        longitude: f64,
    },
    ClearLocation,
    #[serde(rename_all = "camelCase")]
    SetPermission {
        app_id: String,
        permission: String,
        decision: String,
    },
    OpenUrl {
        url: String,
    },
    #[serde(rename_all = "camelCase")]
    LaunchApp {
        app_id: String,
    },
    #[serde(rename_all = "camelCase")]
    TerminateApp {
        app_id: String,
    },
    Shake,
    /// `payload` is an APNs JSON object or its string form.
    #[serde(rename_all = "camelCase")]
    SendPush {
        app_id: String,
        payload: Value,
    },
}
