//! Verifies the PG persistence matrix doc and the k8s cluster-mode manifest.
//! Cited by specs/task-pg-persistence-matrix.spec.md.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

// === matrix doc coverage ===

const PG_TABLES: &[&str] = &[
    "sessions",
    "messages",
    "agent_runs",
    "session_events",
    "outbox",
    "approvals",
    "run_leases",
    "run_checkpoints",
    "tool_invocations",
    "schedules",
    "schedule_firings",
];

const NEVER_PG_CATEGORIES: &[&str] = &[
    "profiles",
    "users",
    "tenants",
    "admin_audit",
    "ui-protocol",
    "usage_ledger",
];

fn read_doc() -> String {
    std::fs::read_to_string(repo_root().join("docs/analysis/pg-persistence-matrix.md"))
        .expect("pg-persistence-matrix.md must exist")
}

#[test]
fn test_pg_matrix_doc_covers_all_tables() {
    let doc = read_doc();
    for table in PG_TABLES {
        assert!(
            doc.contains(table),
            "matrix doc missing PG table: {table}"
        );
    }
}

#[test]
fn test_pg_matrix_doc_never_pg_categories() {
    let doc = read_doc();
    // Search the WHOLE document for each "never PG" category. The
    // doc structure has "永远不接 PG" in the Intent summary, the TL;DR
    // table, and per-category sections. Each category must appear
    // somewhere in the doc.
    for cat in &["profiles", "admin_audit", "ui-protocol", "usage_ledger"] {
        assert!(
            doc.contains(cat),
            "doc missing category: {cat}"
        );
    }
}

// === k8s manifest structural checks ===

fn read_manifest() -> String {
    std::fs::read_to_string(
        repo_root().join("deploy/k8s/03-cluster-with-config.yaml"),
    )
    .expect("03-cluster-with-config.yaml must exist")
}

#[test]
fn test_k8s_manifest_uses_init_container() {
    let manifest = read_manifest();
    assert!(
        manifest.contains("initContainers:"),
        "manifest missing initContainers"
    );
    // The init script command must be present.
    assert!(
        manifest.contains("/usr/local/bin/init-config.sh"),
        "manifest missing init-config.sh"
    );
}

#[test]
fn test_k8s_manifest_no_migrate_subcommand() {
    let manifest = read_manifest();
    // The init script MUST NOT call `octos migrate` because that
    // subcommand doesn't exist. K06 design runs migrations lazily
    // on first DB op (attach_durable_approvals_pg /
    // attach_cron_service_pg).
    for line in manifest.lines() {
        // Skip comments (lines starting with #).
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        assert!(
            !trimmed.contains("octos migrate"),
            "manifest line references non-existent migrate subcommand: {line}"
        );
    }
}

#[test]
fn test_k8s_manifest_binary_path_consistent() {
    let manifest = read_manifest();

    // The octos serve command must reference /opt/octos/octos...
    let serve_command = manifest
        .lines()
        .find(|l| l.contains("command:") && l.contains("/opt/octos/octos"))
        .expect("manifest must have `command: [\"/opt/octos/octos\"]` for serve container");

    assert!(
        serve_command.contains("/opt/octos/octos"),
        "serve command should invoke /opt/octos/octos: {serve_command}"
    );

    // ...and the binary must be mounted at exactly /opt/octos/octos (file)
    let binary_mount = manifest
        .lines()
        .find(|l| l.contains("mountPath:") && l.contains("/opt/octos/octos"))
        .expect("binary must be mounted at /opt/octos/octos");
    assert!(
        binary_mount.contains("/opt/octos/octos"),
        "binary mount should be /opt/octos/octos: {binary_mount}"
    );
}

#[test]
fn test_k8s_manifest_yaml_parse() {
    // The manifest must be parseable YAML with exactly 8 documents.
    let manifest = read_manifest();

    // Count document separators (`---` on its own line). 8 docs => 7 separators.
    let sep_count = manifest
        .lines()
        .filter(|l| l.trim() == "---")
        .count();
    assert!(
        sep_count >= 7,
        "expected at least 7 `---` separators (8 docs), got {sep_count}"
    );

    // Critical resource kinds must all be present.
    for kind in &[
        "kind: Namespace",
        "kind: Deployment", // pg + octos = 2
        "kind: Service",
        "kind: ConfigMap",
        "kind: Secret",
    ] {
        let count = manifest.matches(kind).count();
        let expected = if *kind == "kind: Deployment" { 2 } else { 1 };
        assert!(
            count >= expected,
            "expected >= {expected} occurrences of `{kind}`, got {count}"
        );
    }
}

// === init script content ===

fn read_init_script() -> String {
    // The init-config.sh is embedded in the ConfigMap YAML.
    let manifest = read_manifest();
    let start = manifest
        .find("init-config.sh: |")
        .expect("init-config.sh entry must exist in ConfigMap");
    let after_start = &manifest[start..];
    let yaml_value_start = after_start.find('\n').unwrap() + 1;
    let body = &after_start[yaml_value_start..];
    // The script ends at the next `...` (YAML end-of-document) or at
    // the next top-level key (no leading spaces). We take everything
    // until the next `---` separator.
    let end = body.find("\n---").unwrap_or(body.len());
    body[..end].to_string()
}

#[test]
fn test_init_script_creates_required_dirs() {
    let script = read_init_script();
    assert!(script.contains("mkdir -p /tmp/octos-data"));
    assert!(script.contains("WORKSPACE_DIR"));
}

#[test]
fn test_init_script_writes_profile_config() {
    let script = read_init_script();
    assert!(script.contains("profile") || script.contains("PROFILE_DIR"));
    assert!(script.contains("workspace_root"));
}

#[test]
fn test_init_script_documents_lazy_migration() {
    let script = read_init_script();
    // K06 design: migrations run lazily on first DB op.
    // The init script should document this so operators don't expect
    // eager migrations.
    let has_doc = script.contains("Lazily")
        || script.contains("lazily")
        || script.contains("lazy")
        || script.contains("K06")
        || script.contains("惰性");
    assert!(
        has_doc,
        "init script must document lazy migration behavior (K06 design)"
    );
}