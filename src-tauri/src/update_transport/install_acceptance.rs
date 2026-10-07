//! Opt-in Windows handoff check: a signed minimal NSIS writes only an owned marker.
//! Does not install TokenPulse or claim that normal application cleanup/upgrade passed.
use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Instant,
};
use tauri::test::{mock_builder, mock_context, noop_assets};
use token_pulse_core::{error::ErrorCode, updates::UpdatePhase};

struct OwnedServer {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for OwnedServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.expect("owned loopback server failed");
            }
        }
    }
}

fn nsis_literal(path: &Path) -> String {
    let text = path.to_str().expect("owned fixture path must be UTF-8");
    assert!(!text.contains(['\r', '\n', '\0']));
    text.replace('$', "$$").replace('"', "$\\\"")
}

fn run_owned_tool(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW for the fixture's console tools.
        .spawn()
        .expect("owned fixture tool could not start");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            // Key-generation/signing output can contain private fixture material.
            assert!(
                status.success(),
                "owned fixture tool failed; output withheld"
            );
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned fixture tool timed out; output withheld");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

async fn wait_phase(
    service: &crate::update_service::UpdateService,
    busy: &[UpdatePhase],
) -> token_pulse_core::updates::UpdateSnapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = service.snapshot().unwrap();
        if !busy.contains(&snapshot.phase) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "owned update phase timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
#[ignore = "explicit native NSIS handoff requires cached compiler and Node/Tauri CLI"]
fn signed_nsis_handoff_uses_exact_arguments_and_requests_exit_once() {
    let fixture = tempfile::Builder::new()
        .prefix("tokenpulse-update-handoff-")
        .tempdir()
        .unwrap();
    let directory = fixture.path().canonicalize().unwrap();
    assert!(directory.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let cli = root.join("node_modules/@tauri-apps/cli/tauri.js");
    let compiler = root.join("target/.tauri/NSIS/makensis.exe");
    assert!(
        cli.is_file() && compiler.is_file(),
        "installed tools required"
    );
    let executable = directory.join("handoff-fixture.exe");
    let marker = directory.join("parameters.txt");
    let completed = directory.join("completed.txt");
    let source = directory.join("fixture.nsi");
    // No product files, registry, shortcuts, parent wait, UI or arbitrary path arguments.
    // Final marker is written after closing the parameter file to avoid partial reads.
    std::fs::write(
        &source,
        format!(
            r#"Unicode true
RequestExecutionLevel user
SilentInstall silent
Name "TokenPulse isolated handoff fixture"
OutFile "{}"
!include "FileFunc.nsh"
Section
  ${{GetParameters}} $0
  FileOpen $1 "{}" w
  FileWrite $1 $0
  FileClose $1
  FileOpen $1 "{}" w
  FileWrite $1 "complete"
  FileClose $1
SectionEnd
"#,
            nsis_literal(&executable),
            nsis_literal(&marker),
            nsis_literal(&completed),
        ),
    )
    .unwrap();
    run_owned_tool(
        Command::new(compiler)
            .args(["/V2", "/INPUTCHARSET", "UTF8"])
            .arg(&source)
            .current_dir(&directory),
    );
    let bytes = std::fs::read(&executable).unwrap();
    assert!(bytes.len() > 4096);

    // Ephemeral signing material only; never read the formal project private key.
    let key = directory.join("fixture.key");
    run_owned_tool(
        Command::new("node")
            .arg(&cli)
            .args([
                "signer",
                "generate",
                "--ci",
                "--password",
                "",
                "--write-keys",
            ])
            .arg(&key),
    );
    let public_key = std::fs::read_to_string(directory.join("fixture.key.pub")).unwrap();
    run_owned_tool(
        Command::new("node")
            .arg(&cli)
            .args(["signer", "sign", "--private-key-path"])
            .arg(&key)
            .args(["--password", "", "--app-version", "99.0.0"])
            .arg(&executable),
    );
    let signature = std::fs::read_to_string(executable.with_extension("exe.sig")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "version": "99.0.0",
        "notes": "Explicit minimal native NSIS fixture; no product installation",
        "platforms": { "windows-x86_64": {
            "url": format!("http://127.0.0.1:{port}/installer.exe"), "signature": signature
        }}
    }))
    .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(AtomicUsize::new(0));
    let server_stop = Arc::clone(&stop);
    let server_requests = Arc::clone(&requests);
    let byte_count = bytes.len();
    let server = OwnedServer {
        stop,
        worker: Some(thread::spawn(move || {
            while !server_stop.load(Ordering::Acquire) {
                let (mut socket, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("owned loopback accept failed: {error}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut block = [0; 1024];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    assert!(request.len() < 8192, "owned request exceeded limit");
                    let n = socket.read(&mut block).unwrap();
                    assert!(n > 0, "owned request closed before header");
                    request.extend_from_slice(&block[..n]);
                }
                let body = if request.starts_with(b"GET /latest.json ") {
                    &manifest
                } else if request.starts_with(b"GET /installer.exe ") {
                    &bytes
                } else {
                    panic!("unexpected owned fixture route");
                };
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                socket.write_all(body).unwrap();
                server_requests.fetch_add(1, Ordering::AcqRel);
            }
        })),
    };
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({ "pubkey": "", "requireSignedVersion": true }),
    );
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    let service = crate::update_service::UpdateService::fixture(
        Publication::fixture(port, public_key.trim()).unwrap(),
        Arc::new(|| {}),
    );
    let exits = Arc::new(AtomicUsize::new(0));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        service.start_check(app.handle().clone()).unwrap();
        let available = wait_phase(&service, &[UpdatePhase::Checking]).await;
        assert_eq!(available.phase, UpdatePhase::Available);
        assert_eq!(available.release.as_ref().unwrap().version, "99.0.0");
        service.start_download(&available.update_revision).unwrap();
        let ready = wait_phase(
            &service,
            &[UpdatePhase::Downloading, UpdatePhase::Verifying],
        )
        .await;
        assert_eq!(ready.phase, UpdatePhase::ReadyToInstall);
        assert_eq!(ready.issue, None);
        assert_eq!(ready.downloaded_bytes.unwrap().value(), byte_count as i128);
        assert!(!completed.exists() && !marker.exists());
        assert_eq!(exits.load(Ordering::Acquire), 0);
        let on_exit = Arc::clone(&exits);
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            on_exit.fetch_add(1, Ordering::AcqRel);
        });
        assert!(matches!(
            service.install_fixture(&available.update_revision, Arc::clone(&callback)),
            Err(ErrorCode::RevisionConflict)
        ));
        let installing = service
            .install_fixture(&ready.update_revision, Arc::clone(&callback))
            .unwrap();
        assert_eq!(installing.phase, UpdatePhase::Installing);
        assert!(matches!(
            service.install_fixture(&installing.update_revision, Arc::clone(&callback)),
            Err(ErrorCode::UpdateBusy)
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        while exits.load(Ordering::Acquire) == 0 || !completed.exists() {
            assert!(
                Instant::now() < deadline,
                "verified native handoff timed out"
            );
            let snapshot = service.snapshot().unwrap();
            assert_eq!(snapshot.phase, UpdatePhase::Installing);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(std::fs::read_to_string(&completed).unwrap(), "complete");
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            format!(
                "/P /UPDATE /R /TOKENPULSE_PARENT={} /D={}",
                std::process::id(),
                std::env::current_exe().unwrap().parent().unwrap().display()
            )
        );
        assert_eq!(exits.load(Ordering::Acquire), 1);
        assert!(matches!(
            service.install_fixture(&installing.update_revision, callback),
            Err(ErrorCode::UpdateBusy)
        ));
        assert_eq!(exits.load(Ordering::Acquire), 1);
    });
    drop(server);
    assert_eq!(requests.load(Ordering::Acquire), 2);
    println!(
        "NATIVE_SIGNED_NSIS_HANDOFF_OK: actual signed/version-bound download, native launch, exact arguments, one exit callback; no product install or app cleanup claimed"
    );
}
