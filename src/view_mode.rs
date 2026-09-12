//! Shared persisted view preference; independent of the application window.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    Source,
    #[default]
    LivePreview,
    Reading,
}
