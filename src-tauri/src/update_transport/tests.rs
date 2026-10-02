use super::*;

#[test]
fn publication_and_redirect_boundaries_are_closed_to_other_sources() {
    assert!(matches!(
        Publication::production(""),
        Err(UpdateIssue::PublicationNotConfigured)
    ));
    assert!(matches!(
        Publication::production("not-a-key"),
        Err(UpdateIssue::PublicationNotConfigured)
    ));
    let production = SourcePolicy::Production;
    let artifact = "https://github.com/LiuYongXinn/token-pulse/releases/download/v0.2.0/TokenPulse_0.2.0_x64-setup.exe";
    assert!(production.artifact_allowed(&Url::parse(artifact).unwrap()));
    for url in [
        "http://github.com/LiuYongXinn/token-pulse/releases/download/v0.2.0/app.exe",
        "https://github.com/other/token-pulse/releases/download/v0.2.0/app.exe",
        "https://github.com/LiuYongXinn/token-pulse/releases/download/v0.2.0/app.zip",
        "https://release-assets.githubusercontent.com/arbitrary.exe",
        "https://github.com:444/LiuYongXinn/token-pulse/releases/download/v0.2.0/app.exe",
        "https://user:secret@github.com/LiuYongXinn/token-pulse/releases/download/v0.2.0/app.exe",
    ] {
        assert!(!production.artifact_allowed(&Url::parse(url).unwrap()));
    }
    assert!(
        production.redirect_allowed(
            &Url::parse(
                "https://release-assets.githubusercontent.com/public-asset?temporary=public"
            )
            .unwrap()
        )
    );
    for url in [
        "http://release-assets.githubusercontent.com/asset",
        "https://release-assets.githubusercontent.com.evil.test/asset",
        "https://github.com/other/token-pulse/releases/download/v0.2.0/app.exe",
        "https://127.0.0.1/asset",
    ] {
        assert!(!production.redirect_allowed(&Url::parse(url).unwrap()));
    }
    let fixture = SourcePolicy::Fixture(12345);
    assert!(fixture.artifact_allowed(&Url::parse("http://127.0.0.1:12345/installer.exe").unwrap()));
    assert!(
        !fixture.artifact_allowed(&Url::parse("http://127.0.0.1:12346/installer.exe").unwrap())
    );
}

#[test]
fn cryptographic_failures_are_finite_and_do_not_expose_native_error_text() {
    assert_eq!(
        issue(tauri_plugin_updater::Error::MissingSignedVersion),
        UpdateIssue::SignatureInvalid
    );
    assert_eq!(
        issue(tauri_plugin_updater::Error::SignedVersionMismatch {
            signed: "1.0.0".into(),
            announced: "99.0.0".into()
        }),
        UpdateIssue::SignatureInvalid
    );
    assert_eq!(
        issue(tauri_plugin_updater::Error::Network(
            "sensitive raw provider error".into()
        )),
        UpdateIssue::Network
    );
    assert!(decode_text(&"A".repeat(16385)).is_err());
}

// Real HTTP + minisign + signed-version verification using the installed Tauri CLI.
// Explicitly opt in: ephemeral fixture key, only the owned 127.0.0.1 port, no installer.
#[test]
#[ignore = "explicit local signed updater acceptance requires Node/Tauri CLI"]
fn signed_local_download_checks_bytes_version_and_missing_version() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        path::Path,
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        thread,
    };
    use tauri::test::{mock_builder, mock_context, noop_assets};
    let fixture = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let cli = root.join("node_modules/@tauri-apps/cli/tauri.js");
    assert!(cli.is_file(), "installed Tauri CLI required");
    let key = fixture.path().join("fixture.key");
    let output = Command::new("node")
        .arg(&cli)
        .args([
            "signer",
            "generate",
            "--ci",
            "--password",
            "",
            "--write-keys",
        ])
        .arg(&key)
        .output()
        .unwrap();
    // Never include stdout/stderr: key generation may print private fixture material.
    assert!(
        output.status.success(),
        "fixture key generation failed; output withheld"
    );
    drop(output);
    let public_key = std::fs::read_to_string(fixture.path().join("fixture.key.pub")).unwrap();
    let bytes = b"TokenPulse synthetic signed updater fixture; never an executable.";
    let file = fixture.path().join("fixture.bin");
    std::fs::write(&file, bytes).unwrap();
    fn sign(cli: &Path, key: &Path, file: &Path, bind_version: bool) -> String {
        let mut command = Command::new("node");
        command
            .arg(cli)
            .args(["signer", "sign", "--private-key-path"])
            .arg(key)
            .args(["--password", ""]);
        if bind_version {
            command.args(["--app-version", "99.0.0"]);
        }
        let output = command.arg(file).output().unwrap();
        assert!(
            output.status.success(),
            "fixture signing failed; output withheld"
        );
        std::fs::read_to_string(file.with_extension("bin.sig")).unwrap()
    }
    let signature = sign(&cli, &key, &file, true);
    let legacy = sign(&cli, &key, &file, false);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let mode = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let hold = Arc::new(AtomicBool::new(false));
    let server_mode = Arc::clone(&mode);
    let server_stop = Arc::clone(&stop);
    let server_hold = Arc::clone(&hold);
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            let (mut socket, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(_) => return,
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut block = [0; 1024];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") && request.len() < 8192 {
                let n = socket.read(&mut block).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&block[..n]);
            }
            let scenario = server_mode.load(Ordering::Acquire);
            while server_hold.load(Ordering::Acquire) && !server_stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
            if scenario == 4 && request.starts_with(b"GET /latest.json ") {
                socket
                    .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                    .unwrap();
                continue;
            }
            let body = if request.starts_with(b"GET /latest.json ") {
                serde_json::to_vec(&serde_json::json!({ "version": if scenario == 2 { "100.0.0" } else { "99.0.0" }, "platforms": { "windows-x86_64": { "url": format!("http://127.0.0.1:{port}/{}", if scenario == 5 { "different.exe" } else { "installer.exe" }), "signature": if scenario == 3 { &legacy } else { &signature } } }, "notes": "Explicit synthetic local fixture" })).unwrap()
            } else if request.starts_with(b"GET /installer.exe ") {
                let mut body = bytes.to_vec();
                if scenario == 1 {
                    body[0] ^= 1;
                }
                body
            } else {
                panic!("unexpected owned fixture route");
            };
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({ "pubkey": "", "requireSignedVersion": true }),
    );
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    let publication = Publication::fixture(port, public_key.trim()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(async {
            for scenario in 0..4 {
                mode.store(scenario, Ordering::Release);
                let candidate = publication.check(app.handle()).await.unwrap().unwrap();
                let progress = Arc::new(AtomicUsize::new(0));
                let finished = Arc::new(AtomicBool::new(false));
                let read = Arc::clone(&progress);
                let eof = Arc::clone(&finished);
                let result = candidate
                    .download(
                        move |chunk, total| {
                            assert_eq!(total, Some(bytes.len() as u64));
                            read.fetch_add(chunk, Ordering::AcqRel);
                        },
                        move || {
                            eof.store(true, Ordering::Release);
                        },
                    )
                    .await;
                assert!(finished.load(Ordering::Acquire));
                assert_eq!(progress.load(Ordering::Acquire), bytes.len());
                if scenario == 0 {
                    let verified = result.unwrap();
                    assert_eq!(verified.bytes, bytes);
                    assert_eq!(verified.version(), "99.0.0");
                } else {
                    assert!(matches!(result, Err(UpdateIssue::SignatureInvalid)));
                }
            }
            use token_pulse_core::{error::ErrorCode, updates::UpdatePhase};
            let notifications = Arc::new(AtomicUsize::new(0));
            let notified = Arc::clone(&notifications);
            let service = crate::update_service::UpdateService::fixture(
                publication.clone(),
                Arc::new(move || {
                    notified.fetch_add(1, Ordering::AcqRel);
                }),
            );
            async fn terminal(
                service: &crate::update_service::UpdateService,
                busy: &[UpdatePhase],
            ) -> token_pulse_core::updates::UpdateSnapshot {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    let snapshot = service.snapshot().unwrap();
                    if !busy.contains(&snapshot.phase) {
                        return snapshot;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "owned updater did not reach terminal state"
                    );
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
            for scenario in 0..4 {
                mode.store(scenario, Ordering::Release);
                hold.store(true, Ordering::Release);
                let checking = service.start_check(app.handle().clone()).unwrap();
                assert_eq!(checking.phase, UpdatePhase::Checking);
                assert_eq!(checking.release, None);
                assert!(matches!(
                    service.start_check(app.handle().clone()),
                    Err(ErrorCode::UpdateBusy)
                ));
                assert!(matches!(
                    service.start_download(&checking.update_revision),
                    Err(ErrorCode::UpdateBusy)
                ));
                hold.store(false, Ordering::Release);
                let offer = terminal(&service, &[UpdatePhase::Checking]).await;
                assert_eq!(offer.phase, UpdatePhase::Available);
                assert!(matches!(
                    service.start_download(&checking.update_revision),
                    Err(ErrorCode::RevisionConflict)
                ));
                hold.store(true, Ordering::Release);
                let downloading = service.start_download(&offer.update_revision).unwrap();
                assert_eq!(downloading.phase, UpdatePhase::Downloading);
                assert_eq!(downloading.total_bytes, None);
                assert!(matches!(
                    service.start_check(app.handle().clone()),
                    Err(ErrorCode::UpdateBusy)
                ));
                hold.store(false, Ordering::Release);
                let done = terminal(
                    &service,
                    &[UpdatePhase::Downloading, UpdatePhase::Verifying],
                )
                .await;
                assert_eq!(done.downloaded_bytes.unwrap().value(), bytes.len() as i128);
                if scenario == 0 {
                    assert_eq!(done.phase, UpdatePhase::ReadyToInstall);
                    assert_eq!(done.issue, None);
                    let exits = Arc::new(AtomicUsize::new(0));
                    let exited = Arc::clone(&exits);
                    assert!(matches!(
                        service.install_fixture(&offer.update_revision, Arc::new(|| {})),
                        Err(ErrorCode::RevisionConflict)
                    ));
                    let installing = service
                        .install_fixture(
                            &done.update_revision,
                            Arc::new(move || {
                                exited.fetch_add(1, Ordering::AcqRel);
                            }),
                        )
                        .unwrap();
                    assert_eq!(installing.phase, UpdatePhase::Installing);
                    let failed = terminal(&service, &[UpdatePhase::Installing]).await;
                    assert_eq!(failed.phase, UpdatePhase::Error);
                    assert_eq!(failed.issue, Some(UpdateIssue::InstallerUnavailable));
                    assert_eq!(exits.load(Ordering::Acquire), 0);
                    assert!(matches!(
                        service.install_fixture(&failed.update_revision, Arc::new(|| {})),
                        Err(ErrorCode::UpdateUnavailable)
                    ));
                } else {
                    assert_eq!(done.phase, UpdatePhase::Error);
                    assert_eq!(done.issue, Some(UpdateIssue::SignatureInvalid));
                }
            }
            mode.store(4, Ordering::Release);
            service.start_check(app.handle().clone()).unwrap();
            let current = terminal(&service, &[UpdatePhase::Checking]).await;
            assert_eq!(current.phase, UpdatePhase::Current);
            assert_eq!(current.release, None);
            assert_eq!(current.downloaded_bytes, None);
            assert_eq!(current.total_bytes, None);
            mode.store(5, Ordering::Release);
            service.start_check(app.handle().clone()).unwrap();
            let rejected = terminal(&service, &[UpdatePhase::Checking]).await;
            assert_eq!(rejected.phase, UpdatePhase::Error);
            assert_eq!(rejected.issue, Some(UpdateIssue::InvalidRelease));
            assert_eq!(rejected.last_checked_at_ms, current.last_checked_at_ms);
            assert!(notifications.load(Ordering::Acquire) >= 20);
        })
    }));
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
