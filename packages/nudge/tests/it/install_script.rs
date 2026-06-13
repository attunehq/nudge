//! Static checks for installer safety behavior.

use std::{fs, path::Path};

use pretty_assertions::assert_eq as pretty_assert_eq;

#[test]
fn powershell_installer_fails_closed_when_checksum_verification_is_unavailable() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages dir")
        .parent()
        .expect("repo root");
    let script_path = repo_root.join("scripts/install.ps1");
    let script = fs::read_to_string(&script_path).expect("read install.ps1");

    assert!(
        !script.contains("Write-Warning-Message \"Could not verify checksum"),
        "checksum verification must not warn and continue"
    );
    assert!(
        script.contains("Write-Error-Message \"Could not download checksums"),
        "missing checksum downloads must stop installation"
    );
    assert!(
        script.contains("Write-Error-Message \"Could not find checksum"),
        "missing archive checksums must stop installation"
    );
    assert!(
        script.contains("Write-Error-Message \"Checksum verification failed!"),
        "checksum mismatches must stop installation"
    );
}

#[test]
fn shell_installer_keeps_musl_release_artifacts_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages dir")
        .parent()
        .expect("repo root");
    let script_path = repo_root.join("scripts/install.sh");
    let script = fs::read_to_string(&script_path).expect("read install.sh");

    assert!(
        script.contains("os=\"$os-musl\""),
        "the installer should continue to request musl release artifacts"
    );
    assert!(
        !script.contains("Linux musl/Alpine release binaries are not currently available"),
        "musl Linux installs should not be blocked by the installer"
    );
}

#[test]
fn release_workflow_builds_ort_limited_targets_without_embeddings() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages dir")
        .parent()
        .expect("repo root");
    let workflow_path = repo_root.join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release workflow");

    pretty_assert_eq!(
        target_embeddings(&workflow, "x86_64-unknown-linux-gnu"),
        Some(true),
        "Linux x64 GNU should keep local semantic embeddings"
    );
    pretty_assert_eq!(
        target_embeddings(&workflow, "aarch64-unknown-linux-gnu"),
        Some(false),
        "Linux arm64 GNU should build without embedding support until ONNX Runtime links cleanly"
    );
    pretty_assert_eq!(
        target_embeddings(&workflow, "x86_64-unknown-linux-musl"),
        Some(false),
        "Linux x64 musl should build without embedding support"
    );
    pretty_assert_eq!(
        target_embeddings(&workflow, "aarch64-unknown-linux-musl"),
        Some(false),
        "Linux arm64 musl should build without embedding support"
    );
    pretty_assert_eq!(
        target_embeddings(&workflow, "x86_64-pc-windows-gnu"),
        Some(false),
        "Windows GNU should build without embedding support until ONNX Runtime publishes that target"
    );
    assert!(
        workflow.contains("args+=(--no-default-features)"),
        "feature-limited release builds should disable default embedding support"
    );
}

fn target_embeddings(workflow: &str, target: &str) -> Option<bool> {
    let start = workflow.find(&format!("- target: {target}"))?;
    let rest = &workflow[start..];
    let end = rest
        .find("\n                    - target: ")
        .unwrap_or(rest.len());
    let target_entry = &rest[..end];

    if target_entry.contains("embeddings: true") {
        Some(true)
    } else if target_entry.contains("embeddings: false") {
        Some(false)
    } else {
        None
    }
}
