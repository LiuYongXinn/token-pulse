//! Opt-in synthetic command fixture; no product installer resource, provider config or account.
fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("success") => {
            if arguments.len() != 3 || arguments[1] != "space quote \" and 中文" {
                std::process::exit(31);
            }
            let value: serde_json::Value = serde_json::from_str(&arguments[2]).unwrap();
            if value
                != serde_json::json!({"type":"agent-turn-complete","thread-id":"original-thread","turn-id":"original-turn","input-messages":["synthetic body \"; & |"],"last-assistant-message":"合成", "cwd":"ignored-directory"})
            {
                std::process::exit(32);
            }
            // A successful wrapper must discard both streams, including a real large output.
            use std::io::Write;
            let _ = std::io::stdout().write_all(&vec![b'x'; 131072]);
            let _ = std::io::stderr().write_all(b"synthetic private output");
        }
        Some("failure") => std::process::exit(17),
        Some("tree") => {
            let marker = &arguments[1];
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["linger", marker])
                .spawn()
                .unwrap();
            std::fs::write(marker, child.id().to_string()).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(30));
            let _ = child.wait();
        }
        Some("linger") => std::thread::sleep(std::time::Duration::from_secs(30)),
        _ => std::process::exit(30),
    }
}
