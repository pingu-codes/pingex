//! What the system prompt holds: the Prompt parts of a Context composition
//! (`features/15-usage.md`, "What the system prompt holds").
//!
//! The ledger's `system` figure is one lump. A harness that can say what is
//! inside it — Claude's `get_context_usage`, or the record Codex keeps of the
//! text it sent — hands back a list of parts. This module owns the neutral
//! types and the one rule that keeps the list honest: it must add up to the
//! `system` slice of the same composition, with tool definitions as whatever
//! the named parts do not explain.

use serde::{Deserialize, Serialize};
use specta::Type;

/// What a Prompt part is. Harness-neutral: a driver maps its own vocabulary
/// onto these and the frontend never learns which harness answered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PromptPartKind {
    /// The harness's own prompt for the model.
    BaseInstructions,
    /// An AGENTS.md (or equivalent) file; `detail` names its directory.
    AgentsMd,
    /// The harness's skills index or instructions, not a skill the user invoked.
    Skills,
    /// Sandboxing and approval rules.
    Permissions,
    /// Working directory, shell, and the context-window figures.
    Environment,
    /// Memory files and developer instructions.
    Memory,
    /// App and connector instructions.
    Apps,
    /// Plugin instructions.
    Plugins,
    /// The summary a compaction left behind.
    Compaction,
    /// Tool definitions, and whatever else nothing names.
    Tools,
    Other,
}

/// Whether the harness reported a part's size or the app sized its text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PartSource {
    Exact,
    Estimated,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptPart {
    pub kind: PromptPartKind,
    pub label: String,
    pub tokens: u64,
    pub source: PartSource,
    /// Where it came from, when one kind has several: an AGENTS.md directory,
    /// the harness's own category name.
    pub detail: Option<String>,
}

impl PromptPart {
    pub(crate) fn estimated(kind: PromptPartKind, label: &str, tokens: u64) -> Self {
        Self {
            kind,
            label: label.to_string(),
            tokens,
            source: PartSource::Estimated,
            detail: None,
        }
    }

    pub(crate) fn exact(kind: PromptPartKind, label: &str, tokens: u64) -> Self {
        Self {
            kind,
            label: label.to_string(),
            tokens,
            source: PartSource::Exact,
            detail: Some(label.to_string()),
        }
    }
}

pub(crate) const REMAINDER_LABEL: &str = "Tool definitions and other";

/// Estimated parts reconciled with the `system` figure they sit inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FittedParts {
    pub parts: Vec<PromptPart>,
    /// The parts outgrew the measured system prompt and were scaled to fit.
    pub scaled: bool,
}

/// Make estimated `parts` add up to `system`. When they fall short the gap is
/// the tool definitions (never written down by Codex) and anything else
/// unnamed; when they exceed it — the ledger fitted the lump down to a
/// measurement — every part is scaled pro rata and nothing is left over.
/// Zero-sized parts are dropped; the order is by size, remainder last.
pub(crate) fn fit_parts(parts: Vec<PromptPart>, system: u64) -> FittedParts {
    let mut parts: Vec<PromptPart> = parts.into_iter().filter(|part| part.tokens > 0).collect();
    let sum: u64 = parts.iter().map(|part| part.tokens).sum();
    let mut scaled = false;
    if sum > system {
        scaled = true;
        for part in &mut parts {
            part.tokens = ((part.tokens as u128 * system as u128) / sum as u128) as u64;
        }
        parts.retain(|part| part.tokens > 0);
    }
    parts.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.kind.cmp(&b.kind)));
    let placed: u64 = parts.iter().map(|part| part.tokens).sum();
    if system > placed {
        parts.push(PromptPart::estimated(
            PromptPartKind::Tools,
            REMAINDER_LABEL,
            system - placed,
        ));
    }
    FittedParts { parts, scaled }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: PromptPartKind, tokens: u64) -> PromptPart {
        PromptPart::estimated(kind, "x", tokens)
    }

    #[test]
    fn the_gap_to_the_system_figure_is_the_tool_definitions() {
        let fitted = fit_parts(
            vec![
                part(PromptPartKind::BaseInstructions, 4_000),
                part(PromptPartKind::AgentsMd, 1_000),
            ],
            8_000,
        );
        assert!(!fitted.scaled);
        let sizes: Vec<(PromptPartKind, u64)> =
            fitted.parts.iter().map(|p| (p.kind, p.tokens)).collect();
        assert_eq!(
            sizes,
            vec![
                (PromptPartKind::BaseInstructions, 4_000),
                (PromptPartKind::AgentsMd, 1_000),
                (PromptPartKind::Tools, 3_000),
            ]
        );
        assert_eq!(fitted.parts[2].label, REMAINDER_LABEL);
    }

    #[test]
    fn parts_larger_than_the_system_figure_are_scaled_with_no_remainder() {
        let fitted = fit_parts(
            vec![
                part(PromptPartKind::BaseInstructions, 6_000),
                part(PromptPartKind::Skills, 2_000),
            ],
            4_000,
        );
        assert!(fitted.scaled);
        let total: u64 = fitted.parts.iter().map(|p| p.tokens).sum();
        assert!(
            total <= 4_000 && total >= 3_998,
            "sums to the system figure: {total}"
        );
        assert!(fitted.parts.iter().all(|p| p.kind != PromptPartKind::Tools));
        assert_eq!(fitted.parts[0].tokens, 3_000);
        assert_eq!(fitted.parts[1].tokens, 1_000);
    }

    #[test]
    fn empty_parts_and_an_exact_fit_leave_no_remainder() {
        assert!(fit_parts(vec![], 0).parts.is_empty());
        let exact = fit_parts(vec![part(PromptPartKind::BaseInstructions, 10)], 10);
        assert_eq!(exact.parts.len(), 1);
        assert!(!exact.scaled);
        let unknown = fit_parts(vec![], 500);
        assert_eq!(unknown.parts.len(), 1);
        assert_eq!(unknown.parts[0].kind, PromptPartKind::Tools);
    }
}
