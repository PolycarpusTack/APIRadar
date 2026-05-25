use serde_json::{json, Value};
use uuid::Uuid;

/// Compute a deterministic evidence ID for collection file evidence.
/// Stable across re-scans and server restarts → enables idempotent insert.
pub(crate) fn collection_evidence_id(
    consumer_id: &str,
    service_id: &str,
    operation: &str,
    field_path: &str,
) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("collection_file:{consumer_id}:{service_id}:{operation}:{field_path}").as_bytes(),
    )
    .to_string()
}

/// Extract a named string attribute from an OTLP attribute array.
pub(crate) fn otlp_attr(attrs: &[Value], key: &str) -> Option<String> {
    attrs.iter().find_map(|a| {
        if a.get("key")?.as_str()? == key {
            a.get("value")?.get("stringValue")?.as_str().map(|s| s.to_owned())
        } else {
            None
        }
    })
}

/// Normalise an HTTP path to a route-like form by replacing pure numeric segments
/// with `{id}` so that `/users/123` and `/users/456` collapse to `/users/{id}`.
pub(crate) fn normalise_path(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg.chars().all(|c| c.is_ascii_digit()) && !seg.is_empty() {
                "{id}".to_string()
            } else {
                seg.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Check whether a field path matches any deny-list pattern (comma-separated globs).
pub(crate) fn field_in_deny_list(field: &str, deny_list: &str) -> bool {
    if deny_list.is_empty() { return false; }
    deny_list.split(',').any(|pat| path_matches(pat.trim(), field))
}

/// Determine whether this event should be kept given the sample rate [0.0, 1.0].
pub(crate) fn sample_keep(rate: f64) -> bool {
    if rate >= 1.0 { return true; }
    if rate <= 0.0 { return false; }
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1);
    (ns as f64 / u32::MAX as f64) < rate
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "safe" => 0,
        "non_breaking_risky" => 1,
        "breaking" => 2,
        _ => 0,
    }
}

/// Returns true if `override_sev` is strictly less severe than `current_sev`.
/// Rules may only relax (downgrade) severity, never tighten it.
pub(crate) fn is_severity_downgrade(current: &str, to: &str) -> bool {
    severity_rank(to) < severity_rank(current)
}

/// Match a dot-separated field path against a glob pattern where:
/// - `*`  matches exactly one path segment (no dots)
/// - `**` matches zero or more path segments
/// - An empty/None pattern matches everything
pub(crate) fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let pat: Vec<&str> = pattern.split('.').collect();
    let parts: Vec<&str> = path.split('.').collect();
    glob_match(&pat, &parts)
}

fn glob_match(pat: &[&str], path: &[&str]) -> bool {
    if pat.is_empty() {
        return path.is_empty();
    }
    if pat[0] == "**" {
        for i in 0..=path.len() {
            if glob_match(&pat[1..], &path[i..]) {
                return true;
            }
        }
        return false;
    }
    if path.is_empty() {
        return false;
    }
    if pat[0] == "*" || pat[0] == path[0] {
        return glob_match(&pat[1..], &path[1..]);
    }
    false
}

/// Apply org evolution rules to a list of change JSON objects, returning the
/// same objects with optionally overridden `severity` and an `applied_rule` field.
pub(crate) fn apply_evolution_rules(
    changes: Vec<Value>,
    rules: &[(String, String, Option<String>, String, String)],
) -> Vec<Value> {
    // rules: (id, name, path_pattern, change_kind, severity_override)
    changes
        .into_iter()
        .map(|mut c| {
            let kind = c.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let path = c.get("path").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let current_sev = c.get("severity").and_then(|v| v.as_str()).unwrap_or("").to_owned();

            for (id, name, pat, rule_kind, override_sev) in rules {
                if rule_kind != &kind { continue; }
                let pattern = pat.as_deref().unwrap_or("");
                if !path_matches(pattern, &path) { continue; }
                if !is_severity_downgrade(&current_sev, override_sev) { continue; }
                let original = current_sev.clone();
                c["severity"] = json!(override_sev);
                c["applied_rule"] = json!({
                    "id":                id,
                    "name":              name,
                    "original_severity": original,
                });
                break;
            }
            c
        })
        .collect()
}

pub(crate) fn parse_codeowners(content: &str) -> Vec<String> {
    let mut owners: Vec<String> = content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .flat_map(|l| {
            let mut parts = l.split_whitespace();
            let _pattern = parts.next();
            parts
                .filter(|s| s.starts_with('@'))
                .map(|s| s.trim_start_matches('@').to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    owners.sort();
    owners.dedup();
    owners
}
