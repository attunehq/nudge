//! Static checks for installer safety behavior.

use std::{fs, path::Path};

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
fn release_workflow_builds_musl_targets_without_embeddings() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages dir")
        .parent()
        .expect("repo root");
    let workflow_path = repo_root.join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release workflow");

    assert!(
        workflow.contains("x86_64-unknown-linux-gnu"),
        "release workflow should still build the supported Linux x64 GNU artifact"
    );
    assert!(
        workflow.contains("aarch64-unknown-linux-gnu"),
        "release workflow should still build the supported Linux arm64 GNU artifact"
    );
    assert!(
        workflow.contains("x86_64-unknown-linux-musl"),
        "release workflow should build the Linux x64 musl artifact"
    );
    assert!(
        workflow.contains("aarch64-unknown-linux-musl"),
        "release workflow should build the Linux arm64 musl artifact"
    );
    assert!(
        workflow.contains("embeddings: false")
            && workflow.contains("args+=(--no-default-features)"),
        "musl release builds should disable default embedding support"
    );
}
