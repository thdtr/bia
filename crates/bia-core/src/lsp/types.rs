//! The parts of server replies BIA uses, read from JSON leniently: a field
//! missing or of an unexpected shape means "nothing", never an error.

use super::text::Range;
use serde_json::Value;

fn range(v: &Value) -> Option<Range> {
    serde_json::from_value(v.clone()).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
}

/// `textDocument/publishDiagnostics`: the file and its diagnostics.
pub fn diagnostics(params: &Value) -> Option<(String, Vec<Diagnostic>)> {
    let uri = params["uri"].as_str()?.to_owned();
    let list = params["diagnostics"].as_array()?;
    let diagnostics = list
        .iter()
        .filter_map(|d| {
            Some(Diagnostic {
                range: range(&d["range"])?,
                severity: match d["severity"].as_u64() {
                    Some(2) => Severity::Warning,
                    Some(3) => Severity::Info,
                    Some(4) => Severity::Hint,
                    _ => Severity::Error,
                },
                message: d["message"].as_str()?.to_owned(),
                source: d["source"].as_str().map(str::to_owned),
            })
        })
        .collect();
    Some((uri, diagnostics))
}

/// A definition reply: `Location`, `Location[]` or `LocationLink[]`.
pub fn locations(result: &Value) -> Vec<(String, Range)> {
    let one = |v: &Value| -> Option<(String, Range)> {
        if let Some(uri) = v["targetUri"].as_str() {
            let r = range(&v["targetSelectionRange"]).or_else(|| range(&v["targetRange"]))?;
            return Some((uri.to_owned(), r));
        }
        Some((v["uri"].as_str()?.to_owned(), range(&v["range"])?))
    };
    match result {
        Value::Array(items) => items.iter().filter_map(one).collect(),
        Value::Object(_) => one(result).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// A hover reply as plain text.
pub fn hover(result: &Value) -> Option<String> {
    fn text(v: &Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("value").and_then(Value::as_str).map(str::to_owned),
            Value::Array(items) => {
                let parts: Vec<String> = items.iter().filter_map(text).collect();
                (!parts.is_empty()).then(|| parts.join("\n\n"))
            }
            _ => None,
        }
    }
    let text = text(&result["contents"])?;
    let text = text.trim();
    (!text.is_empty()).then(|| strip_fences(text))
}

/// Markdown code fences say nothing on a plain card.
fn strip_fences(text: &str) -> String {
    text.lines().filter(|l| !l.trim_start().starts_with("```")).collect::<Vec<_>>().join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub detail: Option<String>,
    /// What to insert, replacing `edit_range` if the server gave one or
    /// else the word before the cursor.
    pub insert: String,
    pub edit_range: Option<Range>,
    /// What typing is matched against.
    pub filter: String,
    pub sort: String,
}

/// A completion reply: `CompletionItem[]` or a `CompletionList`.
pub fn completions(result: &Value) -> Vec<Completion> {
    let items = match result {
        Value::Array(items) => items,
        Value::Object(o) => match o.get("items") {
            Some(Value::Array(items)) => items,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let label = item["label"].as_str()?.to_owned();
            let edit = &item["textEdit"];
            let (edit_range, edit_text) = match edit["newText"].as_str() {
                Some(text) => (
                    range(&edit["range"]).or_else(|| range(&edit["insert"])),
                    Some(text.to_owned()),
                ),
                None => (None, None),
            };
            let insert = edit_text
                .or_else(|| item["insertText"].as_str().map(str::to_owned))
                .unwrap_or_else(|| label.clone());
            Some(Completion {
                detail: item["detail"].as_str().map(str::to_owned),
                insert,
                edit_range,
                filter: item["filterText"].as_str().map_or_else(|| label.clone(), str::to_owned),
                sort: item["sortText"].as_str().map_or_else(|| label.clone(), str::to_owned),
                label,
            })
        })
        .collect()
}

/// `TextEdit[]`: formatting replies.
pub fn text_edits(result: &Value) -> Vec<(Range, String)> {
    result
        .as_array()
        .map(|edits| {
            edits
                .iter()
                .filter_map(|e| Some((range(&e["range"])?, e["newText"].as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// A `WorkspaceEdit` (rename): edits per file URI.
pub fn workspace_edit(result: &Value) -> Vec<(String, Vec<(Range, String)>)> {
    let mut out = Vec::new();
    if let Some(changes) = result["changes"].as_object() {
        for (uri, edits) in changes {
            out.push((uri.clone(), text_edits(edits)));
        }
    }
    if let Some(changes) = result["documentChanges"].as_array() {
        for change in changes {
            if let Some(uri) = change["textDocument"]["uri"].as_str() {
                out.push((uri.to_owned(), text_edits(&change["edits"])));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::text::Position;
    use serde_json::json;

    fn r(l0: u32, c0: u32, l1: u32, c1: u32) -> Range {
        Range {
            start: Position { line: l0, character: c0 },
            end: Position { line: l1, character: c1 },
        }
    }

    #[test]
    fn replies() {
        let d = diagnostics(&json!({ "uri": "file:///a", "diagnostics": [
            { "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 5 } }, "severity": 2, "message": "unused" },
            { "range": "bad", "message": "skipped" }
        ]}))
        .unwrap();
        assert_eq!(
            d.1,
            [Diagnostic {
                range: r(1, 2, 1, 5),
                severity: Severity::Warning,
                message: "unused".into(),
                source: None
            }]
        );

        let loc = json!({ "uri": "file:///b", "range": { "start": { "line": 3, "character": 0 }, "end": { "line": 3, "character": 4 } } });
        assert_eq!(locations(&loc), [("file:///b".to_owned(), r(3, 0, 3, 4))]);
        let link = json!([{ "targetUri": "file:///c", "targetRange": loc["range"], "targetSelectionRange": loc["range"] }]);
        assert_eq!(locations(&link).len(), 1);
        assert!(locations(&Value::Null).is_empty());

        let h = json!({ "contents": { "kind": "markdown", "value": "```rust\nfn f()\n```\ndoc" } });
        assert_eq!(hover(&h).as_deref(), Some("fn f()\ndoc"));

        let c = completions(
            &json!({ "items": [{ "label": "len", "detail": "fn len()", "insertText": "len()" }, { "label": "x" }] }),
        );
        assert_eq!(c[0].insert, "len()");
        assert_eq!(c[1].insert, "x");

        let w = workspace_edit(
            &json!({ "changes": { "file:///a": [{ "range": r(0, 0, 0, 1), "newText": "y" }] } }),
        );
        assert_eq!(w, [("file:///a".to_owned(), vec![(r(0, 0, 0, 1), "y".to_owned())])]);
    }
}
