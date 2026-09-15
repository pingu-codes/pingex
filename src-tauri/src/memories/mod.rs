//! Memories: the notes a harness keeps between conversations, on disk in its
//! Home. Codex consolidates them under `$CODEX_HOME/memories`; Claude Code
//! writes one directory per project under `<config>/projects/<slug>/memory`.
//!
//! Neither harness offers a protocol for editing them, so this module reads
//! and writes the files directly. The types below are harness-neutral: a Home
//! has switches and stores, a store has entries. What differs per harness
//! (layout, index file, where the switches live) stays in `codex` and `claude`.

mod claude;
mod codex;
pub(crate) mod commands;
mod files;

use serde::Serialize;

/// The memories of one Home in the active Profile.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryHome {
    pub home_id: String,
    pub label: String,
    pub config_dir: String,
    pub toggles: Vec<MemoryToggle>,
    pub stores: Vec<MemoryStore>,
    /// Why this Home's memories could not be read, when they could not.
    pub error: Option<String>,
}

/// An on/off switch for how the harness uses memories.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryToggle {
    pub key: String,
    pub label: String,
    pub description: String,
    pub value: bool,
}

/// One directory of memories.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryStore {
    /// The directory as the Home's Host sees it. Unique within a Home.
    pub root: String,
    pub title: String,
    /// The project these memories belong to, when they are per project.
    pub project_path: Option<String>,
    pub entries: Vec<MemoryEntry>,
}

/// One memory file.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryEntry {
    /// The file as the Home's Host sees it.
    pub path: String,
    /// The path below the store root, `/`-separated.
    pub rel_path: String,
    pub title: String,
    pub description: Option<String>,
    /// What sort of memory this is: `index`, `summary`, `raw`, `rollout`,
    /// `skill`, or the type a Claude memory declares (`user`, `feedback`, ...).
    pub kind: Option<String>,
    /// Seconds since the epoch.
    pub modified_at: Option<f64>,
    pub bytes: f64,
    /// A file the store's index does not link to, so the harness never loads it.
    pub unindexed: bool,
}
