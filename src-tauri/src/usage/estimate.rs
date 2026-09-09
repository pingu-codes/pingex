//! Where a turn's tokens went, estimated.
//!
//! Neither harness says which part of the prompt its input tokens paid for.
//! What the app does know is what it put in front of the model: the user's
//! messages, the skills they invoked, the tool calls and their output, and
//! the replies that stay in context. Sizing those at roughly four characters
//! a token gives a composition of the context, and a turn's input tokens are
//! then split across it pro rata. Output and reasoning tokens are exact from
//! the wire and sit on top.
//!
//! The system prompt is never seen, so it is derived once: the part of the
//! first measured context that nothing else explains. The rules are spelled
//! out in `features/15-usage.md`; everything here is pure and unit-tested.

use serde_json::Value;

use crate::storage::{CategoryTokens, UsageTokens};

/// What an item contributes to the context, as sketched from its payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Sketch {
    /// Text the user typed, and the skills it invoked by path.
    User { chars: u64, skills: Vec<String> },
    /// A tool call and whatever came back from it.
    Tool { chars: u64 },
    /// The harness compacted the conversation; the messages are gone.
    Compaction,
}

/// About four characters to a token, rounded up.
pub(crate) fn tokens_for_chars(chars: u64) -> u64 {
    chars.div_ceil(4)
}

fn chars_of(value: Option<&Value>) -> u64 {
    match value {
        None | Some(Value::Null) => 0,
        Some(Value::String(text)) => text.chars().count() as u64,
        Some(other) => other.to_string().chars().count() as u64,
    }
}

/// Sketch a completed item. Agent messages and reasoning are deliberately
/// absent: their size is exact from the wire, so estimating it would count it
/// twice.
pub(crate) fn sketch_item(payload: &Value) -> Option<Sketch> {
    let kind = payload.get("type").and_then(Value::as_str)?;
    match kind {
        "userMessage" => {
            let mut chars = 0;
            let mut skills = Vec::new();
            if let Some(parts) = payload.get("content").and_then(Value::as_array) {
                for part in parts {
                    match part.get("type").and_then(Value::as_str) {
                        Some("text") => chars += chars_of(part.get("text")),
                        Some("skill") => {
                            if let Some(path) = part.get("path").and_then(Value::as_str) {
                                skills.push(path.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some(Sketch::User { chars, skills })
        }
        "commandExecution" => Some(Sketch::Tool {
            chars: chars_of(payload.get("command")) + chars_of(payload.get("aggregatedOutput")),
        }),
        "fileChange" => Some(Sketch::Tool {
            chars: payload
                .get("changes")
                .and_then(Value::as_array)
                .map(|changes| {
                    changes
                        .iter()
                        .map(|change| chars_of(change.get("diff")))
                        .sum()
                })
                .unwrap_or(0),
        }),
        "mcpToolCall"
        | "dynamicToolCall"
        | "collabAgentToolCall"
        | "webSearch"
        | "functionCallOutput"
        | "subAgentActivity" => Some(Sketch::Tool {
            chars: ["arguments", "input", "query", "prompt", "result", "output"]
                .iter()
                .map(|key| chars_of(payload.get(key)))
                .sum(),
        }),
        "contextCompaction" => Some(Sketch::Compaction),
        _ => None,
    }
}

/// What the context holds right now, by category, in tokens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Composition {
    pub(crate) system: u64,
    pub(crate) skills: u64,
    pub(crate) user: u64,
    pub(crate) tool: u64,
    pub(crate) output: u64,
    /// Whether `system` has been derived; it is derived once and kept.
    seeded: bool,
    /// Skills already sized, so a skill invoked twice is counted once — the
    /// harness inlines it once.
    skills_seen: Vec<String>,
}

impl Composition {
    /// The estimated categories only — what a request's input tokens are
    /// split across.
    fn estimated_total(&self) -> u64 {
        self.system + self.skills + self.user + self.tool + self.output
    }

    /// Fold one item in. `skill_tokens` sizes a skill by path (the file's
    /// length, or 0 when it cannot be read).
    pub(crate) fn absorb(&mut self, sketch: &Sketch, skill_tokens: impl Fn(&str) -> u64) {
        match sketch {
            Sketch::User { chars, skills } => {
                self.user += tokens_for_chars(*chars);
                for path in skills {
                    if self.skills_seen.iter().any(|seen| seen == path) {
                        continue;
                    }
                    self.skills_seen.push(path.clone());
                    self.skills += skill_tokens(path);
                }
            }
            Sketch::Tool { chars } => self.tool += tokens_for_chars(*chars),
            Sketch::Compaction => {
                self.user = 0;
                self.tool = 0;
                self.output = 0;
            }
        }
    }

    /// Derive the system prompt from the first context measurement: whatever
    /// `context_tokens` holds that the sketched items do not explain. Later
    /// calls are no-ops; the prompt does not change mid-thread.
    pub(crate) fn seed(&mut self, context_tokens: u64) {
        if self.seeded {
            return;
        }
        self.seeded = true;
        self.system =
            context_tokens.saturating_sub(self.skills + self.user + self.tool + self.output);
    }

    pub(crate) fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// Bring the estimate back under a measured context size. Items are sized
    /// generously (a diff's markup, JSON punctuation), so the estimate drifts
    /// upward; the measurement wins.
    pub(crate) fn fit(&mut self, context_tokens: u64) {
        let total = self.estimated_total();
        if total <= context_tokens || total == 0 {
            return;
        }
        let scale = |value: u64| ((value as u128 * context_tokens as u128) / total as u128) as u64;
        self.system = scale(self.system);
        self.skills = scale(self.skills);
        self.user = scale(self.user);
        self.tool = scale(self.tool);
        self.output = scale(self.output);
    }

    /// A reply the model gave stays in context for the turns that follow.
    pub(crate) fn add_output(&mut self, tokens: u64) {
        self.output += tokens;
    }

    /// Split a request's usage across the categories: input tokens pro rata
    /// over the composition, output and reasoning exact. The parts always sum
    /// to `tokens.total()`.
    pub(crate) fn attribute(&self, tokens: &UsageTokens) -> CategoryTokens {
        let reasoning = tokens.reasoning_output.min(tokens.output);
        let mut out = CategoryTokens {
            output: tokens.output - reasoning,
            reasoning,
            ..Default::default()
        };
        let total = self.estimated_total();
        if total == 0 {
            out.unattributed = tokens.input;
            return out;
        }
        let share = |value: u64| ((tokens.input as u128 * value as u128) / total as u128) as u64;
        out.system = share(self.system);
        out.skills = share(self.skills);
        out.user = share(self.user);
        out.tool = share(self.tool);
        // Prior replies are part of the prompt; the tokens re-reading them are
        // booked as output so the split still says what was in context.
        out.output += share(self.output);
        let placed = out.system
            + out.skills
            + out.user
            + out.tool
            + (out.output - (tokens.output - reasoning));
        let remainder = tokens.input - placed;
        // Rounding leftovers go to the largest bucket so nothing is invented.
        let largest = [
            (self.system, 0),
            (self.skills, 1),
            (self.user, 2),
            (self.tool, 3),
            (self.output, 4),
        ]
        .iter()
        .max_by_key(|(value, _)| *value)
        .map(|(_, index)| *index)
        .unwrap_or(0);
        match largest {
            0 => out.system += remainder,
            1 => out.skills += remainder,
            2 => out.user += remainder,
            3 => out.tool += remainder,
            _ => out.output += remainder,
        }
        out
    }

    /// The composition as categories, with the part of a measured context the
    /// estimate does not cover left unattributed.
    pub(crate) fn snapshot(&self, context_tokens: Option<u64>) -> CategoryTokens {
        let estimated = self.estimated_total();
        CategoryTokens {
            system: self.system,
            skills: self.skills,
            user: self.user,
            tool: self.tool,
            output: self.output,
            reasoning: 0,
            unattributed: context_tokens
                .map(|context| context.saturating_sub(estimated))
                .unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn no_skills(_: &str) -> u64 {
        0
    }

    #[test]
    fn four_chars_make_a_token_rounded_up() {
        assert_eq!(tokens_for_chars(0), 0);
        assert_eq!(tokens_for_chars(1), 1);
        assert_eq!(tokens_for_chars(4), 1);
        assert_eq!(tokens_for_chars(5), 2);
    }

    #[test]
    fn items_sketch_by_type() {
        let user = sketch_item(&json!({
            "type": "userMessage",
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "skill", "name": "deploy", "path": "/home/me/.codex/skills/deploy/SKILL.md"},
                {"type": "image", "url": "data:..."}
            ]
        }));
        assert_eq!(
            user,
            Some(Sketch::User {
                chars: 5,
                skills: vec!["/home/me/.codex/skills/deploy/SKILL.md".into()]
            })
        );
        assert_eq!(
            sketch_item(
                &json!({"type": "commandExecution", "command": "ls", "aggregatedOutput": "a\nb"})
            ),
            Some(Sketch::Tool { chars: 5 })
        );
        assert_eq!(
            sketch_item(
                &json!({"type": "fileChange", "changes": [{"diff": "+x"}, {"diff": "-yy"}]})
            ),
            Some(Sketch::Tool { chars: 5 })
        );
        assert_eq!(
            sketch_item(&json!({"type": "dynamicToolCall", "arguments": {"a": 1}, "output": "ok"})),
            Some(Sketch::Tool { chars: 9 })
        );
        assert_eq!(
            sketch_item(&json!({"type": "contextCompaction"})),
            Some(Sketch::Compaction)
        );
        assert_eq!(
            sketch_item(&json!({"type": "agentMessage", "text": "long"})),
            None
        );
        assert_eq!(
            sketch_item(&json!({"type": "reasoning", "summary": ["x"]})),
            None
        );
    }

    #[test]
    fn a_skill_is_sized_once_per_thread() {
        let mut comp = Composition::default();
        let sketch = Sketch::User {
            chars: 8,
            skills: vec!["/s/a".into()],
        };
        comp.absorb(&sketch, |_| 500);
        comp.absorb(&sketch, |_| 500);
        assert_eq!(comp.skills, 500);
        assert_eq!(comp.user, 4);
    }

    #[test]
    fn the_system_prompt_is_what_the_first_context_does_not_explain() {
        let mut comp = Composition::default();
        comp.absorb(
            &Sketch::User {
                chars: 400,
                skills: vec![],
            },
            no_skills,
        );
        comp.seed(12_100);
        assert_eq!(comp.system, 12_000);
        comp.seed(50_000);
        assert_eq!(comp.system, 12_000, "seeding happens once");
        let mut tiny = Composition::default();
        tiny.absorb(
            &Sketch::User {
                chars: 4_000,
                skills: vec![],
            },
            no_skills,
        );
        tiny.seed(10);
        assert_eq!(tiny.system, 0, "never negative");
    }

    #[test]
    fn input_is_split_pro_rata_and_sums_exactly() {
        let mut comp = Composition::default();
        comp.absorb(
            &Sketch::User {
                chars: 4_000,
                skills: vec!["/s".into()],
            },
            |_| 2_000,
        );
        comp.absorb(&Sketch::Tool { chars: 12_000 }, no_skills);
        comp.seed(20_000); // system = 20000 - 1000 - 2000 - 3000 = 14000
        let tokens = UsageTokens {
            input: 33_333,
            cached_input: 0,
            cache_write_input: 0,
            output: 100,
            reasoning_output: 40,
        };
        let attribution = comp.attribute(&tokens);
        assert_eq!(attribution.total(), tokens.total());
        assert_eq!(attribution.output, 60);
        assert_eq!(attribution.reasoning, 40);
        assert_eq!(attribution.unattributed, 0);
        // 14000/20000 of the input, plus the rounding remainder.
        assert!(attribution.system >= 23_333 && attribution.system <= 23_336);
        assert_eq!(attribution.skills, 3_333);
        assert_eq!(attribution.user, 1_666);
        assert_eq!(attribution.tool, 4_999);
    }

    #[test]
    fn without_a_composition_input_is_unattributed() {
        let comp = Composition::default();
        let attribution = comp.attribute(&UsageTokens {
            input: 500,
            output: 20,
            ..Default::default()
        });
        assert_eq!(attribution.unattributed, 500);
        assert_eq!(attribution.output, 20);
    }

    #[test]
    fn compaction_drops_the_conversation_but_keeps_the_prompt() {
        let mut comp = Composition::default();
        comp.absorb(
            &Sketch::User {
                chars: 400,
                skills: vec!["/s".into()],
            },
            |_| 300,
        );
        comp.absorb(&Sketch::Tool { chars: 400 }, no_skills);
        comp.add_output(50);
        comp.seed(1_000);
        comp.absorb(&Sketch::Compaction, no_skills);
        assert_eq!(comp.user, 0);
        assert_eq!(comp.tool, 0);
        assert_eq!(comp.output, 0);
        assert_eq!(comp.skills, 300);
        assert_eq!(comp.system, 450);
        assert_eq!(comp.snapshot(Some(2_000)).unattributed, 1_250);
    }

    #[test]
    fn a_measurement_smaller_than_the_estimate_scales_it_down() {
        let mut comp = Composition::default();
        comp.absorb(&Sketch::Tool { chars: 40_000 }, no_skills);
        comp.absorb(
            &Sketch::User {
                chars: 4_000,
                skills: vec![],
            },
            no_skills,
        );
        comp.seed(11_000);
        assert_eq!(comp.system, 0);
        comp.fit(5_500);
        assert_eq!(comp.tool, 5_000);
        assert_eq!(comp.user, 500);
        comp.fit(100_000);
        assert_eq!(comp.tool, 5_000, "a larger measurement leaves it alone");
    }

    #[test]
    fn a_snapshot_reports_the_unexplained_context() {
        let mut comp = Composition::default();
        comp.absorb(&Sketch::Tool { chars: 400 }, no_skills);
        comp.seed(1_000);
        let snapshot = comp.snapshot(Some(1_400));
        assert_eq!(snapshot.system, 900);
        assert_eq!(snapshot.tool, 100);
        assert_eq!(snapshot.unattributed, 400);
        assert_eq!(comp.snapshot(None).unattributed, 0);
    }
}
