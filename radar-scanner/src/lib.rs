// FEATURE: tree-sitter property-access scanner for TypeScript, Python, and Go.
use std::path::Path;

use serde::{Deserialize, Serialize};
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// Language enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Lang {
    TypeScript,
    Python,
    Go,
}

impl Lang {
    /// Infer language from a file extension. Returns `None` for unsupported types.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "ts" | "tsx" => Some(Lang::TypeScript),
            "py" => Some(Lang::Python),
            "go" => Some(Lang::Go),
            _ => None,
        }
    }

    fn ts_language(&self) -> tree_sitter::Language {
        match self {
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
        }
    }

    /// AST node kind that represents a property access expression.
    fn member_kind(&self) -> &'static str {
        match self {
            Lang::TypeScript => "member_expression",
            Lang::Python => "attribute",
            Lang::Go => "selector_expression",
        }
    }

    /// Grammar field name that holds the property/attribute name within the member node.
    fn property_field(&self) -> &'static str {
        match self {
            Lang::TypeScript => "property",
            Lang::Python => "attribute",
            Lang::Go => "field",
        }
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSiteRecord {
    pub file_path: String,
    pub line_number: usize,
    pub field_path: String,
}

// ---------------------------------------------------------------------------
// File scanner
// ---------------------------------------------------------------------------

/// Parse `content` for property accesses and return `(property_name, 1-indexed_line)` pairs.
pub fn scan_file(content: &[u8], lang: &Lang) -> Vec<(String, usize)> {
    let mut parser = Parser::new();
    if parser.set_language(&lang.ts_language()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut results = Vec::new();
    collect(&tree.root_node(), content, lang, &mut results);
    results
}

fn collect(
    node: &tree_sitter::Node<'_>,
    source: &[u8],
    lang: &Lang,
    out: &mut Vec<(String, usize)>,
) {
    if node.kind() == lang.member_kind() {
        if let Some(prop) = node.child_by_field_name(lang.property_field()) {
            if let Ok(name) = prop.utf8_text(source) {
                out.push((name.to_string(), prop.start_position().row + 1));
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect(&child, source, lang, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Directory walker
// ---------------------------------------------------------------------------

/// Walk `dir` recursively, skipping common non-source directories, and collect all
/// property accesses found in TypeScript, Python, and Go source files.
pub fn scan_directory(dir: &Path) -> Vec<CallSiteRecord> {
    let mut records = Vec::new();
    walk(dir, &mut records);
    records
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "vendor",
    "__pycache__",
    ".tox",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
];

fn walk(dir: &Path, records: &mut Vec<CallSiteRecord>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            walk(&path, records);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(lang) = Lang::from_extension(ext) {
                if let Ok(content) = std::fs::read(&path) {
                    let file_str = path.to_string_lossy().into_owned();
                    for (field_path, line_number) in scan_file(&content, &lang) {
                        records.push(CallSiteRecord {
                            file_path: file_str.clone(),
                            line_number,
                            field_path,
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_member_access() {
        let src = b"const id = response.userId;";
        let hits = scan_file(src, &Lang::TypeScript);
        assert!(
            hits.iter().any(|(name, _)| name == "userId"),
            "expected userId in {hits:?}"
        );
    }

    #[test]
    fn typescript_chained_access() {
        let src = b"const x = response.data.items;";
        let hits = scan_file(src, &Lang::TypeScript);
        let names: Vec<_> = hits.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"data"), "expected data in {names:?}");
        assert!(names.contains(&"items"), "expected items in {names:?}");
    }

    #[test]
    fn python_attribute_access() {
        let src = b"name = response.user_name";
        let hits = scan_file(src, &Lang::Python);
        assert!(
            hits.iter().any(|(name, _)| name == "user_name"),
            "expected user_name in {hits:?}"
        );
    }

    #[test]
    fn go_selector_access() {
        let src = b"package main\nfunc main() { x := resp.UserID }";
        let hits = scan_file(src, &Lang::Go);
        assert!(
            hits.iter().any(|(name, _)| name == "UserID"),
            "expected UserID in {hits:?}"
        );
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert!(Lang::from_extension("rs").is_none());
        assert!(Lang::from_extension("java").is_none());
        assert!(Lang::from_extension("").is_none());
    }

    #[test]
    fn known_extensions_recognized() {
        assert_eq!(Lang::from_extension("ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("tsx"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("py"), Some(Lang::Python));
        assert_eq!(Lang::from_extension("go"), Some(Lang::Go));
    }

    #[test]
    fn line_numbers_are_one_indexed() {
        let src = b"const a = x.foo;\nconst b = x.bar;";
        let hits = scan_file(src, &Lang::TypeScript);
        let foo_line = hits.iter().find(|(n, _)| n == "foo").map(|(_, l)| *l);
        let bar_line = hits.iter().find(|(n, _)| n == "bar").map(|(_, l)| *l);
        assert_eq!(foo_line, Some(1), "foo should be on line 1");
        assert_eq!(bar_line, Some(2), "bar should be on line 2");
    }
}
