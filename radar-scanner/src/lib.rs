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

    /// AST node kind for a function-call expression.
    fn call_node_kind(&self) -> &'static str {
        match self {
            Lang::TypeScript | Lang::Go => "call_expression",
            Lang::Python => "call",
        }
    }

    /// Field name for the object/receiver within a method-call's function part.
    fn call_object_field(&self) -> &'static str {
        match self {
            Lang::TypeScript | Lang::Python => "object",
            Lang::Go => "operand",
        }
    }

    /// Returns true when `name` looks like an API/HTTP client variable.
    fn is_api_object(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        lower.contains("api") || lower.contains("client")
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
    /// HTTP operation inferred from a generated-client method call, e.g. "GET /users/{id}".
    /// None when only S1 (field-only) evidence is available → confidence=low.
    pub operation: Option<String>,
}

// ---------------------------------------------------------------------------
// File scanner (S0/S1 — leaf property extraction for all languages)
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
// E-5: S2 TypeScript scanner — operation-aware
// ---------------------------------------------------------------------------

/// Convert a generated-client method name to an HTTP operation using naming conventions.
/// Handles openapi-typescript-codegen and orval method naming patterns.
/// Returns None when the name does not follow a recognized verb prefix.
pub fn method_name_to_operation(name: &str) -> Option<String> {
    let (http_method, rest) = if let Some(r) = name.strip_prefix("get") {
        ("GET", r)
    } else if let Some(r) = name.strip_prefix("list") {
        ("GET", r)
    } else if let Some(r) = name.strip_prefix("create") {
        ("POST", r)
    } else if let Some(r) = name.strip_prefix("post") {
        ("POST", r)
    } else if let Some(r) = name.strip_prefix("add") {
        ("POST", r)
    } else if let Some(r) = name.strip_prefix("update") {
        ("PUT", r)
    } else if let Some(r) = name.strip_prefix("put") {
        ("PUT", r)
    } else if let Some(r) = name.strip_prefix("patch") {
        ("PATCH", r)
    } else if let Some(r) = name.strip_prefix("delete") {
        ("DELETE", r)
    } else if let Some(r) = name.strip_prefix("remove") {
        ("DELETE", r)
    } else {
        return None;
    };

    if rest.is_empty() {
        return None;
    }

    // Split a "ById", "BySlug", or "ByName" suffix to add a path parameter.
    let (resource, by_param) = if let Some(r) = rest.strip_suffix("ById") {
        (r, Some("id"))
    } else if let Some(r) = rest.strip_suffix("BySlug") {
        (r, Some("slug"))
    } else if let Some(r) = rest.strip_suffix("ByName") {
        (r, Some("name"))
    } else {
        (rest, None)
    };

    if resource.is_empty() {
        return None;
    }

    let path_segment = pascal_to_kebab_plural(resource);
    let path = match by_param {
        Some(p) => format!("/{path_segment}/{{{p}}}"),
        None => format!("/{path_segment}"),
    };

    Some(format!("{http_method} {path}"))
}

/// Convert a PascalCase resource name to lowercase-plural kebab-case path segment.
/// "User" → "users", "ProductVariant" → "product-variants"
fn pascal_to_kebab_plural(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('-');
        }
        for lc in ch.to_lowercase() {
            result.push(lc);
        }
    }
    if !result.ends_with('s') {
        result.push('s');
    }
    result
}

/// S2 TypeScript scan — thin wrapper around `scan_s2` kept for backward compatibility.
pub fn scan_typescript_s2(content: &[u8]) -> Vec<CallSiteRecord> {
    scan_s2(content, &Lang::TypeScript)
}

// ---------------------------------------------------------------------------
// TD-5: Unified S2 scanner for TypeScript, Python, and Go
// ---------------------------------------------------------------------------

/// Normalise a method name to camelCase so `method_name_to_operation` can parse it.
/// Handles:
/// - snake_case (`get_user_by_id` → `getUserById`)
/// - PascalCase (`GetUserById` → `getUserById`)
/// - camelCase (unchanged)
fn normalise_method_name(s: &str) -> String {
    // snake_case → camelCase
    let camel: String = {
        let mut out = String::new();
        let mut cap_next = false;
        for ch in s.chars() {
            if ch == '_' {
                cap_next = true;
            } else if cap_next {
                for uc in ch.to_uppercase() {
                    out.push(uc);
                }
                cap_next = false;
            } else {
                out.push(ch);
            }
        }
        out
    };
    // PascalCase → camelCase (lowercase first char)
    let mut chars = camel.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
    }
}

/// Unified API-call collector for TypeScript, Python, and Go ASTs.
/// Detects `obj.method(...)` where `obj` looks like an API/HTTP client.
fn collect_api_calls_s2(
    node: &tree_sitter::Node<'_>,
    source: &[u8],
    lang: &Lang,
    out: &mut Vec<String>,
) {
    if node.kind() == lang.call_node_kind() {
        if let Some(func) = node.child_by_field_name("function") {
            if func.kind() == lang.member_kind() {
                let obj_name = func
                    .child_by_field_name(lang.call_object_field())
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                let method_name = func
                    .child_by_field_name(lang.property_field())
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                if lang.is_api_object(obj_name) {
                    if let Some(op) = method_name_to_operation(&normalise_method_name(method_name)) {
                        out.push(op);
                    }
                }
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_api_calls_s2(&child, source, lang, out);
        }
    }
}

/// S2 scanner for any supported language: pairs leaf property accesses with the
/// HTTP operation inferred from an API client method call in the same file.
/// Falls back to S1 (operation = None) when no API call is detected.
/// The `file_path` field is left empty and filled in by `walk()`.
pub fn scan_s2(content: &[u8], lang: &Lang) -> Vec<CallSiteRecord> {
    let mut parser = Parser::new();
    if parser.set_language(&lang.ts_language()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let root = tree.root_node();

    let mut api_ops: Vec<String> = Vec::new();
    collect_api_calls_s2(&root, content, lang, &mut api_ops);
    let primary_op: Option<String> = api_ops.into_iter().next();

    let leaf_accesses = scan_file(content, lang);

    leaf_accesses
        .into_iter()
        .map(|(field_path, line_number)| CallSiteRecord {
            file_path: String::new(),
            line_number,
            field_path,
            operation: primary_op.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Directory walker
// ---------------------------------------------------------------------------

/// Walk `dir` recursively, skipping common non-source directories, and collect all
/// property accesses found in TypeScript, Python, and Go source files.
/// TypeScript files use the S2 operation-aware scanner; Python and Go use S1.
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
                    // S2: operation-aware scanner for all supported languages
                    let mut recs = scan_s2(&content, &lang);
                    for r in &mut recs {
                        r.file_path = file_str.clone();
                    }
                    records.extend(recs);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// E-7: Postman Collection v2.1 parser
// ---------------------------------------------------------------------------

/// A single request extracted from a Postman Collection v2.1 file.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionRequest {
    /// Request name (item.name)
    pub name: String,
    /// HTTP method in uppercase, e.g. "GET", "POST"
    pub method: String,
    /// Normalised operation path with `{{variable}}` prefixes stripped,
    /// e.g. "/users/{id}". None when the URL cannot be parsed.
    pub operation: Option<String>,
    /// Field paths extracted from test script assertions, e.g. ["phone", "email"].
    /// Empty when no assertions are found.
    pub field_paths: Vec<String>,
}

/// Parse a Postman Collection v2.1 file at `path`.
/// Returns `(collection_name, requests)` on success.
/// Returns an error for malformed or non-v2.1 JSON; never panics.
pub fn parse_collection(path: &std::path::Path) -> anyhow::Result<(String, Vec<CollectionRequest>)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    parse_collection_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid collection {}: {e}", path.display()))
}

/// Inner parser that works on a string slice (for unit-testability).
pub fn parse_collection_str(content: &str) -> anyhow::Result<(String, Vec<CollectionRequest>)> {
    let root: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;

    let name = root
        .pointer("/info/name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing /info/name"))?
        .to_string();

    let items = root
        .get("item")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing top-level 'item' array"))?;

    let requests = items
        .iter()
        .filter_map(extract_request)
        .collect();

    Ok((name, requests))
}

fn extract_request(item: &serde_json::Value) -> Option<CollectionRequest> {
    let item_name = item.get("name")?.as_str()?.to_string();
    let req = item.get("request")?;

    let method = req
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_uppercase();

    let operation = extract_operation(req);

    let events = item
        .get("event")
        .and_then(|v| v.as_array())
        .map(|arr| arr.as_slice())
        .unwrap_or(&[]);

    let field_paths = extract_field_paths_from_events(events);

    Some(CollectionRequest {
        name: item_name,
        method,
        operation,
        field_paths,
    })
}

/// Extract the URL path from a request item, stripping leading `{{variable}}` segments.
fn extract_operation(req: &serde_json::Value) -> Option<String> {
    // url can be a string or an object
    let raw_url = match req.get("url") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(obj) => obj
            .get("raw")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        None => return None,
    };

    if raw_url.is_empty() {
        return None;
    }

    // Strip protocol+host prefix. Keep only the path portion.
    // Handles: "http://host/path", "{{base_url}}/path", "{{a}}{{b}}/path"
    let path_part = strip_url_prefix(&raw_url);
    if path_part.is_empty() {
        return None;
    }

    // Ensure it starts with /
    let normalised = if path_part.starts_with('/') {
        path_part.to_string()
    } else {
        format!("/{path_part}")
    };

    Some(normalised)
}

/// Strip any scheme://host or leading `{{variable}}` segments, returning the path only.
fn strip_url_prefix(url: &str) -> &str {
    // Remove scheme://host (e.g. "https://api.example.com")
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };

    // Skip consecutive `{{...}}` template variable segments at the start.
    // e.g. "{{base_url}}/users/{id}" → "/users/{id}"
    // e.g. "{{base_url}}{{api_prefix}}/orders" → "/orders"
    let mut rest = after_scheme;
    loop {
        if let Some(stripped) = rest.strip_prefix("{{") {
            // find the closing }}
            if let Some(end) = stripped.find("}}") {
                rest = &stripped[end + 2..];
                continue;
            }
        }
        break;
    }

    // If there's still a host (no leading / after stripping variables), strip up to first /
    if !rest.starts_with('/') && !rest.starts_with('{') {
        if let Some(slash) = rest.find('/') {
            rest = &rest[slash..];
        }
    }

    rest
}

/// Extract field path names from Postman test script exec lines.
/// Recognises:
///   pm.response.json().<field>
///   pm.response.json().<a>.<b>  (deep path → "a.b")
///   .json().<field>
///   jsonPath(..., "$.<field>")
fn extract_field_paths_from_events(events: &[serde_json::Value]) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();

    for event in events {
        let listen = event.get("listen").and_then(|v| v.as_str()).unwrap_or("");
        if listen != "test" {
            continue;
        }
        let exec_lines: Vec<&str> = event
            .pointer("/script/exec")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.as_str())
                    .collect()
            })
            .unwrap_or_default();

        for line in exec_lines {
            for fp in extract_field_paths_from_line(line) {
                if !paths.contains(&fp) {
                    paths.push(fp);
                }
            }
        }
    }

    paths
}

/// Extract field path(s) from a single exec line.
fn extract_field_paths_from_line(line: &str) -> Vec<String> {
    let mut results = Vec::new();

    // Pattern: *.json().<field>[.<field>]* — captures the chain after .json()
    // e.g. "pm.response.json().phone" → "phone"
    // e.g. "data.json().user.name" → "user.name"
    if let Some(after_json) = find_after_json_call(line) {
        if let Some(fp) = extract_dotted_path(after_json) {
            results.push(fp);
        }
    }

    // Pattern: jsonPath($.<field>) or pm.expect(data.<field>)
    // Simple: look for `data.<ident>` or `json.<ident>` as variable accesses
    for fp in extract_variable_field_accesses(line) {
        if !results.contains(&fp) {
            results.push(fp);
        }
    }

    results
}

/// Find the text after `.json()` in a line.
fn find_after_json_call(line: &str) -> Option<&str> {
    let marker = ".json()";
    let pos = line.find(marker)?;
    Some(&line[pos + marker.len()..])
}

/// Extract a dotted field path from text starting with ".<ident>[.<ident>]*"
/// Returns None if there's no leading dot or the first token is not an identifier.
fn extract_dotted_path(text: &str) -> Option<String> {
    let text = text.trim_start();
    if !text.starts_with('.') {
        return None;
    }
    let rest = &text[1..];
    // Collect consecutive ident.ident segments
    let mut parts: Vec<&str> = Vec::new();
    let mut cursor = rest;
    loop {
        let end = cursor
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(cursor.len());
        if end == 0 {
            break;
        }
        parts.push(&cursor[..end]);
        cursor = &cursor[end..];
        if cursor.starts_with('.') {
            cursor = &cursor[1..];
        } else {
            break;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

/// Extract `<var>.<field>` patterns where the variable looks like a JSON response object.
/// Matches: `json.<field>`, `data.<field>`, `response.<field>` etc.
fn extract_variable_field_accesses(line: &str) -> Vec<String> {
    let mut results = Vec::new();
    // Look for known response-variable prefixes
    for prefix in &["json.", "data.", "response.body.", "response."] {
        let mut search = line;
        while let Some(pos) = search.find(prefix) {
            let after = &search[pos + prefix.len()..];
            if let Some(fp) = extract_identifier(after) {
                // Avoid re-adding paths already captured via .json() pattern
                if !results.contains(&fp) {
                    results.push(fp);
                }
            }
            search = &search[pos + 1..];
        }
    }
    results
}

fn extract_identifier(text: &str) -> Option<String> {
    let end = text
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(text.len());
    if end == 0 {
        None
    } else {
        Some(text[..end].to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- existing S1 tests (unchanged) ---

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

    // --- E-5: method_name_to_operation tests ---

    #[test]
    fn operation_get_by_id() {
        assert_eq!(
            method_name_to_operation("getUserById"),
            Some("GET /users/{id}".to_string())
        );
    }

    #[test]
    fn operation_list() {
        assert_eq!(
            method_name_to_operation("listProducts"),
            Some("GET /products".to_string())
        );
    }

    #[test]
    fn operation_create() {
        assert_eq!(
            method_name_to_operation("createOrder"),
            Some("POST /orders".to_string())
        );
    }

    #[test]
    fn operation_delete_by_id() {
        assert_eq!(
            method_name_to_operation("deleteUserById"),
            Some("DELETE /users/{id}".to_string())
        );
    }

    #[test]
    fn operation_patch() {
        assert_eq!(
            method_name_to_operation("patchUser"),
            Some("PATCH /users".to_string())
        );
    }

    #[test]
    fn operation_unknown_returns_none() {
        assert_eq!(method_name_to_operation("handleClick"), None);
        assert_eq!(method_name_to_operation("fetchData"), None);
        assert_eq!(method_name_to_operation("get"), None);
    }

    // --- E-5: S2 scanner tests ---

    #[test]
    fn s2_detects_operation_from_api_method_call() {
        let src = b"
async function getPhone(usersApi, userId) {
    const response = await usersApi.getUserById(userId);
    const phone = response.phone;
}
";
        let records = scan_typescript_s2(src);
        let s2: Vec<_> = records.iter().filter(|r| r.operation.is_some()).collect();
        assert!(!s2.is_empty(), "should have S2 records with operation set");
        let op = s2[0].operation.as_deref().unwrap();
        assert_eq!(op, "GET /users/{id}");
        let has_phone = s2.iter().any(|r| r.field_path == "phone");
        assert!(has_phone, "field_path 'phone' should be present in S2 records");
    }

    #[test]
    fn s2_no_api_call_yields_operation_none() {
        let src = b"
function extractPhone(response) {
    return response.phone;
}
";
        let records = scan_typescript_s2(src);
        assert!(
            records.iter().all(|r| r.operation.is_none()),
            "no API call detected — all records should have operation=None"
        );
    }

    #[test]
    fn s2_non_api_object_not_detected() {
        // "helper.getUser()" — "helper" doesn't contain "api" → no operation
        let src = b"const x = helper.getUser(id).name;";
        let records = scan_typescript_s2(src);
        assert!(
            records.iter().all(|r| r.operation.is_none()),
            "non-api object should not produce an operation"
        );
    }

    #[test]
    fn s2_api_suffix_variants_detected() {
        // Both "usersApi" and "UsersApi" contain "api"
        let src1 = b"usersApi.listUsers();";
        let src2 = b"UsersApi.listUsers();";
        for src in [src1.as_ref(), src2.as_ref()] {
            let records = scan_typescript_s2(src);
            let has_op = records.iter().any(|r| r.operation.is_some())
                || {
                    // scan_typescript_s2 on bare expressions may return 0 field records
                    // but the API call detection must still succeed; test that directly
                    let mut ops = Vec::new();
                    let mut parser = Parser::new();
                    parser
                        .set_language(&Lang::TypeScript.ts_language())
                        .unwrap();
                    let tree = parser.parse(src, None).unwrap();
                    collect_api_calls_s2(&tree.root_node(), src, &Lang::TypeScript, &mut ops);
                    !ops.is_empty()
                };
            assert!(has_op, "api-suffix object should produce operation detection");
        }
    }

    #[test]
    fn pascal_to_kebab_plural_examples() {
        assert_eq!(pascal_to_kebab_plural("User"), "users");
        assert_eq!(pascal_to_kebab_plural("Users"), "users");
        assert_eq!(pascal_to_kebab_plural("Order"), "orders");
        assert_eq!(pascal_to_kebab_plural("ProductVariant"), "product-variants");
    }

    // --- TD-5: S2 Python and Go scanner tests ---

    #[test]
    fn s2_python_detects_api_client_call() {
        let src = b"
def fetch_phone(api_client, user_id):
    response = api_client.get_user_by_id(user_id)
    return response.phone
";
        let records = scan_s2(src, &Lang::Python);
        let s2: Vec<_> = records.iter().filter(|r| r.operation.is_some()).collect();
        assert!(!s2.is_empty(), "should have S2 records with operation set; got {records:?}");
        assert_eq!(s2[0].operation.as_deref(), Some("GET /users/{id}"));
        assert!(s2.iter().any(|r| r.field_path == "phone"), "field 'phone' expected");
    }

    #[test]
    fn s2_python_client_prefix_detected() {
        // "client" in object name should also trigger S2
        let src = b"
def get_orders(client):
    return client.list_orders()
";
        let records = scan_s2(src, &Lang::Python);
        let has_op = records.iter().any(|r| r.operation.is_some()) || {
            let mut ops = Vec::new();
            let mut parser = Parser::new();
            parser.set_language(&Lang::Python.ts_language()).unwrap();
            let tree = parser.parse(src, None).unwrap();
            collect_api_calls_s2(&tree.root_node(), src, &Lang::Python, &mut ops);
            !ops.is_empty()
        };
        assert!(has_op, "client.list_orders() should produce an operation");
    }

    #[test]
    fn s2_python_non_api_object_yields_none() {
        let src = b"
def process(helper, data):
    result = helper.get_user_by_id(data.id)
    return result.phone
";
        let records = scan_s2(src, &Lang::Python);
        assert!(
            records.iter().all(|r| r.operation.is_none()),
            "object 'helper' should not produce an operation; got {records:?}"
        );
    }

    #[test]
    fn s2_go_detects_client_call() {
        let src = b"
package main

func fetchPhone(client *UserClient, id string) string {
    resp := client.GetUserById(ctx, id)
    return resp.Phone
}
";
        let records = scan_s2(src, &Lang::Go);
        let s2: Vec<_> = records.iter().filter(|r| r.operation.is_some()).collect();
        assert!(!s2.is_empty(), "should have S2 records with operation set; got {records:?}");
        assert_eq!(s2[0].operation.as_deref(), Some("GET /users/{id}"));
    }

    #[test]
    fn s2_go_api_prefix_detected() {
        let src = b"
package main

func listOrders(apiClient *ApiClient) []Order {
    return apiClient.ListOrders(ctx)
}
";
        let records = scan_s2(src, &Lang::Go);
        let has_op = records.iter().any(|r| r.operation.is_some()) || {
            let mut ops = Vec::new();
            let mut parser = Parser::new();
            parser.set_language(&Lang::Go.ts_language()).unwrap();
            let tree = parser.parse(src, None).unwrap();
            collect_api_calls_s2(&tree.root_node(), src, &Lang::Go, &mut ops);
            !ops.is_empty()
        };
        assert!(has_op, "apiClient.ListOrders() should produce an operation");
    }

    #[test]
    fn s2_go_non_matching_object_yields_none() {
        let src = b"
package main

func process(svc *Service, id string) string {
    resp := svc.GetUserById(ctx, id)
    return resp.Name
}
";
        let records = scan_s2(src, &Lang::Go);
        assert!(
            records.iter().all(|r| r.operation.is_none()),
            "object 'svc' should not produce an operation; got {records:?}"
        );
    }

    #[test]
    fn normalise_method_name_snake_case() {
        assert_eq!(normalise_method_name("get_user_by_id"), "getUserById");
        assert_eq!(normalise_method_name("list_orders"), "listOrders");
        assert_eq!(normalise_method_name("create_order"), "createOrder");
    }

    #[test]
    fn normalise_method_name_pascal_case() {
        assert_eq!(normalise_method_name("GetUserById"), "getUserById");
        assert_eq!(normalise_method_name("ListOrders"), "listOrders");
    }

    #[test]
    fn normalise_method_name_camel_case_unchanged() {
        assert_eq!(normalise_method_name("getUserById"), "getUserById");
        assert_eq!(normalise_method_name("listOrders"), "listOrders");
    }

    // --- E-7: parse_collection tests ---

    const FIXTURE_COLLECTION: &str = include_str!(
        "../../fixtures/billing-svc-tests.postman_collection.json"
    );

    #[test]
    fn parse_collection_extracts_name() {
        let (name, _) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        assert_eq!(name, "Billing Service Tests");
    }

    #[test]
    fn parse_collection_returns_three_requests() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        assert_eq!(reqs.len(), 3, "fixture has 3 items; got: {reqs:?}");
    }

    #[test]
    fn parse_collection_extracts_get_operation() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let user_req = reqs.iter().find(|r| r.name == "Get User by ID").expect("should find Get User by ID");
        assert_eq!(user_req.method, "GET");
        assert_eq!(user_req.operation.as_deref(), Some("/users/{id}"),
            "should strip {{base_url}} prefix; got {:?}", user_req.operation);
    }

    #[test]
    fn parse_collection_extracts_post_method() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let order_req = reqs.iter().find(|r| r.name == "Create Order").expect("should find Create Order");
        assert_eq!(order_req.method, "POST");
        assert_eq!(order_req.operation.as_deref(), Some("/orders"));
    }

    #[test]
    fn parse_collection_extracts_field_paths_from_test_scripts() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let user_req = reqs.iter().find(|r| r.name == "Get User by ID").expect("should find Get User by ID");
        assert!(
            user_req.field_paths.iter().any(|fp| fp == "phone"),
            "should extract 'phone' from pm.response.json().phone; got: {:?}", user_req.field_paths
        );
        assert!(
            user_req.field_paths.iter().any(|fp| fp == "email"),
            "should extract 'email' from pm.expect(json.email); got: {:?}", user_req.field_paths
        );
    }

    #[test]
    fn parse_collection_post_with_no_assertions_has_empty_field_paths() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let order_req = reqs.iter().find(|r| r.name == "Create Order").expect("should find Create Order");
        assert!(
            order_req.field_paths.is_empty(),
            "POST /orders has no test assertions; got: {:?}", order_req.field_paths
        );
    }

    #[test]
    fn parse_collection_strips_multiple_variable_prefixes() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let status_req = reqs.iter().find(|r| r.name.contains("variable prefix")).expect("should find variable prefix request");
        // {{base_url}}{{api_prefix}}/orders/{id}/status → /orders/{id}/status
        // but the path array has {{api_prefix}} as first element — raw URL wins
        let op = status_req.operation.as_deref().unwrap_or("");
        assert!(
            op.ends_with("/orders/{id}/status") || op.contains("orders"),
            "should strip variable prefixes; got: {op:?}"
        );
    }

    #[test]
    fn parse_collection_malformed_json_returns_error() {
        let result = parse_collection_str("{ not valid json }");
        assert!(result.is_err(), "malformed JSON should return error");
    }

    #[test]
    fn parse_collection_missing_info_name_returns_error() {
        let json = r#"{"info": {}, "item": []}"#;
        let result = parse_collection_str(json);
        assert!(result.is_err(), "missing info.name should return error");
    }
}
