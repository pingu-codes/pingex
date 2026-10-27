//! What one ACP agent needs beyond the protocol: how to launch it, how to
//! tell whether it is installed and logged in, what models it offers, and
//! the few habits of its wire the translator has to know. A new ACP harness
//! is a [`HarnessKind`] variant and one of these.

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::harness::{version_below, HarnessKind, HarnessStatus, ToolCallContent};
use crate::util::host::Host;

/// One mode the composer offers for a harness, and what it means on the
/// wire: an ACP session mode, run on a process that does or does not prompt.
pub(crate) struct AcpMode {
    /// The app's id for it, stored as the thread's permission mode.
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    /// The ACP `modeId` the session switches to.
    pub(crate) session_mode: &'static str,
    /// Whether the process is started with its prompts switched off.
    pub(crate) skips_prompts: bool,
    pub(crate) plan: bool,
}

/// The model entry that leaves the choice to the agent: no `-m` at launch.
pub(crate) const DEFAULT_MODEL: &str = "default";

/// What a process is launched with. A change to either means a new process.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnSettings {
    pub model: Option<String>,
    pub skips_prompts: bool,
}

/// Where the driver finds the agent's binary, and on which host.
#[derive(Clone)]
pub(crate) struct AcpRuntime {
    pub(crate) binary: PathBuf,
    pub(crate) host: Host,
}

impl AcpRuntime {
    /// The environment override wins over the Home's binary, as for Claude.
    pub(crate) fn resolve(spec: &dyn AcpSpec, binary: Option<&str>, host: Host) -> Self {
        let binary = std::env::var_os(spec.binary_env())
            .map(PathBuf::from)
            .or_else(|| binary.filter(|b| !b.is_empty()).map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(spec.default_binary()));
        Self { binary, host }
    }

    pub(crate) fn binary_str(&self) -> String {
        self.binary.to_string_lossy().into_owned()
    }
}

pub(crate) trait AcpSpec: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn default_binary(&self) -> &'static str;
    /// The environment variable that points the app at another binary.
    fn binary_env(&self) -> &'static str;
    /// The oldest release the driver is known to work with.
    fn version_floor(&self) -> &'static str;
    /// Where the agent keeps its configuration on `host`, for display.
    fn config_dir(&self, host: &Host) -> String;
    /// The command line after the binary.
    fn args(&self, settings: &SpawnSettings) -> Vec<String>;
    /// The modes the composer offers, the first being the default.
    fn modes(&self) -> &'static [AcpMode];
    fn logged_in(&self, host: &Host, program: &str) -> Option<bool>;
    /// The composer's model list, in Codex's `model/list` shape.
    fn models(&self, host: &Host, program: &str) -> Result<Value, String>;

    /// The version in `--version` output.
    fn version_of(&self, stdout: &str) -> Option<String> {
        stdout.split_whitespace().last().map(str::to_string)
    }

    /// Tool calls whose id starts with this are the agent's notices to the
    /// user, not tools.
    fn notice_prefix(&self) -> Option<&'static str> {
        None
    }

    /// A diff for an edit the agent announces without one, built from the
    /// tool's input.
    fn tool_diff(&self, _name: &str, _raw_input: &Value) -> Vec<ToolCallContent> {
        Vec::new()
    }

    /// The command line that deletes a session, for an agent that keeps them.
    fn delete_session_args(&self, _session_id: &str) -> Option<Vec<String>> {
        None
    }

    fn mode(&self, id: &str) -> Option<&'static AcpMode> {
        self.modes().iter().find(|mode| mode.id == id)
    }

    /// The app mode a session reports, given how its process was started.
    fn app_mode(&self, session_mode: &str, skips_prompts: bool) -> Option<&'static AcpMode> {
        self.modes()
            .iter()
            .find(|mode| mode.session_mode == session_mode && mode.skips_prompts == skips_prompts)
            .or_else(|| {
                self.modes()
                    .iter()
                    .find(|mode| mode.session_mode == session_mode)
            })
    }
}

/// The Codex `model/list` entry for one model id.
pub(crate) fn model_entry(id: &str, name: &str, description: &str, is_default: bool) -> Value {
    json!({
        "id": id,
        "model": id,
        "displayName": name,
        "description": description,
        "hidden": false,
        "supportedReasoningEfforts": [],
        "defaultReasoningEffort": null,
        "isDefault": is_default,
    })
}

/// Whether the binary resolves on its host, its version against the floor,
/// and the login probe. Shells out, so callers keep it off async workers.
pub(crate) fn status(spec: &dyn AcpSpec, runtime: &AcpRuntime) -> HarnessStatus {
    let host = &runtime.host;
    let config_dir = spec.config_dir(host);
    let floor = spec.version_floor().to_string();
    let label = spec.kind().label();
    let Some(path) = host.resolve_binary(&runtime.binary_str()) else {
        return HarnessStatus {
            available: false,
            path: None,
            version: None,
            config_dir,
            logged_in: None,
            protocol_floor: floor,
            message: Some(missing_message(spec, runtime)),
        };
    };
    let version = host
        .command(&path, &["--version"], None, &[], &[])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| spec.version_of(String::from_utf8_lossy(&output.stdout).trim()));
    let below_floor = version
        .as_deref()
        .is_some_and(|version| version_below(version, &floor));
    let logged_in = spec.logged_in(host, &path);
    HarnessStatus {
        available: !below_floor,
        message: below_floor.then(|| {
            format!(
                "{label} {} is older than {floor}, the oldest release this app drives. Update {label} and try again.",
                version.clone().unwrap_or_default()
            )
        }),
        path: Some(path),
        version,
        config_dir,
        logged_in,
        protocol_floor: floor,
    }
}

/// [`crate::codex::binary::missing_message_on`], reworded for this agent.
pub(crate) fn missing_message(spec: &dyn AcpSpec, runtime: &AcpRuntime) -> String {
    let binary = spec.default_binary();
    crate::codex::binary::missing_message_on(&runtime.host, &runtime.binary_str())
        .replace("Codex CLI", &format!("{} CLI", spec.kind().label()))
        .replace("codex binary", &format!("{binary} binary"))
        .replace("which codex", &format!("which {binary}"))
}
