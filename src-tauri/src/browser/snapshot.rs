//! Page snapshots the model can read cheaply: an indented outline of the
//! accessibility tree (`Accessibility.getFullAXTree`) in which every element
//! the agent can act on carries a short ref (`e12`).
//!
//! Refs are keyed by the DOM node's `backendDOMNodeId`, which Chrome keeps
//! for the node's whole life, so a ref means the same element across
//! snapshots until the document is replaced. After an action the tools send
//! only the lines that changed (see [`diff`]), not the whole page again.

use serde_json::Value;
use std::collections::HashMap;

/// Names longer than this are cut; a label is rarely useful past it.
const NAME_LIMIT: usize = 100;
const VALUE_LIMIT: usize = 80;

/// Roles that add structure but no meaning of their own: their children are
/// drawn in their place.
const TRANSPARENT_ROLES: &[&str] = &[
    "generic",
    "none",
    "presentation",
    "RootWebArea",
    "WebArea",
    "LineBreak",
    "InlineTextBox",
    "LayoutTable",
    "LayoutTableRow",
    "LayoutTableCell",
    "Section",
    "group",
    "Canvas",
    "Iframe",
    "IframePresentational",
    "LabelText",
    "MenuListPopup",
];

/// Editable fields whose inner text is their value, already on their line.
const VALUE_ROLES: &[&str] = &["textbox", "searchbox", "TextField", "spinbutton"];

/// Roles the agent can act on, which always get a ref.
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "checkbox",
    "radio",
    "combobox",
    "listbox",
    "option",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "tab",
    "switch",
    "slider",
    "spinbutton",
    "treeitem",
    "PopUpButton",
    "ToggleButton",
    "DisclosureTriangle",
    "TextField",
];

/// Properties worth spelling out, in the order they are written.
const SHOWN_PROPERTIES: &[&str] = &[
    "level", "checked", "pressed", "expanded", "selected", "disabled", "required", "invalid",
    "focused", "readonly",
];

/// Stable short refs for one tab's document.
#[derive(Debug, Default)]
pub(crate) struct RefMap {
    by_backend: HashMap<i64, u32>,
    by_ref: HashMap<u32, i64>,
    next: u32,
}

impl RefMap {
    fn ref_for(&mut self, backend: i64) -> u32 {
        if let Some(existing) = self.by_backend.get(&backend) {
            return *existing;
        }
        self.next += 1;
        self.by_backend.insert(backend, self.next);
        self.by_ref.insert(self.next, backend);
        self.next
    }

    pub(crate) fn backend(&self, reference: u32) -> Option<i64> {
        self.by_ref.get(&reference).copied()
    }

    /// Forget every ref: the document was replaced, so the backend ids are
    /// all new.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// `e12` → 12. Also accepts a bare number or `[e12]`.
pub(crate) fn parse_ref(text: &str) -> Option<u32> {
    let text = text.trim().trim_start_matches('[').trim_end_matches(']');
    let digits = text.strip_prefix('e').unwrap_or(text);
    digits.parse().ok()
}

struct Tree<'a> {
    by_id: HashMap<&'a str, &'a Value>,
}

fn str_field<'a>(node: &'a Value, key: &str) -> &'a str {
    node.get(key)
        .and_then(|field| field.get("value"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn clean(text: &str, limit: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let mut cut: String = collapsed.chars().take(limit).collect();
    cut.push('…');
    cut
}

fn property<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node.get("properties")?
        .as_array()?
        .iter()
        .find(|property| property.get("name").and_then(Value::as_str) == Some(name))?
        .get("value")?
        .get("value")
}

fn backend_id(node: &Value) -> Option<i64> {
    node.get("backendDOMNodeId").and_then(Value::as_i64)
}

/// Draw the tree rooted at `scope` (a backend node id), or the whole page.
pub(crate) fn render(nodes: &[Value], refs: &mut RefMap, scope: Option<i64>) -> Vec<String> {
    let tree = Tree {
        by_id: nodes
            .iter()
            .filter_map(|node| Some((node.get("nodeId")?.as_str()?, node)))
            .collect(),
    };
    let root = match scope {
        Some(backend) => nodes.iter().find(|node| backend_id(node) == Some(backend)),
        None => nodes
            .iter()
            .find(|node| node.get("parentId").is_none())
            .or(nodes.first()),
    };
    let mut lines = Vec::new();
    if let Some(root) = root {
        draw_children_of(&tree, &[root], "", 0, refs, &mut lines);
    }
    lines
}

fn children<'a>(tree: &Tree<'a>, node: &'a Value) -> Vec<&'a Value> {
    node.get("childIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter_map(|id| tree.by_id.get(id).copied())
                .collect()
        })
        .unwrap_or_default()
}

/// Draw `siblings` at `depth`, merging runs of plain text into one line.
fn draw_children_of<'a>(
    tree: &Tree<'a>,
    siblings: &[&'a Value],
    parent_name: &str,
    depth: usize,
    refs: &mut RefMap,
    lines: &mut Vec<String>,
) {
    let mut text_run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, lines: &mut Vec<String>| {
        if run.is_empty() {
            return;
        }
        let text = clean(&run.join(" "), NAME_LIMIT * 2);
        run.clear();
        if !text.is_empty() && text != parent_name {
            lines.push(format!("{}- text: {text}", "  ".repeat(depth)));
        }
    };
    for node in siblings {
        let ignored = node
            .get("ignored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let role = str_field(node, "role");
        if ignored || role == "InlineTextBox" {
            if ignored {
                let kids = children(tree, node);
                flush(&mut text_run, lines);
                draw_children_of(tree, &kids, parent_name, depth, refs, lines);
            }
            continue;
        }
        if role == "StaticText" {
            text_run.push(str_field(node, "name").to_string());
            continue;
        }
        flush(&mut text_run, lines);
        draw_node(tree, node, parent_name, depth, refs, lines);
    }
    flush(&mut text_run, lines);
}

fn draw_node<'a>(
    tree: &Tree<'a>,
    node: &'a Value,
    parent_name: &str,
    depth: usize,
    refs: &mut RefMap,
    lines: &mut Vec<String>,
) {
    let role = str_field(node, "role");
    let name = clean(str_field(node, "name"), NAME_LIMIT);
    let kids = children(tree, node);
    let focusable = property(node, "focusable").and_then(Value::as_bool) == Some(true);
    let interactive = INTERACTIVE_ROLES.contains(&role)
        || (focusable && !TRANSPARENT_ROLES.contains(&role) && role != "RootWebArea");
    // The page root's name is the title, which the tools already print.
    if role == "RootWebArea"
        || (TRANSPARENT_ROLES.contains(&role) && name.is_empty() && !interactive)
    {
        draw_children_of(tree, &kids, parent_name, depth, refs, lines);
        return;
    }
    let mut line = format!("{}- {role}", "  ".repeat(depth));
    if !name.is_empty() {
        line.push_str(&format!(" \"{}\"", name.replace('"', "'")));
    }
    let value = clean(str_field(node, "value"), VALUE_LIMIT);
    if !value.is_empty() && value != name {
        line.push_str(&format!(" = \"{}\"", value.replace('"', "'")));
    }
    let mut shown = Vec::new();
    for key in SHOWN_PROPERTIES {
        match property(node, key) {
            Some(Value::Bool(true)) => shown.push((*key).to_string()),
            Some(Value::String(text)) if text != "false" && !text.is_empty() => {
                shown.push(format!("{key}={text}"))
            }
            Some(Value::Number(number)) => shown.push(format!("{key}={number}")),
            _ => {}
        }
    }
    if !shown.is_empty() {
        line.push_str(&format!(" [{}]", shown.join(", ")));
    }
    if interactive {
        if let Some(backend) = backend_id(node) {
            line.push_str(&format!(" e{}", refs.ref_for(backend)));
        }
    }
    let mut below = Vec::new();
    if !VALUE_ROLES.contains(&role) {
        draw_children_of(tree, &kids, &name, depth + 1, refs, &mut below);
    }
    // An empty container (a `paragraph` with nothing in it) says nothing.
    if !interactive && name.is_empty() && value.is_empty() && shown.is_empty() && below.is_empty() {
        return;
    }
    lines.push(line);
    lines.extend(below);
}

/// The changed lines between two snapshots (`+ ` added, `- ` removed), or
/// `None` when the snapshots are too large to compare.
pub(crate) fn diff(old: &[String], new: &[String]) -> Option<Vec<String>> {
    const LIMIT: usize = 4000;
    if old.len() > LIMIT || new.len() > LIMIT {
        return None;
    }
    // Trim the common prefix and suffix first: most actions change a small
    // window, which keeps the LCS table small.
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let a = &old[prefix..old.len() - suffix];
    let b = &new[prefix..new.len() - suffix];
    if a.len() * b.len() > 4_000_000 {
        return None;
    }
    let mut table = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || table[i][j + 1] >= table[i + 1][j]) {
            out.push(format!("+ {}", b[j].trim_start()));
            j += 1;
        } else {
            out.push(format!("- {}", a[i].trim_start()));
            i += 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, role: &str, name: &str, children: &[&str], backend: i64) -> Value {
        json!({
            "nodeId": id, "ignored": false,
            "role": {"type": "role", "value": role},
            "name": {"type": "computedString", "value": name},
            "childIds": children, "backendDOMNodeId": backend,
        })
    }

    fn page() -> Vec<Value> {
        let mut heading = node("3", "heading", "Sign in", &["4"], 30);
        heading["properties"] =
            json!([{"name": "level", "value": {"type": "integer", "value": 1}}]);
        let mut email = node("6", "textbox", "Email", &[], 60);
        email["value"] = json!({"type": "string", "value": "me@example.com"});
        email["properties"] = json!([{"name": "focusable", "value": {"type": "booleanOrUndefined", "value": true}}, {"name": "focused", "value": {"type": "booleanOrUndefined", "value": true}}]);
        let mut root = node("1", "RootWebArea", "Sign in", &["2"], 10);
        root.as_object_mut().unwrap().remove("parentId");
        vec![
            root,
            node("2", "generic", "", &["3", "5", "6", "7", "9"], 20),
            heading,
            node("4", "StaticText", "Sign in", &[], 40),
            node("5", "StaticText", "Use your   account", &[], 50),
            email,
            node("7", "button", "Continue", &["8"], 70),
            node("8", "StaticText", "Continue", &[], 80),
            json!({"nodeId": "9", "ignored": true, "childIds": ["10"]}),
            node("10", "link", "Help", &[], 100),
        ]
    }

    #[test]
    fn snapshots_are_compact_outlines_with_refs_on_actionable_elements() {
        let mut refs = RefMap::default();
        let lines = render(&page(), &mut refs, None);
        assert_eq!(
            lines,
            vec![
                "- heading \"Sign in\" [level=1]",
                "- text: Use your account",
                "- textbox \"Email\" = \"me@example.com\" [focused] e1",
                "- button \"Continue\" e2",
                "- link \"Help\" e3",
            ]
        );
        assert_eq!(refs.backend(1), Some(60));
        assert_eq!(refs.backend(3), Some(100));
        // The same element keeps its ref on the next snapshot.
        let again = render(&page(), &mut refs, None);
        assert_eq!(again, lines);
        assert_eq!(
            render(&page(), &mut refs, Some(70)),
            vec!["- button \"Continue\" e2"]
        );
    }

    #[test]
    fn empty_containers_and_echoed_field_text_are_left_out() {
        let mut root = node("1", "RootWebArea", "", &["2", "3", "5"], 1);
        root.as_object_mut().unwrap().remove("parentId");
        let mut field = node("3", "textbox", "Name", &["4"], 3);
        field["value"] = json!({"type": "string", "value": "Ada"});
        let nodes = vec![
            root,
            node("2", "paragraph", "", &[], 2),
            field,
            node("4", "StaticText", "Ada", &[], 4),
            node("5", "LabelText", "", &["6"], 5),
            node("6", "StaticText", "Size", &[], 6),
        ];
        let lines = render(&nodes, &mut RefMap::default(), None);
        assert_eq!(
            lines,
            vec!["- textbox \"Name\" = \"Ada\" e1", "- text: Size"]
        );
    }

    #[test]
    fn refs_parse_with_or_without_the_prefix() {
        assert_eq!(parse_ref("e12"), Some(12));
        assert_eq!(parse_ref("[e3]"), Some(3));
        assert_eq!(parse_ref("7"), Some(7));
        assert_eq!(parse_ref("button"), None);
    }

    #[test]
    fn diffs_carry_only_the_changed_lines() {
        let lines = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let old = lines(&[
            "- a", "- b", "- c", "- d", "- e", "- f", "- g", "- h", "- i", "- j",
        ]);
        let new = lines(&[
            "- a", "- b", "- C", "- d", "- e", "- f", "- g", "- h", "- i", "- j", "- k",
        ]);
        assert_eq!(diff(&old, &new), Some(lines(&["+ - C", "- - c", "+ - k"])));
        assert_eq!(diff(&old, &old), Some(vec![]));
    }
}
