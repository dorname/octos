//! Verifies that all deployment artifacts live under `deploy/`.
//! Cited by specs/task-deploy-k8s.spec.md.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `crates/octos-bus`; go up two levels.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

#[test]
fn test_deploy_directory_layout() {
    let root = repo_root();
    let deploy = root.join("deploy");

    assert!(
        deploy.exists(),
        "deploy/ directory missing at {}",
        deploy.display()
    );
    // Mandatory subdirectories
    for sub in &["k8s", "docker", "scripts", "docs"] {
        assert!(
            deploy.join(sub).is_dir(),
            "deploy/{sub}/ directory is required"
        );
    }
}

#[test]
fn test_no_scattered_deploy_files() {
    let root = repo_root();

    let forbidden_globs = [
        "k8s-*.yaml",       // pre-task scattered k8s manifests
        "docker-compose*.yml", // pre-task scattered compose
        "deploy*.sh",       // pre-task top-level deploy scripts
    ];

    for entry in std::fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        for glob in &forbidden_globs {
            // simple glob match: prefix*suffix
            if let (Some(stripped), false) = (glob.strip_suffix("*"), glob.ends_with("*")) {
                let prefix = stripped;
                if name_str.starts_with(prefix) {
                    panic!(
                        "scattered deploy file at root: {name_str} (matches {glob}); \
                         move it under deploy/"
                    );
                }
            }
        }
    }
}

#[test]
fn test_deploy_script_variants() {
    // Path resolution check: the script + 3 manifest files must all be present
    let root = repo_root();
    assert!(root.join("deploy/scripts/deploy-k8s.sh").is_file());
    for v in &["01-baseline", "02-hostpath-dev", "03-cluster-with-config"] {
        let manifest = root.join(format!("deploy/k8s/{v}.yaml"));
        assert!(manifest.is_file(), "missing manifest: {}", manifest.display());
    }
}

#[test]
fn test_deploy_script_unknown_variant() {
    // We can't actually exec the script (it requires kubectl + a cluster).
    // But we can verify the error path text by reading the script source.
    let root = repo_root();
    let script = std::fs::read_to_string(root.join("deploy/scripts/deploy-k8s.sh"))
        .expect("deploy-k8s.sh must exist");
    assert!(script.contains("Unknown variant"));
    assert!(script.contains("Usage:"));
}

#[test]
fn test_deploy_script_syntax() {
    let root = repo_root();
    let script = root.join("deploy/scripts/deploy-k8s.sh");

    let output = std::process::Command::new("bash")
        .arg("-n")
        .arg(&script)
        .output()
        .expect("bash must be on PATH");

    assert!(
        output.status.success(),
        "bash -n {} failed:\n{}",
        script.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_k8s_install_doc_sections() {
    let root = repo_root();
    let doc = std::fs::read_to_string(root.join("deploy/docs/K8S_INSTALL.md"))
        .expect("K8S_INSTALL.md must exist");

    for section in &[
        "## 架构概览",
        "## 前置条件",
        "## 镜像构建",
        "## 三种部署变体",
        "## 配置注入",
        "## 部署步骤",
        "## 验证",
        "## 故障排查",
        "## 升级与回滚",
    ] {
        assert!(
            doc.contains(section),
            "K8S_INSTALL.md missing required section: {section}"
        );
    }
}

#[test]
fn test_k8s_install_prerequisites() {
    let root = repo_root();
    let doc = std::fs::read_to_string(root.join("deploy/docs/K8S_INSTALL.md")).unwrap();

    // Split on "## " — the section heading itself lives between markers.
    let sections: Vec<&str> = doc.split("\n## ").collect();
    let prereq_section = sections
        .iter()
        .find(|s| s.starts_with("前置条件"))
        .expect("前置条件 section missing");

    for keyword in &[
        "kubectl",
        "postgres:16-alpine",
        "8080",
        "5432",
    ] {
        assert!(
            prereq_section.contains(keyword),
            "前置条件 section missing keyword: {keyword}"
        );
    }
}

#[test]
fn test_k8s_install_troubleshooting() {
    let root = repo_root();
    let doc = std::fs::read_to_string(root.join("deploy/docs/K8S_INSTALL.md")).unwrap();

    let sections: Vec<&str> = doc.split("\n## ").collect();
    let trouble_section = sections
        .iter()
        .find(|s| s.starts_with("故障排查"))
        .expect("故障排查 section missing");

    for keyword in &[
        "ContainerCreating",
        "CrashLoopBackOff",
        "API key",
        "migrations",
        "hostPath",
        "PG",
        "k8s",
    ] {
        assert!(
            trouble_section.contains(keyword),
            "故障排查 section missing keyword: {keyword}"
        );
    }
}