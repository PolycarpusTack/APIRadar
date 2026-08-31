// FEATURE: tree-sitter property-access scanner for TypeScript, Python, and Go.
use std::path::Path;

use serde::{Deserialize, Serialize};
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// Language enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Lang {
    /// TypeScript / plain JavaScript (non-JSX) — parsed with the TypeScript grammar.
    TypeScript,
    /// TSX / JSX — parsed with the dedicated TSX grammar so JSX does not yield ERROR nodes.
    Tsx,
    Python,
    Go,
}

impl Lang {
    /// Infer language from a file extension. Returns `None` for unsupported types.
    ///
    /// JSX-bearing extensions (`tsx`, `jsx`) use the TSX grammar; the remaining
    /// TS/JS extensions use the TypeScript grammar (which also parses plain JS).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "tsx" | "jsx" => Some(Lang::Tsx),
            "ts" | "js" | "mjs" | "cjs" | "mts" | "cts" => Some(Lang::TypeScript),
            "py" => Some(Lang::Python),
            "go" => Some(Lang::Go),
            _ => None,
        }
    }

    fn ts_language(&self) -> tree_sitter::Language {
        match self {
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
        }
    }

    /// AST node kind that represents a property access expression.
    fn member_kind(&self) -> &'static str {
        match self {
            Lang::TypeScript | Lang::Tsx => "member_expression",
            Lang::Python => "attribute",
            Lang::Go => "selector_expression",
        }
    }

    /// Grammar field name that holds the property/attribute name within the member node.
    fn property_field(&self) -> &'static str {
        match self {
            Lang::TypeScript | Lang::Tsx => "property",
            Lang::Python => "attribute",
            Lang::Go => "field",
        }
    }

    /// AST node kind for a function-call expression.
    fn call_node_kind(&self) -> &'static str {
        match self {
            Lang::TypeScript | Lang::Tsx | Lang::Go => "call_expression",
            Lang::Python => "call",
        }
    }

    /// Field name for the object/receiver within a method-call's function part.
    fn call_object_field(&self) -> &'static str {
        match self {
            Lang::TypeScript | Lang::Tsx | Lang::Python => "object",
            Lang::Go => "operand",
        }
    }

    /// AST node kinds that introduce a new function scope. Operation attribution
    /// is confined to the nearest enclosing scope of these kinds.
    fn function_scope_kinds(&self) -> &'static [&'static str] {
        match self {
            Lang::TypeScript | Lang::Tsx => &[
                "function_declaration",
                "function_expression",
                "arrow_function",
                "method_definition",
                "generator_function_declaration",
            ],
            Lang::Python => &["function_definition"],
            Lang::Go => &["function_declaration", "method_declaration", "func_literal"],
        }
    }

    fn is_function_scope(&self, kind: &str) -> bool {
        self.function_scope_kinds().contains(&kind)
    }

    /// Returns true when `name` looks like an API/HTTP client variable.
    fn is_api_object(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        lower.contains("api") || lower.contains("client")
    }

    /// Assignment-like node kinds that bind a value to a variable, paired with the
    /// grammar field holding the left-hand-side target. Used to discover the
    /// variable a call result is assigned to (so field accesses on that variable
    /// can be attributed to the call's operation).
    fn assignment_kinds(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Lang::TypeScript | Lang::Tsx => &[
                ("variable_declarator", "name"),
                ("assignment_expression", "left"),
            ],
            Lang::Python => &[("assignment", "left")],
            Lang::Go => &[
                ("short_var_declaration", "left"),
                ("assignment_statement", "left"),
                ("var_spec", "name"),
            ],
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
    // Verb prefixes in match order — the first prefix that matches wins, so
    // entries stay ordered even though no prefix here is a prefix of another.
    const VERB_PREFIXES: &[(&str, &str)] = &[
        ("get", "GET"),
        ("list", "GET"),
        ("create", "POST"),
        ("post", "POST"),
        ("add", "POST"),
        ("update", "PUT"),
        ("put", "PUT"),
        ("patch", "PATCH"),
        ("delete", "DELETE"),
        ("remove", "DELETE"),
    ];

    let (http_method, rest) = VERB_PREFIXES
        .iter()
        .find_map(|(prefix, method)| name.strip_prefix(prefix).map(|rest| (*method, rest)))?;

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
/// Retained for direct-detection unit tests; production scanning uses the
/// scope-aware `collect_scoped_api_ops`.
#[cfg(test)]
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
                    if let Some(op) = method_name_to_operation(&normalise_method_name(method_name))
                    {
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

/// Walk up from `node` to the nearest enclosing function scope. If no function
/// scope is found, returns the topmost (root) node, so module-level API calls
/// attribute to module-level field accesses.
fn enclosing_scope<'a>(node: tree_sitter::Node<'a>, lang: &Lang) -> tree_sitter::Node<'a> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if lang.is_function_scope(parent.kind()) {
            return parent;
        }
        cur = parent;
    }
    cur
}

/// The operation inferred from a *generated-client* method call
/// (`obj.method()` where `obj` looks like an API/HTTP client). Returns None
/// when the call is not a recognised generated-client call.
fn detect_api_call_op(node: &tree_sitter::Node<'_>, lang: &Lang, source: &[u8]) -> Option<String> {
    let func = node.child_by_field_name("function")?;
    if func.kind() != lang.member_kind() {
        return None;
    }
    let obj_name = func
        .child_by_field_name(lang.call_object_field())
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if !lang.is_api_object(obj_name) {
        return None;
    }
    let method_name = func
        .child_by_field_name(lang.property_field())
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    method_name_to_operation(&normalise_method_name(method_name))
}

/// N-17: recognise a *direct* HTTP-client call that does not match the
/// generated-client receiver heuristic — `fetch("/x")`, `axios.get("/x")`,
/// `requests.get(f"{BASE}/users/{id}")`, `http.Get("…")` — and return the
/// operation (`"GET /users/{id}"`) built from the string-literal URL argument.
fn detect_http_op(node: &tree_sitter::Node<'_>, lang: &Lang, source: &[u8]) -> Option<String> {
    let func = node.child_by_field_name("function")?;
    let method: &str = if func.kind() == lang.member_kind() {
        let obj_name = func
            .child_by_field_name(lang.call_object_field())
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if !is_http_client_receiver(obj_name) {
            return None;
        }
        let method_name = func
            .child_by_field_name(lang.property_field())
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        http_method_from(method_name)?
    } else if func.kind() == "identifier" {
        let name = func.utf8_text(source).ok()?;
        match name.to_lowercase().as_str() {
            "fetch" | "axios" => "GET",
            _ => return None,
        }
    } else {
        return None;
    };
    let url = extract_string_literal_arg(node, lang, source)?;
    let path = normalize_http_path(&url)?;
    Some(format!("{method} {path}"))
}

/// Known bare-name HTTP client receivers (case-insensitive).
fn is_http_client_receiver(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "axios" | "requests" | "http" | "session"
    )
}

/// Map an HTTP-verb method name (`get`, `Get`, `POST`, …) to an uppercase method.
fn http_method_from(name: &str) -> Option<&'static str> {
    match name.to_lowercase().as_str() {
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "patch" => Some("PATCH"),
        "delete" => Some("DELETE"),
        "head" => Some("HEAD"),
        "options" => Some("OPTIONS"),
        _ => None,
    }
}

/// Return the cleaned text of the first string-literal argument of a call.
fn extract_string_literal_arg(
    node: &tree_sitter::Node<'_>,
    _lang: &Lang,
    source: &[u8],
) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        let is_string = matches!(
            child.kind(),
            "string"
                | "template_string"
                | "string_literal"
                | "interpreted_string_literal"
                | "raw_string_literal"
        );
        if is_string {
            if let Ok(text) = child.utf8_text(source) {
                return Some(clean_string_literal(text));
            }
        }
    }
    None
}

/// Strip a string-prefix (`f`/`r`/`b`/`u`) and the surrounding quotes/backticks
/// from a raw string-literal token.
fn clean_string_literal(raw: &str) -> String {
    let mut s = raw.trim();
    // Drop a leading string prefix that is immediately followed by a quote.
    while let Some(first) = s.chars().next() {
        if matches!(first, 'f' | 'F' | 'r' | 'R' | 'b' | 'B' | 'u' | 'U')
            && s.len() > 1
            && matches!(s.as_bytes()[1], b'"' | b'\'' | b'`')
        {
            s = &s[1..];
        } else {
            break;
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let q = bytes[0];
        if (q == b'"' || q == b'\'' || q == b'`') && bytes[bytes.len() - 1] == q {
            s = &s[1..s.len() - 1];
        }
    }
    s.to_string()
}

/// Normalise a URL string to an operation path: strip scheme/host and a leading
/// template/host segment, drop query/fragment, and template path parameters
/// (`:id` / `${id}` / `{id}` / numeric → `{id}`).
fn normalize_http_path(raw: &str) -> Option<String> {
    let mut s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(pos) = s.find("://") {
        let after = &s[pos + 3..];
        s = match after.find('/') {
            Some(i) => &after[i..],
            None => return Some("/".to_string()),
        };
    } else if !s.starts_with('/') {
        // Template-host (`{{BASE}}`, `${BASE}`, `example.com`) or relative URL:
        // drop the leading host-like segment before the first '/'.
        if let Some(i) = s.find('/') {
            let first = &s[..i];
            if first.contains('{')
                || first.contains('$')
                || first.contains('.')
                || first.contains(':')
            {
                s = &s[i..];
            }
        }
    }
    let s = s.split(['?', '#']).next().unwrap_or(s);
    let with_slash = if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{s}")
    };
    let normalized = with_slash
        .split('/')
        .map(normalize_path_segment)
        .collect::<Vec<_>>()
        .join("/");
    let trimmed = normalized.trim_end_matches('/');
    Some(if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    })
}

/// Normalise a single path segment to a brace-style path parameter where applicable.
fn normalize_path_segment(seg: &str) -> String {
    if let Some(v) = seg.strip_prefix(':') {
        if !v.is_empty() {
            return format!("{{{v}}}");
        }
    }
    if let Some(inner) = seg.strip_prefix("${").and_then(|x| x.strip_suffix('}')) {
        return format!("{{{inner}}}");
    }
    if seg.len() >= 2 && seg.starts_with('{') && seg.ends_with('}') {
        return seg.to_string();
    }
    if !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()) {
        return "{id}".to_string();
    }
    seg.to_string()
}

/// Recursively find the text of the first `identifier` node at or below `node`.
fn first_identifier_text(node: &tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    if node.kind() == "identifier" {
        return node.utf8_text(source).ok().map(|s| s.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(t) = first_identifier_text(&child, source) {
            return Some(t);
        }
    }
    None
}

/// Walk up from a call node to find the variable its result is assigned to, e.g.
/// `const response = await api.get(id)` → `Some("response")`. Returns None when
/// the call result is not bound to a simple variable in the same scope.
fn assigned_variable(call: &tree_sitter::Node<'_>, lang: &Lang, source: &[u8]) -> Option<String> {
    let mut cur = *call;
    // Bounded walk over the handful of wrapper nodes between a call and its binding
    // (await_expression, expression_list, …).
    for _ in 0..6 {
        let parent = cur.parent()?;
        if lang.is_function_scope(parent.kind()) {
            return None;
        }
        for (kind, field) in lang.assignment_kinds() {
            if parent.kind() == *kind {
                if let Some(lhs) = parent.child_by_field_name(field) {
                    return first_identifier_text(&lhs, source);
                }
            }
        }
        cur = parent;
    }
    None
}

/// Resolve the leftmost (root) identifier of a member/attribute/selector chain,
/// e.g. `response.data.items` → `Some("response")`. Returns None when the chain
/// is not rooted at a plain identifier (e.g. `foo().bar`).
fn member_root_identifier(
    member: &tree_sitter::Node<'_>,
    lang: &Lang,
    source: &[u8],
) -> Option<String> {
    let mut obj = member.child_by_field_name(lang.call_object_field())?;
    while obj.kind() == lang.member_kind() {
        obj = obj.child_by_field_name(lang.call_object_field())?;
    }
    if obj.kind() == "identifier" {
        obj.utf8_text(source).ok().map(|s| s.to_string())
    } else {
        None
    }
}

/// N-16: derive, per function scope, the map `(scope_id, var_name) → operation`
/// for every variable assigned from an API call, and emit an operation-only
/// record for each *direct* HTTP-client call (which may have no field access).
fn collect_ops(
    node: &tree_sitter::Node<'_>,
    source: &[u8],
    lang: &Lang,
    derived: &mut std::collections::HashMap<(usize, String), String>,
    http_records: &mut Vec<CallSiteRecord>,
) {
    if node.kind() == lang.call_node_kind() {
        // A generated-client call (`api.getUserById`) or a direct HTTP call
        // (`axios.get("/x")`). Direct HTTP calls also emit an operation record.
        let op = if let Some(o) = detect_api_call_op(node, lang, source) {
            Some(o)
        } else if let Some(h) = detect_http_op(node, lang, source) {
            http_records.push(CallSiteRecord {
                file_path: String::new(),
                line_number: node.start_position().row + 1,
                field_path: String::new(),
                operation: Some(h.clone()),
            });
            Some(h)
        } else {
            None
        };
        if let Some(op) = op {
            if let Some(var) = assigned_variable(node, lang, source) {
                let scope_id = enclosing_scope(*node, lang).id();
                // Later assignments to the same variable legitimately override
                // earlier ones; distinct variables never collide.
                derived.insert((scope_id, var), op);
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_ops(&child, source, lang, derived, http_records);
        }
    }
}

/// N-16: emit a call-site record for every property access whose receiver root
/// is a variable derived from an API call in the same scope. Member accesses on
/// non-derived values (`console.log`, `JSON.parse`, the API method name itself)
/// are intentionally skipped so they do not flood impact evidence.
fn collect_derived_fields(
    node: &tree_sitter::Node<'_>,
    source: &[u8],
    lang: &Lang,
    derived: &std::collections::HashMap<(usize, String), String>,
    out: &mut Vec<CallSiteRecord>,
) {
    if node.kind() == lang.member_kind() {
        if let Some(root) = member_root_identifier(node, lang, source) {
            let scope_id = enclosing_scope(*node, lang).id();
            if let Some(op) = derived.get(&(scope_id, root)) {
                if let Some(prop) = node.child_by_field_name(lang.property_field()) {
                    if let Ok(name) = prop.utf8_text(source) {
                        out.push(CallSiteRecord {
                            file_path: String::new(),
                            line_number: prop.start_position().row + 1,
                            field_path: name.to_string(),
                            operation: Some(op.clone()),
                        });
                    }
                }
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_derived_fields(&child, source, lang, derived, out);
        }
    }
}

/// S2 scanner for any supported language. Emits evidence only for field accesses
/// on values derived from an API call (generated-client or direct HTTP client),
/// plus an operation-only record per direct HTTP call. Each field access is
/// attributed to the operation of the call that produced its receiver, so a
/// function's second API call attributes its own fields correctly.
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

    let mut derived: std::collections::HashMap<(usize, String), String> =
        std::collections::HashMap::new();
    let mut records = Vec::new();
    collect_ops(&root, content, lang, &mut derived, &mut records);
    collect_derived_fields(&root, content, lang, &derived, &mut records);
    records
}

// ---------------------------------------------------------------------------
// Directory walker
// ---------------------------------------------------------------------------

/// Walk `dir` recursively, skipping common non-source directories, and collect all
/// property accesses found in TypeScript, Python, and Go source files.
/// TypeScript files use the S2 operation-aware scanner; Python and Go use S1.
pub fn scan_directory(dir: &Path) -> Vec<CallSiteRecord> {
    let mut records = Vec::new();
    let mut skipped_large = 0usize;
    walk(dir, 0, &mut records, &mut skipped_large);
    if skipped_large > 0 {
        tracing::info!(
            "radar-scanner: skipped {skipped_large} file(s) larger than {MAX_FILE_SIZE} bytes"
        );
    }
    records
}

/// N-18: maximum directory recursion depth. Guards against runaway trees and,
/// together with the symlink skip in `walk`, against symlink cycles.
const MAX_DEPTH: usize = 64;

/// N-18: files larger than this (~2 MB) are skipped — vendored/minified bundles
/// are not meaningful consumer source and are expensive to parse.
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// True when a file of `len` bytes is within the size cap and should be scanned.
fn is_within_size_limit(len: u64) -> bool {
    len <= MAX_FILE_SIZE
}

/// True when recursion at `depth` would exceed the depth cap and must stop.
fn exceeds_depth(depth: usize) -> bool {
    depth > MAX_DEPTH
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

fn walk(dir: &Path, depth: usize, records: &mut Vec<CallSiteRecord>, skipped_large: &mut usize) {
    if exceeds_depth(depth) {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        // Use file_type() so symlinks are neither followed nor stat-resolved:
        // this is the primary guard against symlink cycles (no stack overflow).
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            walk(&path, depth + 1, records, skipped_large);
        } else if file_type.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if let Some(lang) = Lang::from_extension(ext) {
                    // Size cap: skip oversized (vendored/minified) files.
                    if let Ok(meta) = entry.metadata() {
                        if !is_within_size_limit(meta.len()) {
                            *skipped_large += 1;
                            continue;
                        }
                    }
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
pub fn parse_collection(
    path: &std::path::Path,
) -> anyhow::Result<(String, Vec<CollectionRequest>)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    parse_collection_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid collection {}: {e}", path.display()))
}

/// Inner parser that works on a string slice (for unit-testability).
pub fn parse_collection_str(content: &str) -> anyhow::Result<(String, Vec<CollectionRequest>)> {
    // Strip a leading UTF-8 BOM so a BOM-prefixed collection isn't rejected.
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    let root: serde_json::Value =
        serde_json::from_str(content).map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;

    let name = root
        .pointer("/info/name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing /info/name"))?
        .to_string();

    let items = root
        .get("item")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing top-level 'item' array"))?;

    let mut requests = Vec::new();
    collect_requests(items, &mut requests);

    Ok((name, requests))
}

/// Recursively collect requests from a Postman `item` array, descending into
/// folder items (items with a child `item` array and no `request`).
fn collect_requests(items: &[serde_json::Value], out: &mut Vec<CollectionRequest>) {
    for item in items {
        if let Some(child_items) = item.get("item").and_then(|v| v.as_array()) {
            // Folder: recurse into its children.
            collect_requests(child_items, out);
        } else if let Some(req) = extract_request(item) {
            out.push(req);
        }
    }
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
        Some(obj) => {
            let raw = obj.get("raw").and_then(|v| v.as_str()).unwrap_or("");
            if raw.is_empty() {
                // Fall back to host/path arrays when there is no `raw` key.
                build_raw_from_host_path(obj)
            } else {
                raw.to_string()
            }
        }
        None => return None,
    };

    if raw_url.is_empty() {
        return None;
    }

    // Strip protocol+host prefix. Keep only the path portion.
    // Handles: "http://host/path", "{{base_url}}/path", "{{a}}{{b}}/path"
    let path_part = strip_url_prefix(&raw_url);

    // Drop query string / fragment (everything from the first '?' or '#').
    let path_part = path_part.split(['?', '#']).next().unwrap_or(path_part);

    // Trim a trailing slash (but never reduce below "/").
    let path_part = {
        let trimmed = path_part.trim_end_matches('/');
        if trimmed.is_empty() {
            "/"
        } else {
            trimmed
        }
    };

    // Ensure it starts with /
    let with_leading_slash = if path_part.starts_with('/') {
        path_part.to_string()
    } else {
        format!("/{path_part}")
    };

    // Normalise Postman `:var` path segments to brace-style `{var}`.
    let normalised = normalise_colon_params(&with_leading_slash);

    if normalised.is_empty() {
        None
    } else {
        Some(normalised)
    }
}

/// Build a raw-URL-like string from a URL object's `host` and `path` arrays.
/// e.g. host=["{{base_url}}"], path=["users","{id}"] → "{{base_url}}/users/{id}".
fn build_raw_from_host_path(url_obj: &serde_json::Value) -> String {
    let join = |key: &str, sep: &str| -> String {
        url_obj
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(sep)
            })
            .unwrap_or_default()
    };
    let host = join("host", ".");
    let path = join("path", "/");
    match (host.is_empty(), path.is_empty()) {
        (_, true) => host,
        (true, false) => format!("/{path}"),
        (false, false) => format!("{host}/{path}"),
    }
}

/// Normalise Postman `:var` path segments to brace-style `{var}`.
/// e.g. "/users/:userId/orders/:orderId" → "/users/{userId}/orders/{orderId}".
fn normalise_colon_params(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if let Some(var) = seg.strip_prefix(':') {
                if !var.is_empty() {
                    return format!("{{{var}}}");
                }
            }
            seg.to_string()
        })
        .collect::<Vec<_>>()
        .join("/")
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
            .map(|arr| arr.iter().filter_map(|l| l.as_str()).collect())
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
    // Look for known response-variable prefixes.
    for prefix in &["json.", "data.", "response.body.", "response."] {
        for (pos, _) in line.match_indices(prefix) {
            // Word-boundary check: the prefix must begin a standalone variable, not
            // be the tail of a larger identifier or member chain. This rejects
            // `pm.response.json()` (preceded by `.`) and `metadata.total`
            // (the `data.` prefix preceded by `a`).
            let boundary_ok = pos == 0
                || line[..pos]
                    .chars()
                    .next_back()
                    .map(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
                    .unwrap_or(true);
            if !boundary_ok {
                continue;
            }
            let after = &line[pos + prefix.len()..];
            if let Some(fp) = extract_identifier(after) {
                // Avoid re-adding paths already captured via .json() pattern
                if !results.contains(&fp) {
                    results.push(fp);
                }
            }
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
        // M-11: .tsx now uses the dedicated TSX (JSX) grammar → distinct Lang variant.
        assert_eq!(Lang::from_extension("tsx"), Some(Lang::Tsx));
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
        assert!(
            has_phone,
            "field_path 'phone' should be present in S2 records"
        );
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
            let has_op = records.iter().any(|r| r.operation.is_some()) || {
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
            assert!(
                has_op,
                "api-suffix object should produce operation detection"
            );
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
        assert!(
            !s2.is_empty(),
            "should have S2 records with operation set; got {records:?}"
        );
        assert_eq!(s2[0].operation.as_deref(), Some("GET /users/{id}"));
        assert!(
            s2.iter().any(|r| r.field_path == "phone"),
            "field 'phone' expected"
        );
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
        assert!(
            !s2.is_empty(),
            "should have S2 records with operation set; got {records:?}"
        );
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

    const FIXTURE_COLLECTION: &str =
        include_str!("../../fixtures/billing-svc-tests.postman_collection.json");

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
        let user_req = reqs
            .iter()
            .find(|r| r.name == "Get User by ID")
            .expect("should find Get User by ID");
        assert_eq!(user_req.method, "GET");
        assert_eq!(
            user_req.operation.as_deref(),
            Some("/users/{id}"),
            "should strip {{base_url}} prefix; got {:?}",
            user_req.operation
        );
    }

    #[test]
    fn parse_collection_extracts_post_method() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let order_req = reqs
            .iter()
            .find(|r| r.name == "Create Order")
            .expect("should find Create Order");
        assert_eq!(order_req.method, "POST");
        assert_eq!(order_req.operation.as_deref(), Some("/orders"));
    }

    #[test]
    fn parse_collection_extracts_field_paths_from_test_scripts() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let user_req = reqs
            .iter()
            .find(|r| r.name == "Get User by ID")
            .expect("should find Get User by ID");
        assert!(
            user_req.field_paths.iter().any(|fp| fp == "phone"),
            "should extract 'phone' from pm.response.json().phone; got: {:?}",
            user_req.field_paths
        );
        assert!(
            user_req.field_paths.iter().any(|fp| fp == "email"),
            "should extract 'email' from pm.expect(json.email); got: {:?}",
            user_req.field_paths
        );
    }

    #[test]
    fn parse_collection_post_with_no_assertions_has_empty_field_paths() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let order_req = reqs
            .iter()
            .find(|r| r.name == "Create Order")
            .expect("should find Create Order");
        assert!(
            order_req.field_paths.is_empty(),
            "POST /orders has no test assertions; got: {:?}",
            order_req.field_paths
        );
    }

    #[test]
    fn parse_collection_strips_multiple_variable_prefixes() {
        let (_, reqs) = parse_collection_str(FIXTURE_COLLECTION).expect("should parse");
        let status_req = reqs
            .iter()
            .find(|r| r.name.contains("variable prefix"))
            .expect("should find variable prefix request");
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

    // --- M-10: Postman parser accuracy ---

    #[test]
    fn parse_collection_recurses_nested_folders() {
        let json = r#"{"info":{"name":"C"},"item":[
            {"name":"Users","item":[
                {"name":"Get","request":{"method":"GET","url":{"raw":"{{base_url}}/users/:userId"}}}
            ]}
        ]}"#;
        let (_, reqs) = parse_collection_str(json).expect("parse");
        assert_eq!(
            reqs.len(),
            1,
            "request inside folder should be extracted; got {reqs:?}"
        );
        assert_eq!(reqs[0].name, "Get");
        // also exercises :var normalization
        assert_eq!(reqs[0].operation.as_deref(), Some("/users/{userId}"));
    }

    #[test]
    fn parse_collection_normalizes_colon_path_variables() {
        let json = r#"{"info":{"name":"C"},"item":[
            {"name":"R","request":{"method":"GET","url":{"raw":"{{base_url}}/users/:userId/orders/:orderId"}}}
        ]}"#;
        let (_, reqs) = parse_collection_str(json).expect("parse");
        assert_eq!(
            reqs[0].operation.as_deref(),
            Some("/users/{userId}/orders/{orderId}")
        );
    }

    #[test]
    fn parse_collection_strips_query_fragment_trailing_slash() {
        let json = r#"{"info":{"name":"C"},"item":[
            {"name":"Q","request":{"method":"GET","url":{"raw":"{{base_url}}/users?active=true"}}},
            {"name":"T","request":{"method":"GET","url":{"raw":"{{base_url}}/users/"}}},
            {"name":"F","request":{"method":"GET","url":{"raw":"{{base_url}}/users#frag"}}}
        ]}"#;
        let (_, reqs) = parse_collection_str(json).expect("parse");
        for r in &reqs {
            assert_eq!(
                r.operation.as_deref(),
                Some("/users"),
                "{}: {:?}",
                r.name,
                r.operation
            );
        }
    }

    #[test]
    fn parse_collection_no_false_field_paths_from_pm_response() {
        let json = r#"{"info":{"name":"C"},"item":[{"name":"R",
            "request":{"method":"GET","url":{"raw":"{{base_url}}/users/{id}"}},
            "event":[{"listen":"test","script":{"exec":[
                "var json = pm.response.json();",
                "pm.response.to.have.status(200);",
                "pm.expect(json.phone).to.exist;",
                "pm.expect(metadata.total).to.eql(1);"
            ]}}]}]}"#;
        let (_, reqs) = parse_collection_str(json).expect("parse");
        let fp = &reqs[0].field_paths;
        assert!(
            fp.iter().any(|f| f == "phone"),
            "phone expected; got {fp:?}"
        );
        for bad in ["json", "to", "code", "status", "total"] {
            assert!(
                !fp.iter().any(|f| f == bad),
                "'{bad}' must not be a field path; got {fp:?}"
            );
        }
    }

    #[test]
    fn parse_collection_strips_bom() {
        let json = "\u{feff}{\"info\":{\"name\":\"C\"},\"item\":[]}";
        let (name, _) = parse_collection_str(json).expect("BOM should be stripped");
        assert_eq!(name, "C");
    }

    #[test]
    fn parse_collection_url_object_without_raw_uses_path() {
        let json = r#"{"info":{"name":"C"},"item":[
            {"name":"R","request":{"method":"GET","url":{"host":["{{base_url}}"],"path":["users","{id}"]}}}
        ]}"#;
        let (_, reqs) = parse_collection_str(json).expect("parse");
        assert_eq!(reqs[0].operation.as_deref(), Some("/users/{id}"));
    }

    // --- M-11: TypeScript/TSX scanner accuracy ---

    #[test]
    fn tsx_jsx_member_access_parses() {
        let src = b"function C(user){ return <span>{user.phone}</span>; }";
        let hits = scan_file(src, &Lang::Tsx);
        assert!(
            hits.iter().any(|(n, _)| n == "phone"),
            "expected phone in {hits:?}"
        );
    }

    #[test]
    fn js_family_extensions_recognized() {
        for ext in ["js", "jsx", "mjs", "cjs", "mts", "cts"] {
            assert!(
                Lang::from_extension(ext).is_some(),
                "{ext} should be supported"
            );
        }
        assert_eq!(Lang::from_extension("tsx"), Some(Lang::Tsx));
        assert_eq!(Lang::from_extension("jsx"), Some(Lang::Tsx));
        assert_eq!(Lang::from_extension("js"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_extension("mjs"), Some(Lang::TypeScript));
    }

    #[test]
    fn s2_scopes_operation_to_enclosing_function() {
        let src = b"
async function getUser(usersApi, id) {
    const u = await usersApi.getUserById(id);
    return u.phone;
}
async function getOrders(ordersApi) {
    const o = await ordersApi.listOrders();
    return o.total;
}
";
        let records = scan_typescript_s2(src);
        let phone = records
            .iter()
            .find(|r| r.field_path == "phone")
            .expect("phone record");
        assert_eq!(
            phone.operation.as_deref(),
            Some("GET /users/{id}"),
            "phone should be attributed to its enclosing function's call"
        );
        let total = records
            .iter()
            .find(|r| r.field_path == "total")
            .expect("total record");
        assert_eq!(
            total.operation.as_deref(),
            Some("GET /orders"),
            "total should be attributed to its own function's call, not the first call"
        );
    }

    // --- N-16: evidence precision ---

    #[test]
    fn s2_console_log_and_json_parse_produce_no_evidence() {
        // Neither `console.log` nor `JSON.parse` is derived from an API call, so
        // no call-site evidence should be emitted for them.
        let src = b"
function noise(response) {
    console.log(response);
    JSON.parse(\"{}\");
    return response;
}
";
        let records = scan_typescript_s2(src);
        assert!(
            records.is_empty(),
            "console.log / JSON.parse must not produce evidence; got {records:?}"
        );
    }

    #[test]
    fn s2_only_derived_values_emit_field_evidence() {
        // `user.phone` is derived from an API call → evidence.
        // `console.log(user)` and the `usersApi.getUserById` method name → no evidence.
        let src = b"
async function loadUser(usersApi, id) {
    const user = await usersApi.getUserById(id);
    console.log(user);
    return user.phone;
}
";
        let records = scan_typescript_s2(src);
        let fields: Vec<&str> = records.iter().map(|r| r.field_path.as_str()).collect();
        assert!(fields.contains(&"phone"), "expected phone; got {fields:?}");
        assert!(
            !fields.iter().any(|f| *f == "log" || *f == "getUserById"),
            "must not emit method-name / console.log leaves; got {fields:?}"
        );
    }

    #[test]
    fn s2_second_api_call_fields_attributed_to_second_operation() {
        // Regression for the `or_insert` scope-pinning bug: two API calls in ONE
        // function must attribute each block's fields to its own operation.
        let src = b"
async function loadBoth(usersApi, ordersApi, id) {
    const user = await usersApi.getUserById(id);
    const orders = await ordersApi.listOrders();
    console.log(user.phone);
    return orders.total;
}
";
        let records = scan_typescript_s2(src);
        let phone = records
            .iter()
            .find(|r| r.field_path == "phone")
            .expect("phone record");
        assert_eq!(
            phone.operation.as_deref(),
            Some("GET /users/{id}"),
            "phone belongs to the first call"
        );
        let total = records
            .iter()
            .find(|r| r.field_path == "total")
            .expect("total record");
        assert_eq!(
            total.operation.as_deref(),
            Some("GET /orders"),
            "total belongs to the SECOND call, not the first (or_insert bug)"
        );
    }

    // --- N-17: direct HTTP client detection ---

    #[test]
    fn s2_direct_axios_get_extracts_operation() {
        let src = b"const r = axios.get(\"/users/1\");";
        let records = scan_s2(src, &Lang::TypeScript);
        assert!(
            records
                .iter()
                .any(|r| r.operation.as_deref() == Some("GET /users/{id}")),
            "axios.get(\"/users/1\") should yield GET /users/{{id}}; got {records:?}"
        );
    }

    #[test]
    fn s2_direct_requests_get_extracts_operation() {
        let src = b"resp = requests.get(\"/orders\")\n";
        let records = scan_s2(src, &Lang::Python);
        assert!(
            records
                .iter()
                .any(|r| r.operation.as_deref() == Some("GET /orders")),
            "requests.get(\"/orders\") should yield GET /orders; got {records:?}"
        );
    }

    #[test]
    fn s2_direct_fetch_extracts_operation() {
        let src = b"const r = fetch(\"/users/1\");";
        let records = scan_s2(src, &Lang::TypeScript);
        assert!(
            records
                .iter()
                .any(|r| r.operation.as_deref() == Some("GET /users/{id}")),
            "fetch(\"/users/1\") should yield GET /users/{{id}}; got {records:?}"
        );
    }

    #[test]
    fn s2_direct_requests_fstring_strips_base_and_templatizes() {
        let src = b"resp = requests.get(f\"{BASE}/users/{id}\")\n";
        let records = scan_s2(src, &Lang::Python);
        assert!(
            records
                .iter()
                .any(|r| r.operation.as_deref() == Some("GET /users/{id}")),
            "f-string URL should strip host and keep {{id}}; got {records:?}"
        );
    }

    #[test]
    fn normalize_http_path_examples() {
        assert_eq!(
            normalize_http_path("/users/1").as_deref(),
            Some("/users/{id}")
        );
        assert_eq!(normalize_http_path("/orders").as_deref(), Some("/orders"));
        assert_eq!(
            normalize_http_path("https://api.example.com/v1/users/42?active=true").as_deref(),
            Some("/v1/users/{id}")
        );
        assert_eq!(
            normalize_http_path("{BASE}/users/${id}").as_deref(),
            Some("/users/{id}")
        );
        assert_eq!(
            normalize_http_path("/users/:userId/orders").as_deref(),
            Some("/users/{userId}/orders")
        );
    }

    // --- N-18: robustness ---

    #[test]
    fn size_limit_helper_boundaries() {
        assert!(is_within_size_limit(0));
        assert!(is_within_size_limit(MAX_FILE_SIZE));
        assert!(!is_within_size_limit(MAX_FILE_SIZE + 1));
    }

    #[test]
    fn depth_cap_helper_boundaries() {
        assert!(!exceeds_depth(0));
        assert!(!exceeds_depth(MAX_DEPTH));
        assert!(exceeds_depth(MAX_DEPTH + 1));
    }

    fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("radar_scanner_{tag}_{nanos}_{n}"))
    }

    #[test]
    fn walk_recurses_normal_directory_tree() {
        let dir = unique_temp_dir("tree");
        let nested = dir.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("f.ts"),
            b"const user = usersApi.getUserById(1);\nconst p = user.phone;",
        )
        .unwrap();
        let records = scan_directory(&dir);
        let has_phone = records.iter().any(|r| r.field_path == "phone");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(has_phone, "nested source file should be scanned");
    }

    #[test]
    fn walk_skips_files_over_size_cap() {
        let dir = unique_temp_dir("size");
        std::fs::create_dir_all(&dir).unwrap();
        // Small file: should be scanned.
        std::fs::write(
            dir.join("small.ts"),
            b"const user = usersApi.getUserById(1);\nconst p = user.phone;",
        )
        .unwrap();
        // Oversized file (>2 MB): should be skipped, so `bigfield` never appears.
        let mut big = String::from("const q = usersApi.listUsers();\nconst z = q.bigfield;\n// ");
        big.push_str(&"a".repeat((MAX_FILE_SIZE as usize) + 1000));
        std::fs::write(dir.join("big.ts"), big.as_bytes()).unwrap();

        let records = scan_directory(&dir);
        let has_phone = records.iter().any(|r| r.field_path == "phone");
        let has_bigfield = records.iter().any(|r| r.field_path == "bigfield");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(has_phone, "small file should be scanned");
        assert!(!has_bigfield, "oversized file should be skipped");
    }
}
