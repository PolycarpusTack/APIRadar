use radar_cli_lib::{
    github::build_comment_with_suites,
    render::{BlastRadiusEntry, BlastRadiusResponse, ConsumerInfo, EvidenceItem},
};
use radar_core::{
    diff::{diff_openapi, parse_openapi, DiffChange},
    models::{ChangeKind, Severity},
};

const V1: &str = include_str!("../../fixtures/demo-payments-api/v1.yaml");
const V2: &str = include_str!("../../fixtures/demo-payments-api/v2.yaml");

fn diff_v1_v2() -> Vec<DiffChange> {
    let base = parse_openapi(V1).expect("v1 should parse");
    let head = parse_openapi(V2).expect("v2 should parse");
    diff_openapi(&base, &head)
}

fn billing_svc_entry() -> BlastRadiusEntry {
    BlastRadiusEntry {
        consumer: ConsumerInfo {
            name: "billing-svc".to_string(),
            owner_team: "billing".to_string(),
            contact: "billing@example.com".to_string(),
        },
        confidence: "high".to_string(),
        last_seen: "2026-05-24T10:00:00Z".to_string(),
        has_runtime_usage: true,
        has_call_site: false,
        evidence: vec![EvidenceItem {
            kind: "runtime_usage".to_string(),
            operation: Some("GET /users/{id}".to_string()),
            field_path: Some("response.body.phone".to_string()),
            recorded_at: Some("2026-05-24T10:00:00Z".to_string()),
        }],
    }
}

fn mobile_gateway_entry() -> BlastRadiusEntry {
    BlastRadiusEntry {
        consumer: ConsumerInfo {
            name: "mobile-gateway".to_string(),
            owner_team: "mobile".to_string(),
            contact: "mobile@example.com".to_string(),
        },
        confidence: "medium".to_string(),
        last_seen: "2026-05-24T09:00:00Z".to_string(),
        has_runtime_usage: false,
        has_call_site: true,
        evidence: vec![EvidenceItem {
            kind: "static_call_site".to_string(),
            operation: Some("GET /users/{id}".to_string()),
            field_path: Some("response.phone".to_string()),
            recorded_at: None,
        }],
    }
}

fn blast_radius_fixture() -> BlastRadiusResponse {
    BlastRadiusResponse {
        entries: vec![billing_svc_entry(), mobile_gateway_entry()],
    }
}

#[test]
fn demo_field_removed_is_breaking() {
    let changes = diff_v1_v2();
    let phone_change = changes.iter().find(|c| {
        c.path.contains("phone") && c.kind == ChangeKind::FieldRemoved
    });
    assert!(
        phone_change.is_some(),
        "expected a FieldRemoved change for the phone field; got: {changes:?}"
    );
    let change = phone_change.unwrap();
    assert_eq!(change.severity, Severity::Breaking);
}

#[test]
fn demo_diff_produces_exactly_one_change() {
    let changes = diff_v1_v2();
    assert_eq!(
        changes.len(),
        1,
        "expected exactly one change (phone removed); got: {changes:?}"
    );
}

#[test]
fn demo_billing_svc_has_high_confidence_runtime_evidence() {
    let entry = billing_svc_entry();
    assert_eq!(entry.confidence, "high");
    assert!(entry.has_runtime_usage);
    let ev = &entry.evidence[0];
    assert_eq!(ev.kind, "runtime_usage");
    assert_eq!(ev.field_path.as_deref(), Some("response.body.phone"));
}

#[test]
fn demo_mobile_gateway_has_static_call_site_evidence() {
    let entry = mobile_gateway_entry();
    assert_eq!(entry.confidence, "medium");
    assert!(entry.has_call_site);
    let ev = &entry.evidence[0];
    assert_eq!(ev.kind, "static_call_site");
    assert_eq!(ev.operation.as_deref(), Some("GET /users/{id}"));
}

#[test]
fn demo_pr_comment_contains_evidence_and_verdict() {
    let changes = diff_v1_v2();
    let br = blast_radius_fixture();
    let comment = build_comment_with_suites(&changes, "v1", "v2", Some(&br), "block", "closed", &[]);

    assert!(comment.contains("BREAKING"), "should contain BREAKING badge");
    assert!(comment.contains("phone"), "should reference phone field");
    assert!(comment.contains("billing-svc"), "should mention billing-svc");
    assert!(comment.contains("mobile-gateway"), "should mention mobile-gateway");
    assert!(comment.contains("runtime_usage"), "should show runtime_usage source");
    assert!(comment.contains("static_call_site"), "should show static_call_site source");
    assert!(comment.contains("BLOCKED"), "should show BLOCKED verdict for fail_mode=closed");
}

#[test]
fn demo_pr_comment_structural_sections() {
    let changes = diff_v1_v2();
    let br = blast_radius_fixture();
    let comment = build_comment_with_suites(&changes, "v1", "v2", Some(&br), "block", "closed", &[]);

    assert!(comment.contains("Radar Monitor"), "should have Radar Monitor header");
    assert!(comment.contains("| Severity |"), "should have changes table");
    assert!(comment.contains("### Evidence"), "should have Evidence section");
    assert!(comment.contains("### Policy Verdict"), "should have Policy Verdict section");
    assert!(comment.contains("fail_mode: closed"), "should show fail_mode");
}
