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
