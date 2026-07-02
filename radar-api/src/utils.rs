use serde_json::{json, Value};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Host allowlist (RADAR_ALLOWED_HOSTS)
// ---------------------------------------------------------------------------

/// Returns true if the URL's hostname matches at least one of the patterns in
/// `RADAR_ALLOWED_HOSTS` (comma-separated glob-style patterns, e.g.
/// `*.internal,api.github.com`).
///
/// When `RADAR_ALLOWED_HOSTS` is unset or empty, all non-SSRF hosts are allowed.
pub(crate) fn is_host_allowed(url_str: &str) -> bool {
    let allowlist = std::env::var("RADAR_ALLOWED_HOSTS").unwrap_or_default();
    if allowlist.trim().is_empty() {
        return true; // no restriction beyond SSRF guard
    }
    let Ok(url) = url::Url::parse(url_str) else {
        return false;
    };
    let host = url.host_str().unwrap_or("").to_lowercase();
    allowlist.split(',').any(|pat| {
        let p = pat.trim().to_lowercase();
        if let Some(suffix) = p.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{suffix}"))
        } else {
            host == p
        }
    })
}

// ---------------------------------------------------------------------------
// Secret handling — constant-time compare + credential redaction
// ---------------------------------------------------------------------------

/// Compare two byte strings in constant time (w.r.t. content) to avoid leaking
/// how many leading bytes matched via timing. The length check short-circuits,
/// which only reveals length equality — acceptable for fixed-format tokens.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// Pagination clamping — identical behaviour on SQLite and PostgreSQL
// ---------------------------------------------------------------------------

/// Clamp user-supplied pagination to safe bounds. A negative `LIMIT` means
/// "return everything" on SQLite (a potential data dump / DoS) but raises an
/// error on PostgreSQL — so this floors both values and caps the limit,
/// yielding identical behaviour on both backends.
pub(crate) fn clamp_pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    const DEFAULT_LIMIT: i64 = 50;
    const MAX_LIMIT: i64 = 200;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

// ---------------------------------------------------------------------------
// SSRF protection — shared guard for webhooks and scheduled scans
// ---------------------------------------------------------------------------

/// Returns true if the URL must be blocked (non-HTTPS, private address space, or unresolvable).
/// Shared by webhook creation and scheduled-scan registration/execution.
pub(crate) fn is_ssrf_blocked(url_str: &str) -> bool {
    let Ok(url) = url::Url::parse(url_str) else {
        return true;
    };
    if url.scheme() != "https" {
        return true;
    }
    let Some(host) = url.host_str() else {
        return true;
    };
    use std::net::ToSocketAddrs;
    match (host, 443u16).to_socket_addrs() {
        Ok(addrs) => addrs.into_iter().any(|a| is_rfc1918_or_loopback(a.ip())),
        Err(_) => true, // fail-safe: block if DNS resolution fails
    }
}

fn is_rfc1918_or_loopback(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let oct = v4.octets();
            oct[0] == 10                                         // 10.0.0.0/8
            || (oct[0] == 172 && (oct[1] & 0xf0) == 16)         // 172.16.0.0/12
            || (oct[0] == 192 && oct[1] == 168)                  // 192.168.0.0/16
            || oct[0] == 127                                     // 127.0.0.0/8
            || (oct[0] == 169 && oct[1] == 254) // 169.254.0.0/16 link-local
        }
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            v6.is_loopback()                       // ::1
            || (s[0] & 0xfe00) == 0xfc00           // fc00::/7 — ULA (private unicast)
            || (s[0] & 0xffc0) == 0xfe80 // fe80::/10 — link-local
        }
    }
}

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
            a.get("value")?
                .get("stringValue")?
                .as_str()
                .map(|s| s.to_owned())
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
    if deny_list.is_empty() {
        return false;
    }
    deny_list
        .split(',')
        .any(|pat| path_matches(pat.trim(), field))
}

/// Determine whether this event should be kept given the sample rate [0.0, 1.0].
pub(crate) fn sample_keep(rate: f64) -> bool {
    if rate >= 1.0 {
        return true;
    }
    if rate <= 0.0 {
        return false;
    }
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // NOTE: sampling uses the clock's sub-second part as a cheap pseudo-random
    // source; it is adequate but weakly distributed under burst traffic (a real
    // RNG would be better). What matters for correctness is the [0.0, 1.0) range.
    unit_sample_from_nanos(ns) < rate
}

/// Map a sub-second nanosecond count to a sample in `[0.0, 1.0)`.
/// `subsec_nanos()` spans `[0, 1_000_000_000)`, so it must be divided by 1e9.
/// (Dividing by `u32::MAX` (~4.29e9) capped the sample at ~0.233 and made every
/// `sample_rate >= 0.24` keep 100% of events — the bug this replaces.)
fn unit_sample_from_nanos(ns: u32) -> f64 {
    ns as f64 / 1_000_000_000.0
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
            let kind = c
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let path = c
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let current_sev = c
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();

            for (id, name, pat, rule_kind, override_sev) in rules {
                if rule_kind != &kind {
                    continue;
                }
                let pattern = pat.as_deref().unwrap_or("");
                if !path_matches(pattern, &path) {
                    continue;
                }
                if !is_severity_downgrade(&current_sev, override_sev) {
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_semantics() {
        assert!(constant_time_eq(b"Bearer abc", b"Bearer abc"));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer abd"));
        assert!(!constant_time_eq(b"short", b"longer-value"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn unit_sample_spans_full_range() {
        // The regression guard for the divisor bug: the old code divided by
        // u32::MAX and produced ~0.233 for max nanos, so any rate >= 0.24 kept
        // everything. With /1e9 the sample spans the full [0,1) range.
        assert_eq!(unit_sample_from_nanos(0), 0.0);
        assert!(unit_sample_from_nanos(999_999_999) > 0.999);
        let mid = unit_sample_from_nanos(500_000_000);
        assert!((0.49..0.51).contains(&mid), "mid sample was {mid}");
    }

    #[test]
    fn sample_keep_boundaries() {
        assert!(sample_keep(1.0));
        assert!(sample_keep(2.0)); // >= 1.0 keeps all
        assert!(!sample_keep(0.0));
        assert!(!sample_keep(-0.5)); // <= 0.0 drops all
    }

    #[test]
    fn clamp_pagination_bounds() {
        // Negative limit → floored to 1 (not "everything"); huge → capped at 200.
        assert_eq!(clamp_pagination(Some(-1), Some(-5)), (1, 0));
        assert_eq!(clamp_pagination(Some(10_000), Some(30)), (200, 30));
        assert_eq!(clamp_pagination(Some(25), Some(0)), (25, 0));
        assert_eq!(clamp_pagination(None, None), (50, 0));
    }

    // is_rfc1918_or_loopback — covers every blocked range and a public address
    #[test]
    fn rfc1918_blocks_10_slash_8() {
        assert!(is_rfc1918_or_loopback("10.0.0.1".parse().unwrap()));
        assert!(is_rfc1918_or_loopback("10.255.255.255".parse().unwrap()));
    }

    #[test]
    fn rfc1918_blocks_172_16_slash_12() {
        assert!(is_rfc1918_or_loopback("172.16.0.1".parse().unwrap()));
        assert!(is_rfc1918_or_loopback("172.31.255.255".parse().unwrap()));
        assert!(!is_rfc1918_or_loopback("172.15.0.1".parse().unwrap()));
        assert!(!is_rfc1918_or_loopback("172.32.0.1".parse().unwrap()));
    }

    #[test]
    fn rfc1918_blocks_192_168_slash_16() {
        assert!(is_rfc1918_or_loopback("192.168.0.1".parse().unwrap()));
        assert!(is_rfc1918_or_loopback("192.168.255.255".parse().unwrap()));
        assert!(!is_rfc1918_or_loopback("192.169.0.1".parse().unwrap()));
    }

    #[test]
    fn rfc1918_blocks_loopback() {
        assert!(is_rfc1918_or_loopback("127.0.0.1".parse().unwrap()));
        assert!(is_rfc1918_or_loopback("127.255.255.255".parse().unwrap()));
    }

    #[test]
    fn rfc1918_blocks_link_local() {
        assert!(is_rfc1918_or_loopback("169.254.0.1".parse().unwrap()));
        assert!(is_rfc1918_or_loopback("169.254.169.254".parse().unwrap())); // AWS IMDSv1
    }

    #[test]
    fn rfc1918_allows_public_ip() {
        assert!(!is_rfc1918_or_loopback("8.8.8.8".parse().unwrap()));
        assert!(!is_rfc1918_or_loopback("1.1.1.1".parse().unwrap()));
        assert!(!is_rfc1918_or_loopback("93.184.216.34".parse().unwrap()));
    }

    // is_ssrf_blocked — checks that scheme and IP-literal addresses are caught
    // without relying on external DNS resolution.
    #[test]
    fn ssrf_blocked_non_https_scheme() {
        assert!(is_ssrf_blocked("http://example.com/hook"));
        assert!(is_ssrf_blocked("ftp://example.com/hook"));
        assert!(is_ssrf_blocked("file:///etc/passwd"));
    }

    #[test]
    fn ssrf_blocked_invalid_url() {
        assert!(is_ssrf_blocked("not-a-url"));
        assert!(is_ssrf_blocked(""));
    }

    #[test]
    fn ssrf_blocked_rfc1918_ip_literals() {
        // IP literals resolve without external DNS — safe to test hermetically.
        assert!(is_ssrf_blocked("https://192.168.1.100/hook"));
        assert!(is_ssrf_blocked("https://10.0.0.1/hook"));
        assert!(is_ssrf_blocked("https://172.16.0.1/hook"));
        assert!(is_ssrf_blocked("https://169.254.169.254/latest/meta-data/")); // AWS IMDSv1
    }

    #[test]
    fn ssrf_blocked_loopback_ip_literal() {
        assert!(is_ssrf_blocked("https://127.0.0.1/hook"));
    }

    #[test]
    fn ssrf_blocked_ipv6_loopback_literal() {
        // ::1 is the IPv6 loopback address.
        assert!(is_ssrf_blocked("https://[::1]/hook"));
    }

    #[test]
    fn ssrf_blocked_ipv6_ula() {
        // fc00::/7 — Unique Local Addresses (private unicast, equivalent to RFC 1918 for IPv6).
        assert!(is_ssrf_blocked("https://[fd00::1]/hook"));
        assert!(is_ssrf_blocked("https://[fc00::1]/hook"));
    }

    #[test]
    fn ssrf_blocked_ipv6_link_local() {
        // fe80::/10 — link-local (equivalent to 169.254.0.0/16 for IPv6).
        assert!(is_ssrf_blocked("https://[fe80::1]/hook"));
    }

    // is_host_allowed — covers empty list, exact match, wildcard subdomain
    #[test]
    fn host_allowed_empty_list_permits_all() {
        std::env::remove_var("RADAR_ALLOWED_HOSTS");
        assert!(is_host_allowed("https://api.github.com/hook"));
        assert!(is_host_allowed("https://example.com/hook"));
    }

    #[test]
    fn host_allowed_exact_match() {
        std::env::set_var("RADAR_ALLOWED_HOSTS", "api.github.com,hooks.slack.com");
        assert!(is_host_allowed("https://api.github.com/hook"));
        assert!(is_host_allowed(
            "https://hooks.slack.com/services/T0/B0/xyz"
        ));
        assert!(!is_host_allowed("https://evil.com/hook"));
        std::env::remove_var("RADAR_ALLOWED_HOSTS");
    }

    #[test]
    fn host_allowed_wildcard_subdomain() {
        std::env::set_var("RADAR_ALLOWED_HOSTS", "*.internal");
        assert!(is_host_allowed("https://api.internal/hook"));
        assert!(is_host_allowed("https://build.ci.internal/hook"));
        assert!(!is_host_allowed("https://notinternal.com/hook"));
        assert!(!is_host_allowed("https://evil.internal.attacker.com/hook"));
        std::env::remove_var("RADAR_ALLOWED_HOSTS");
    }
}
