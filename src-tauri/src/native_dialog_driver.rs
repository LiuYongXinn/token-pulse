//! Debug-only bounded driver for actual, owned Windows common dialogs.
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub(super) struct DialogDriver(Child);
impl DialogDriver {
    pub(super) fn start(
        kind: &str,
        action: &str,
        folder: &std::path::Path,
    ) -> Result<Self, String> {
        use std::os::windows::process::CommandExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("repository missing")?
            .join("scripts/native-source-dialog-driver.ps1");
        let child = Command::new("pwsh.exe")
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(script)
            .args([
                "-ApplicationId",
                &std::process::id().to_string(),
                "-Kind",
                kind,
                "-Action",
                action,
                "-Folder",
            ])
            .arg(folder)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "owned dialog driver could not start")?;
        Ok(Self(child))
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(25);
        while Instant::now() < deadline {
            if let Some(status) = self.0.try_wait().map_err(|_| "dialog driver wait failed")? {
                let mut diagnostics = String::new();
                if let Some(mut error) = self.0.stderr.take() {
                    use std::io::Read;
                    let _ = error.read_to_string(&mut diagnostics);
                }
                return if status.success() {
                    Ok(())
                } else {
                    let code = [
                        "DIALOG_NOT_FOUND",
                        "DIALOG_PATH_FIELD_MISSING",
                        "DIALOG_PATH_VALUE_MISSING",
                        "DIALOG_ACTION_MISSING",
                        "DIALOG_API_FAILED",
                        "DIALOG_NOT_CLOSED",
                        "DIALOG_ACTION_TYPE_INVALID",
                        "DIALOG_AMBIGUOUS_CONTROL",
                        "DIALOG_PATH_SET_FAILED",
                        "DIALOG_ACTION_FAILED",
                    ]
                    .into_iter()
                    .find(|code| diagnostics.contains(code))
                    .unwrap_or("DIALOG_DRIVER_START_FAILED");
                    let fields = diagnostics
                        .lines()
                        .filter(|line| {
                            line.starts_with("DIALOG_FIELD_")
                                && line.len() <= 64
                                && line.chars().all(|c| {
                                    c.is_ascii_uppercase()
                                        || c.is_ascii_digit()
                                        || c == '_'
                                        || c == '-'
                                })
                        })
                        .take(12)
                        .collect::<Vec<_>>();
                    Err(format!(
                        "owned dialog driver rejected scene: {code}; fields={fields:?}"
                    ))
                };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("owned dialog driver deadline".into())
    }
}
impl Drop for DialogDriver {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
