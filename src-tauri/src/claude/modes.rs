use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClaudeExecutionMode {
    #[default]
    Default,
    AcceptEdits,
    Auto,
    BypassPermissions,
}

impl ClaudeExecutionMode {
    pub(crate) const ALL: [Self; 4] = [
        Self::Default,
        Self::AcceptEdits,
        Self::Auto,
        Self::BypassPermissions,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Auto => "auto",
            Self::BypassPermissions => "bypassPermissions",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::AcceptEdits => "Accept edits",
            Self::Auto => "Auto",
            Self::BypassPermissions => "Bypass permissions",
        }
    }

    pub(crate) fn from_mode(mode: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == mode)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClaudePermissionMode {
    Default,
    AcceptEdits,
    Auto,
    Plan,
    BypassPermissions,
}

impl ClaudePermissionMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Auto => "auto",
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeModeSettings {
    pub mode: ClaudePermissionMode,
    pub exit_plan_mode: ClaudeExecutionMode,
}

impl ClaudeModeSettings {
    pub(crate) fn restore(mode: Option<&str>, exit_mode: Option<&str>) -> Result<Self, String> {
        let mode: ClaudePermissionMode =
            serde_json::from_value(serde_json::Value::String(mode.unwrap_or("default").into()))
                .map_err(|_| "Unknown saved Claude permission mode".to_string())?;
        let exit_plan_mode = match exit_mode {
            Some(saved) => ClaudeExecutionMode::from_mode(saved)
                .ok_or("Unknown saved Claude execution mode")?,
            None => ClaudeExecutionMode::from_mode(mode.as_str()).unwrap_or_default(),
        };
        Ok(Self {
            mode,
            exit_plan_mode,
        })
    }
}

/// One FIFO for mode writes across every process owned by a driver. An older
/// Plan event must finish writing before a newer Bypass approval is saved.
pub(crate) struct ModeStore(tokio::sync::mpsc::UnboundedSender<ModeWrite>);

enum ModeWrite {
    Save {
        database: turso::Database,
        thread: String,
        mode: String,
        exit_mode: Option<String>,
    },
    Flush(tokio::sync::oneshot::Sender<()>),
}

impl ModeStore {
    pub(crate) fn new() -> Self {
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
        tauri::async_runtime::spawn(async move {
            while let Some(write) = receive.recv().await {
                match write {
                    ModeWrite::Save {
                        database,
                        thread,
                        mode,
                        exit_mode,
                    } => {
                        let _ = crate::storage::set_harness_permission_mode(
                            &database,
                            &thread,
                            &mode,
                            exit_mode.as_deref(),
                        )
                        .await;
                    }
                    ModeWrite::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self(send)
    }

    pub(crate) fn save(
        &self,
        database: turso::Database,
        thread: &str,
        mode: &str,
        exit_mode: Option<&str>,
    ) {
        let _ = self.0.send(ModeWrite::Save {
            database,
            thread: thread.into(),
            mode: mode.into(),
            exit_mode: exit_mode.map(str::to_string),
        });
    }

    pub(crate) async fn flush(&self) {
        let (send, receive) = tokio::sync::oneshot::channel();
        if self.0.send(ModeWrite::Flush(send)).is_ok() {
            let _ = receive.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rapid_plan_and_approval_writes_restore_the_latest_mode() {
        let home = tempfile::tempdir().unwrap();
        let database = crate::storage::open(home.path()).await.unwrap();
        crate::storage::record_harness_thread(&database, "t", "claude", "/repo", "Plan", 0)
            .await
            .unwrap();
        let store = ModeStore::new();
        store.save(database.clone(), "t", "plan", Some("auto"));
        store.save(
            database.clone(),
            "t",
            "bypassPermissions",
            Some("bypassPermissions"),
        );
        store.flush().await;
        let saved = crate::storage::thread_harness(&database, "t")
            .await
            .unwrap()
            .unwrap();
        let modes = ClaudeModeSettings::restore(
            saved.permission_mode.as_deref(),
            saved.plan_exit_mode.as_deref(),
        )
        .unwrap();
        assert_eq!(modes.mode.as_str(), "bypassPermissions");
        assert_eq!(modes.exit_plan_mode.as_str(), "bypassPermissions");
    }
}
