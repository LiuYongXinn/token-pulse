//! Sole NSIS channel: stage only previously verified bytes, launch, then normal Tauri exit.
//! No plugin install(): it exits the process before our ExitRequested/Exit cleanup can run.
use std::{
    io::Write,
    process::{Command, Stdio},
};
use token_pulse_core::updates::UpdateIssue;

pub(super) fn supported<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    !cfg!(debug_assertions)
        && app.config().identifier == "com.tokenpulse.desktop"
        && matches!(
            tauri::utils::platform::bundle_type(),
            Some(tauri::utils::config::BundleType::Nsis)
        )
}
fn portable_executable(bytes: &[u8]) -> bool {
    if bytes.get(..2) != Some(b"MZ") {
        return false;
    }
    let Some(offset) = bytes
        .get(0x3c..0x40)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
    else {
        return false;
    };
    let Ok(offset) = usize::try_from(offset) else {
        return false;
    };
    offset.checked_add(4).and_then(|end| bytes.get(offset..end)) == Some(b"PE\0\0")
}
pub(super) fn launch(bytes: &[u8]) -> Result<(), UpdateIssue> {
    if !portable_executable(bytes) {
        return Err(UpdateIssue::InstallerUnavailable);
    }
    let mut file = tempfile::Builder::new()
        .prefix("tokenpulse-verified-update-")
        .suffix(".exe")
        .tempfile()
        .map_err(|_| UpdateIssue::InstallerUnavailable)?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| UpdateIssue::InstallerUnavailable)?;
    // Close the writable file before CreateProcess. keep() retains only public signed
    // installer bytes. Any failure removes this exact owned file, never app data.
    let (handle, path) = file.keep().map_err(|_| UpdateIssue::InstallerUnavailable)?;
    drop(handle);
    let result = Command::new(&path)
        .args(["/P", "/UPDATE", "/R"])
        .arg(format!("/TOKENPULSE_PARENT={}", std::process::id()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match result {
        Ok(child) => {
            drop(child);
            Ok(())
        }
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            Err(UpdateIssue::InstallFailed)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_text_or_truncated_header_cannot_be_launched() {
        assert_eq!(
            launch(b"synthetic signed non executable"),
            Err(UpdateIssue::InstallerUnavailable)
        );
        for bytes in [vec![], b"MZ".to_vec(), vec![0; 64]] {
            assert!(!portable_executable(&bytes));
        }
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        assert!(portable_executable(&bytes));
        // Native CreateProcess rejects this deliberately incomplete PE. It must not be
        // treated as successful launch or cause shutdown (no executable instructions).
        assert_eq!(launch(&bytes), Err(UpdateIssue::InstallFailed));
        bytes[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(!portable_executable(&bytes));
    }
}
