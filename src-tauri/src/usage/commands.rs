//! What the usage views read: a breakdown for a thread, a project or the
//! whole home, and — where the harness can say — the exact composition of a
//! live thread's context.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::{AppHandle, State};

use super::ledger::ContextSnapshot;
use super::prompt_parts::{fit_parts, PromptPart, PromptPartKind};
use crate::storage::{self, CategoryTokens, UsageScope, UsageTokens};
use crate::AppState;

/// Prefix on the error `read_context_breakdown` returns when the thread's
/// harness cannot report its context. The frontend matches it and shows the
/// estimate instead.
pub(crate) const CONTEXT_BREAKDOWN_UNSUPPORTED: &str = "harness-unsupported:context_breakdown";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum UsageScopeArg {
    Thread { thread_id: String },
    Project { path: String },
    Global,
}

impl From<UsageScopeArg> for UsageScope {
    fn from(scope: UsageScopeArg) -> Self {
        match scope {
            UsageScopeArg::Thread { thread_id } => UsageScope::Thread(thread_id),
            UsageScopeArg::Project { path } => UsageScope::Project(path),
            UsageScopeArg::Global => UsageScope::Global,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CategoryTokensDto {
    pub system: u64,
    pub skills: u64,
    pub user: u64,
    pub tool: u64,
    pub output: u64,
    pub reasoning: u64,
    pub unattributed: u64,
}

impl From<CategoryTokens> for CategoryTokensDto {
    fn from(c: CategoryTokens) -> Self {
        Self {
            system: c.system,
            skills: c.skills,
            user: c.user,
            tool: c.tool,
            output: c.output,
            reasoning: c.reasoning,
            unattributed: c.unattributed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageTokensDto {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
}

impl From<UsageTokens> for UsageTokensDto {
    fn from(t: UsageTokens) -> Self {
        Self {
            input_tokens: t.input,
            cached_input_tokens: t.cached_input,
            cache_write_input_tokens: t.cache_write_input,
            output_tokens: t.output,
            reasoning_output_tokens: t.reasoning_output,
            total_tokens: t.total(),
        }
    }
}

/// Where a context composition came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CompositionSource {
    /// Estimated by the ledger from journaled items.
    Estimate,
    /// Reported by the harness for the live context.
    Harness,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextComposition {
    pub categories: CategoryTokensDto,
    pub total_tokens: u64,
    pub context_window: Option<u64>,
    pub source: CompositionSource,
    /// What the system prompt holds, when the harness can say; they add up
    /// to `categories.system`. `None` when nothing can name the parts.
    pub parts: Option<Vec<PromptPart>>,
    /// The estimated parts outgrew the measured system prompt and were
    /// scaled down to it.
    pub parts_scaled: bool,
}

impl From<ContextSnapshot> for ContextComposition {
    fn from(snapshot: ContextSnapshot) -> Self {
        let categories = snapshot.categories;
        Self {
            total_tokens: snapshot
                .context_tokens
                .unwrap_or_else(|| categories.total()),
            categories: categories.into(),
            context_window: snapshot.context_window,
            source: CompositionSource::Estimate,
            parts: None,
            parts_scaled: false,
        }
    }
}

impl ContextComposition {
    /// Attach estimated parts, reconciled with this composition's `system`.
    pub(crate) fn with_estimated_parts(mut self, parts: Vec<PromptPart>) -> Self {
        let fitted = fit_parts(parts, self.categories.system);
        self.parts = Some(fitted.parts);
        self.parts_scaled = fitted.scaled;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelUsage {
    pub model: Option<String>,
    pub harness: String,
    pub tokens: UsageTokensDto,
    pub cost_usd: Option<f64>,
    pub turns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadUsageSummary {
    pub thread_id: String,
    pub title: Option<String>,
    pub tokens: UsageTokensDto,
    pub categories: CategoryTokensDto,
    pub cost_usd: Option<f64>,
    pub last_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageBreakdown {
    pub tokens: UsageTokensDto,
    pub categories: CategoryTokensDto,
    /// Sum of what the harnesses reported; `None` when nothing was priced.
    pub reported_cost_usd: Option<f64>,
    /// Whether some of the tokens carry no reported cost and need estimating.
    pub unpriced_tokens: bool,
    pub turns: u64,
    pub by_model: Vec<ModelUsage>,
    pub by_thread: Vec<ThreadUsageSummary>,
    /// The thread's estimated context composition; thread scope only.
    pub context: Option<ContextComposition>,
}

/// Token usage summed over `scope`, optionally since a unix time.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_usage_breakdown(
    scope: UsageScopeArg,
    since: Option<i64>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<UsageBreakdown, String> {
    let ctx = state.ctx(&window);
    let thread_id = match &scope {
        UsageScopeArg::Thread { thread_id } => Some(thread_id.clone()),
        _ => None,
    };
    let stored = storage::read_usage_breakdown(&ctx.database(), &scope.into(), since).await?;
    let context = match thread_id {
        Some(thread_id) => ctx
            .usage
            .snapshot(&app, &ctx.home_key, &thread_id)
            .await
            .map(ContextComposition::from),
        None => None,
    };
    Ok(UsageBreakdown {
        tokens: stored.totals.tokens.into(),
        categories: stored.totals.attribution.into(),
        reported_cost_usd: stored.totals.cost_usd,
        unpriced_tokens: stored.totals.unpriced,
        turns: stored.totals.turns,
        by_model: stored
            .by_model
            .into_iter()
            .map(|row| ModelUsage {
                model: row.model,
                harness: row.harness,
                tokens: row.tokens.into(),
                cost_usd: row.cost_usd,
                turns: row.turns,
            })
            .collect(),
        by_thread: stored
            .by_thread
            .into_iter()
            .map(|row| ThreadUsageSummary {
                thread_id: row.thread_id,
                title: row.title,
                tokens: row.tokens.into(),
                categories: row.attribution.into(),
                cost_usd: row.cost_usd,
                last_at: row.last_at,
            })
            .collect(),
        context,
    })
}

/// The composition of a thread's context with what its system prompt holds:
/// exact from a live Claude process, or the ledger's estimate with the parts
/// sized from the Codex rollout file. Fails with
/// [`CONTEXT_BREAKDOWN_UNSUPPORTED`] when neither can say — a Claude thread
/// between processes, or a Codex thread with no rollout on disk or no
/// measured context yet.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_context_breakdown(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ContextComposition, String> {
    let ctx = state.ctx(&window);
    if let Some(reported) = ctx.claude.context_usage(&thread_id).await? {
        let estimate = ctx.usage.snapshot(&app, &ctx.home_key, &thread_id).await;
        return Ok(harness_composition(&reported, estimate.as_ref()));
    }
    if storage::thread_harness(&ctx.database(), &thread_id)
        .await?
        .is_some()
    {
        return Err(format!(
            "{CONTEXT_BREAKDOWN_UNSUPPORTED}: no live process can report this thread's context"
        ));
    }
    let local_home = ctx.runtime().local_home();
    let Some(path) = crate::codex::rollout::locate(&local_home, &thread_id) else {
        return Err(format!(
            "{CONTEXT_BREAKDOWN_UNSUPPORTED}: no rollout on disk for this thread"
        ));
    };
    let Some(snapshot) = ctx.usage.snapshot(&app, &ctx.home_key, &thread_id).await else {
        return Err(format!(
            "{CONTEXT_BREAKDOWN_UNSUPPORTED}: this thread's context has not been measured yet"
        ));
    };
    let parts = {
        let ctx = ctx.clone();
        tauri::async_runtime::spawn_blocking(move || ctx.rollouts.prompt_parts(&path))
            .await
            .map_err(|error| format!("Could not read the rollout: {error}"))??
    };
    Ok(ContextComposition::from(snapshot).with_estimated_parts(parts))
}

/// Map Claude's `get_context_usage` categories onto ours. Prompt-side
/// fixtures (system prompt, tools, agents, memory files) are the system
/// prompt; skills are skills; the conversation is one "Messages" figure,
/// split by the ledger's estimated ratio since the harness does not break it
/// down; free space and the autocompact buffer are not tokens in use.
pub(crate) fn harness_composition(
    reported: &Value,
    estimate: Option<&ContextSnapshot>,
) -> ContextComposition {
    let mut categories = CategoryTokens::default();
    let mut parts = Vec::new();
    let mut messages = 0;
    if let Some(list) = reported.get("categories").and_then(Value::as_array) {
        for category in list {
            let label = category
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = label.to_ascii_lowercase();
            let tokens = category.get("tokens").and_then(Value::as_u64).unwrap_or(0);
            // Deferred tools are loaded on demand and not in the context;
            // free space and the autocompact buffer are what is left of it.
            let deferred = category
                .get("isDeferred")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if deferred
                || name.contains("free")
                || name.contains("buffer")
                || name.contains("reserved")
            {
                continue;
            }
            if name.contains("skill") {
                categories.skills += tokens;
            } else if name.contains("message") {
                messages += tokens;
            } else if name.contains("system")
                || name.contains("tool")
                || name.contains("agent")
                || name.contains("memory")
                || name.contains("prompt")
            {
                categories.system += tokens;
                let kind = if name.contains("tool") {
                    PromptPartKind::Tools
                } else if name.contains("memory") {
                    PromptPartKind::Memory
                } else if name.contains("prompt") {
                    PromptPartKind::BaseInstructions
                } else {
                    PromptPartKind::Other
                };
                parts.push(PromptPart::exact(kind, label, tokens));
            } else {
                categories.unattributed += tokens;
            }
        }
    }
    parts.retain(|part| part.tokens > 0);
    parts.sort_by_key(|part| std::cmp::Reverse(part.tokens));
    // The conversation, split the way the estimate says it is composed.
    let ratio = estimate.map(|snapshot| snapshot.categories);
    let (user, tool, output) = match ratio {
        Some(r) if r.user + r.tool + r.output > 0 => {
            let sum = (r.user + r.tool + r.output) as u128;
            let share = |part: u64| ((messages as u128 * part as u128) / sum) as u64;
            let (user, tool) = (share(r.user), share(r.tool));
            (user, tool, messages - user - tool)
        }
        _ => (0, 0, 0),
    };
    if user + tool + output == messages {
        categories.user += user;
        categories.tool += tool;
        categories.output += output;
    } else {
        categories.unattributed += messages;
    }
    let total_tokens = reported
        .get("totalTokens")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| categories.total());
    ContextComposition {
        total_tokens,
        categories: categories.into(),
        context_window: reported.get("maxTokens").and_then(Value::as_u64),
        source: CompositionSource::Harness,
        parts: Some(parts),
        parts_scaled: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_categories_map_onto_ours_and_messages_follow_the_estimate() {
        let reported = json!({
            "totalTokens": 30000,
            "maxTokens": 200000,
            "categories": [
                {"name": "System prompt", "tokens": 3000},
                {"name": "System tools", "tokens": 12000},
                {"name": "MCP tools", "tokens": 1000},
                {"name": "Custom agents", "tokens": 500},
                {"name": "Memory files", "tokens": 1500},
                {"name": "Skills", "tokens": 2000},
                {"name": "Messages", "tokens": 10000},
                {"name": "Free space", "tokens": 150000},
                {"name": "Autocompact buffer", "tokens": 20000}
            ]
        });
        let estimate = ContextSnapshot {
            categories: CategoryTokens {
                user: 100,
                tool: 300,
                output: 100,
                ..Default::default()
            },
            ..Default::default()
        };
        let composition = harness_composition(&reported, Some(&estimate));
        assert_eq!(composition.source, CompositionSource::Harness);
        assert_eq!(composition.total_tokens, 30000);
        assert_eq!(composition.context_window, Some(200000));
        assert_eq!(composition.categories.system, 18000);
        assert_eq!(composition.categories.skills, 2000);
        assert_eq!(composition.categories.user, 2000);
        assert_eq!(composition.categories.tool, 6000);
        assert_eq!(composition.categories.output, 2000);
        assert_eq!(composition.categories.unattributed, 0);
    }

    #[test]
    fn a_recorded_claude_response_sums_to_its_own_total() {
        let reported: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/protocol/claude/context-usage/response.json"
        ))
        .unwrap();
        let composition = harness_composition(&reported, None);
        // Deferred tools and free space are not tokens in use: the categories
        // that remain add up to what the CLI itself reports (to a rounding).
        let sum = composition.categories.system
            + composition.categories.skills
            + composition.categories.unattributed;
        assert!(
            (sum as i64 - composition.total_tokens as i64).abs() <= 1,
            "{sum} vs {}",
            composition.total_tokens
        );
        assert_eq!(composition.categories.system, 6365 + 9965);
        assert_eq!(composition.categories.skills, 1605);
        assert_eq!(composition.categories.unattributed, 2747);
        assert_eq!(composition.context_window, Some(200_000));
    }

    #[test]
    fn claude_prompt_parts_are_exact_and_add_up_to_the_system_prompt() {
        let reported: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/protocol/claude/context-usage/response.json"
        ))
        .expect("fixture parses");
        let composition = harness_composition(&reported, None);
        let parts = composition.parts.expect("parts");
        assert!(parts
            .iter()
            .all(|p| p.source == super::super::prompt_parts::PartSource::Exact));
        assert_eq!(
            parts.iter().map(|p| p.tokens).sum::<u64>(),
            composition.categories.system
        );
        assert_eq!(parts[0].label, "System tools");
        assert_eq!(parts[0].kind, PromptPartKind::Tools);
        assert_eq!(parts[1].kind, PromptPartKind::BaseInstructions);
        assert!(
            !parts.iter().any(|p| p.label.contains("deferred")),
            "deferred tools are not in context"
        );
        assert!(!composition.parts_scaled);
    }

    #[test]
    fn without_an_estimate_the_conversation_is_unattributed() {
        let reported = json!({"categories": [{"name": "Messages", "tokens": 400}, {"name": "Weird", "tokens": 5}]});
        let composition = harness_composition(&reported, None);
        assert_eq!(composition.categories.unattributed, 405);
        assert_eq!(composition.total_tokens, 405);
        assert_eq!(composition.context_window, None);
    }
}
