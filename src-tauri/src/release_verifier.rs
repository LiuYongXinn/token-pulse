//! Local release tooling only: no Tauri runtime, database, network or installer execution.
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};

const MAX_INSTALLER: u64 = 512 * 1024 * 1024;

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, ()> {
    let file = std::fs::File::open(path).map_err(|_| ())?;
    if !file.metadata().map_err(|_| ())?.is_file() {
        return Err(());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err(());
    }
    Ok(bytes)
}

fn verify(
    bytes: &[u8],
    encoded_signature: &str,
    encoded_key: &str,
    version: &str,
) -> Result<(), ()> {
    if encoded_key.is_empty() || encoded_key.len() > 4096 || encoded_signature.len() > 16384 {
        return Err(());
    }
    let key = String::from_utf8(STANDARD.decode(encoded_key).map_err(|_| ())?).map_err(|_| ())?;
    let signature =
        String::from_utf8(STANDARD.decode(encoded_signature).map_err(|_| ())?).map_err(|_| ())?;
    let key = minisign_verify::PublicKey::decode(&key).map_err(|_| ())?;
    let signature = minisign_verify::Signature::decode(&signature).map_err(|_| ())?;
    // Includes the global signature: trusted comment fields are used only after this succeeds.
    key.verify(bytes, &signature, true).map_err(|_| ())?;
    let versions: Vec<_> = signature
        .trusted_comment()
        .split('\t')
        .filter_map(|field| field.strip_prefix("version:"))
        .collect();
    // Release preparation uses one canonical version spelling; duplicate fields are rejected.
    if versions != [version] {
        return Err(());
    }
    Ok(())
}

fn write_report(installer: &Path, signature: &Path, report: &Path, key: &str) -> Result<(), ()> {
    let bytes = bounded_read(installer, MAX_INSTALLER)?;
    let signature_bytes = bounded_read(signature, 16384)?;
    let signature_text = std::str::from_utf8(&signature_bytes)
        .map_err(|_| ())?
        .trim();
    verify(&bytes, signature_text, key, env!("CARGO_PKG_VERSION"))?;
    let output = serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "version": env!("CARGO_PKG_VERSION"),
        "target": env!("TOKENPULSE_BUILD_TARGET"),
        "installer_sha256": format!("{:x}", Sha256::digest(&bytes)),
        "installer_bytes": bytes.len().to_string(),
        "signature_sha256": format!("{:x}", Sha256::digest(&signature_bytes)),
        "public_key_sha256": format!("{:x}", Sha256::digest(key.as_bytes()))
    }))
    .map_err(|_| ())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)
        .map_err(|_| ())?;
    file.write_all(&output)
        .and_then(|_| file.sync_all())
        .map_err(|_| ())
}

/// Reserved maintenance command. Finite exit statuses deliberately reveal no raw paths/errors.
pub fn run_if_requested() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--verify-update-release")) {
        return None;
    }
    if cfg!(debug_assertions) {
        return Some(13);
    }
    let remaining: Vec<_> = args.collect();
    if remaining.len() != 3 {
        return Some(12);
    }
    let key = option_env!("TOKENPULSE_UPDATER_PUBLIC_KEY").unwrap_or("");
    Some(
        if write_report(
            Path::new(&remaining[0]),
            Path::new(&remaining[1]),
            Path::new(&remaining[2]),
            key,
        )
        .is_ok()
        {
            0
        } else {
            14
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_or_missing_material_never_creates_a_report() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("installer.exe");
        let signature = temp.path().join("installer.exe.sig");
        let report = temp.path().join("report.json");
        std::fs::write(&input, b"synthetic non executable").unwrap();
        std::fs::write(&signature, "invalid").unwrap();
        assert!(write_report(&input, &signature, &report, "").is_err());
        assert!(!report.exists());
        assert!(bounded_read(&input, 4).is_err());
        assert!(bounded_read(temp.path(), 100).is_err());
    }

    #[test]
    #[ignore = "uses installed Tauri CLI to sign ephemeral synthetic fixtures"]
    fn actual_signature_version_and_key_binding() {
        let temp = tempfile::tempdir().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let cli = root.join("node_modules/@tauri-apps/cli/tauri.js");
        let key = temp.path().join("fixture.key");
        let second = temp.path().join("other.key");
        for path in [&key, &second] {
            let output = std::process::Command::new("node")
                .arg(&cli)
                .args([
                    "signer",
                    "generate",
                    "--ci",
                    "--password",
                    "",
                    "--write-keys",
                ])
                .arg(path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fixture key generation failed; output withheld"
            );
        }
        let installer = temp.path().join("synthetic.exe");
        let signature_path = temp.path().join("synthetic.exe.sig");
        let report = temp.path().join("report.json");
        let bytes = b"explicitly synthetic signed fixture; never executed";
        std::fs::write(&installer, bytes).unwrap();
        let public = std::fs::read_to_string(key.with_extension("key.pub")).unwrap();
        let other = std::fs::read_to_string(second.with_extension("key.pub")).unwrap();
        let sign = |version: Option<&str>| {
            let mut command = std::process::Command::new("node");
            command
                .arg(&cli)
                .args(["signer", "sign", "--private-key-path"])
                .arg(&key)
                .args(["--password", ""]);
            if let Some(version) = version {
                command.args(["--app-version", version]);
            }
            let output = command.arg(&installer).output().unwrap();
            assert!(
                output.status.success(),
                "fixture signing failed; output withheld"
            );
            std::fs::read_to_string(&signature_path).unwrap()
        };
        let signed = sign(Some(env!("CARGO_PKG_VERSION")));
        assert!(
            verify(
                bytes,
                signed.trim(),
                public.trim(),
                env!("CARGO_PKG_VERSION")
            )
            .is_ok()
        );
        assert!(
            verify(
                b"tampered",
                signed.trim(),
                public.trim(),
                env!("CARGO_PKG_VERSION")
            )
            .is_err()
        );
        assert!(
            verify(
                bytes,
                signed.trim(),
                other.trim(),
                env!("CARGO_PKG_VERSION")
            )
            .is_err()
        );
        assert!(verify(bytes, signed.trim(), public.trim(), "99.0.0").is_err());
        write_report(&installer, &signature_path, &report, public.trim()).unwrap();
        let saved = std::fs::read(&report).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(value["installer_bytes"], bytes.len().to_string());
        assert_eq!(
            value["installer_sha256"],
            format!("{:x}", Sha256::digest(bytes))
        );
        assert!(write_report(&installer, &signature_path, &report, public.trim()).is_err());
        assert_eq!(std::fs::read(&report).unwrap(), saved);
        let legacy = sign(None);
        assert!(
            verify(
                bytes,
                legacy.trim(),
                public.trim(),
                env!("CARGO_PKG_VERSION")
            )
            .is_err()
        );
    }
}
