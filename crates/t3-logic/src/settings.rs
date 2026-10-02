//! Client-only settings (`ClientSettingsSchema`, `packages/contracts/src/settings.ts:42-95`),
//! persisted by the app as `client-settings.json` in the same shape as Electron's file.
//!
//! Every field decodes on its own: a missing or invalid value falls back to its default instead of
//! failing the whole file, and unknown keys are ignored.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::Value;

/// How sidebar projects are ordered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSortOrder {
    /// Newest user message first ("Last user message").
    #[default]
    UpdatedAt,
    CreatedAt,
    /// The user's drag order (`UiState::project_order`).
    Manual,
}

/// How threads inside a project are ordered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadSortOrder {
    #[default]
    UpdatedAt,
    CreatedAt,
}

/// Which projects share a sidebar row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectGroupingMode {
    /// Same repository (`repositoryIdentity.canonicalKey`).
    #[default]
    Repository,
    /// Same repository and repo-relative path.
    RepositoryPath,
    /// Every project path is its own row.
    Separate,
}

impl ProjectGroupingMode {
    /// Menu label ("Group by repository", ...).
    pub fn label(self) -> &'static str {
        match self {
            Self::Repository => "Group by repository",
            Self::RepositoryPath => "Group by repository path",
            Self::Separate => "Keep separate",
        }
    }
}

/// Clock format for absolute timestamps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimestampFormat {
    #[default]
    #[serde(rename = "locale")]
    Locale,
    #[serde(rename = "12-hour")]
    TwelveHour,
    #[serde(rename = "24-hour")]
    TwentyFourHour,
}

/// Number of threads a project shows before "Show more" (1 to 15, default 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ThreadPreviewCount(u8);

impl ThreadPreviewCount {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 15;

    /// `None` outside 1..=15.
    pub fn new(count: u8) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&count)
            .then_some(Self(count))
    }

    pub fn get(self) -> usize {
        usize::from(self.0)
    }
}

impl Default for ThreadPreviewCount {
    fn default() -> Self {
        Self(6)
    }
}

impl<'de> Deserialize<'de> for ThreadPreviewCount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let count = u8::deserialize(deserializer)?;
        Self::new(count).ok_or_else(|| serde::de::Error::custom("preview count out of range"))
    }
}

/// A favorited model in the model picker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFavorite {
    /// Provider instance id (historically the provider kind).
    pub provider: String,
    pub model: String,
}

/// Per provider instance model list preferences.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderModelPreferences {
    pub hidden_models: Vec<String>,
    pub model_order: Vec<String>,
}

/// Local client settings. Defaults match `DEFAULT_CLIENT_SETTINGS`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClientSettings {
    #[serde(deserialize_with = "lenient")]
    pub auto_open_plan_sidebar: bool,
    #[serde(deserialize_with = "lenient")]
    pub confirm_thread_archive: bool,
    #[serde(deserialize_with = "lenient")]
    pub confirm_thread_delete: bool,
    #[serde(deserialize_with = "lenient")]
    pub dismissed_provider_update_notification_keys: Vec<String>,
    #[serde(deserialize_with = "lenient")]
    pub diff_ignore_whitespace: bool,
    #[serde(deserialize_with = "lenient")]
    pub favorites: Vec<ModelFavorite>,
    #[serde(deserialize_with = "lenient")]
    pub provider_model_preferences: BTreeMap<String, ProviderModelPreferences>,
    /// The project-grouped sidebar (`build_sidebar`) instead of the sectioned inbox.
    #[serde(deserialize_with = "lenient")]
    pub legacy_sidebar_enabled: bool,
    /// Beta: fold threads busy with work that does not need the user into a Working shelf.
    #[serde(deserialize_with = "lenient")]
    pub sidebar_working_shelf_enabled: bool,
    #[serde(deserialize_with = "lenient")]
    pub sidebar_project_grouping_mode: ProjectGroupingMode,
    /// Per physical project key (`<environmentId>:<normalized path>`).
    #[serde(deserialize_with = "lenient")]
    pub sidebar_project_grouping_overrides: BTreeMap<String, ProjectGroupingMode>,
    #[serde(deserialize_with = "lenient")]
    pub sidebar_project_sort_order: ProjectSortOrder,
    #[serde(deserialize_with = "lenient")]
    pub sidebar_thread_sort_order: ThreadSortOrder,
    #[serde(deserialize_with = "lenient")]
    pub sidebar_thread_preview_count: ThreadPreviewCount,
    #[serde(deserialize_with = "lenient")]
    pub timestamp_format: TimestampFormat,
    #[serde(deserialize_with = "lenient")]
    pub word_wrap: bool,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            auto_open_plan_sidebar: false,
            confirm_thread_archive: false,
            confirm_thread_delete: true,
            dismissed_provider_update_notification_keys: Vec::new(),
            diff_ignore_whitespace: true,
            favorites: Vec::new(),
            provider_model_preferences: BTreeMap::new(),
            legacy_sidebar_enabled: false,
            sidebar_working_shelf_enabled: false,
            sidebar_project_grouping_mode: ProjectGroupingMode::default(),
            sidebar_project_grouping_overrides: BTreeMap::new(),
            sidebar_project_sort_order: ProjectSortOrder::default(),
            sidebar_thread_sort_order: ThreadSortOrder::default(),
            sidebar_thread_preview_count: ThreadPreviewCount::default(),
            timestamp_format: TimestampFormat::default(),
            word_wrap: true,
        }
    }
}

impl ClientSettings {
    /// Grouping mode for one physical project: its override, else the global mode.
    pub fn grouping_mode_for(&self, physical_key: &str) -> ProjectGroupingMode {
        self.sidebar_project_grouping_overrides
            .get(physical_key)
            .copied()
            .unwrap_or(self.sidebar_project_grouping_mode)
    }

    /// Decodes a settings file, falling back to defaults for anything unreadable.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

/// Decodes a field, or its default when the value has the wrong shape.
fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    //! Failure modes: one bad field wiping every setting, out-of-range preview counts, unknown
    //! keys, and the wire spelling of enum values (`updated_at`, `12-hour`).
    use super::*;

    #[test]
    fn invalid_fields_fall_back_individually() {
        let settings = ClientSettings::from_json(
            r#"{"confirmThreadDelete":false,"sidebarThreadPreviewCount":40,
                "sidebarProjectSortOrder":"bogus","timestampFormat":"24-hour","futureKey":1}"#,
        );
        assert!(!settings.confirm_thread_delete);
        assert_eq!(settings.sidebar_thread_preview_count.get(), 6);
        assert_eq!(
            settings.sidebar_project_sort_order,
            ProjectSortOrder::UpdatedAt
        );
        assert_eq!(settings.timestamp_format, TimestampFormat::TwentyFourHour);
        assert_eq!(
            ClientSettings::from_json("not json"),
            ClientSettings::default()
        );
    }

    #[test]
    fn serializes_with_wire_names() {
        let mut settings = ClientSettings {
            sidebar_project_sort_order: ProjectSortOrder::Manual,
            ..ClientSettings::default()
        };
        settings
            .sidebar_project_grouping_overrides
            .insert("env:/a".into(), ProjectGroupingMode::RepositoryPath);
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["sidebarProjectSortOrder"], "manual");
        assert_eq!(json["sidebarThreadPreviewCount"], 6);
        assert_eq!(
            json["sidebarProjectGroupingOverrides"]["env:/a"],
            "repository_path"
        );
        assert_eq!(ClientSettings::from_json(&json.to_string()), settings);
        assert_eq!(
            settings.grouping_mode_for("env:/a"),
            ProjectGroupingMode::RepositoryPath
        );
        assert_eq!(
            settings.grouping_mode_for("env:/b"),
            ProjectGroupingMode::Repository
        );
    }
}
