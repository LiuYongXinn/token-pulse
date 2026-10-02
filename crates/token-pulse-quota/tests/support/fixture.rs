//! Explicitly enabled synthetic protocol server; never part of a normal application build.
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    time::Duration,
};
fn send(value: Value) {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &value).unwrap();
    stdout.write_all(b"\n").unwrap();
    stdout.flush().unwrap();
}
fn read(reader: &mut impl BufRead) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
fn limits(used: i32) -> Value {
    json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":used,"windowDurationMins":300,"resetsAt":1790900000}}})
}
fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("CODEX_HOME").unwrap());
    if std::env::args().nth(1).as_deref() == Some("synthetic-descendant") {
        std::fs::write(home.join("descendant.pid"), std::process::id().to_string()).unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    assert_eq!(
        std::env::args().skip(1).collect::<Vec<_>>(),
        vec!["app-server"]
    );
    std::fs::write(home.join("service.pid"), std::process::id().to_string()).unwrap();
    let mode = std::fs::read_to_string(home.join("fixture-mode")).unwrap();
    let mut stdin = io::stdin().lock();
    let init = read(&mut stdin);
    assert_eq!(init["method"], "initialize");
    assert_eq!(init["params"]["clientInfo"]["name"], "token_pulse");
    assert_eq!(init["params"]["capabilities"]["experimentalApi"], false);
    if mode == "bad-init" {
        send(json!({"id":init["id"],"result":{"userAgent":null}}));
        return;
    }
    if mode == "unsupported" {
        send(json!({"id":init["id"],"error":{"code":-32601,"message":"SECRET"}}));
        return;
    }
    if mode == "stderr" {
        io::stderr().write_all(&vec![b'S'; 2_000_000]).unwrap();
    }
    send(
        json!({"id":init["id"],"result":{"userAgent":"synthetic/1", "codexHome":"SECRET", "accessToken":"SECRET"}}),
    );
    assert_eq!(
        read(&mut stdin),
        json!({"method":"initialized","params":{}})
    );
    if mode == "descendant" {
        #[cfg(windows)]
        {
            // Inherit pipes to prove job cleanup unblocks both owned workers.
            let mut descendant = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("synthetic-descendant")
                .spawn()
                .unwrap();
            let _ = descendant.wait();
            return;
        }
    }
    if mode == "flood" {
        for _ in 0..128 {
            send(json!({"method":"account/updated","params":{"email":"SECRET"}}));
        }
    }
    let mut limits_reads = 0;
    let mut account_reads = 0;
    loop {
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let request: Value = serde_json::from_str(&line).unwrap();
        let method = request["method"].as_str().unwrap();
        match method {
            "account/read" => {
                account_reads += 1;
                assert_eq!(request["params"], json!({"refreshToken":false}));
                if mode == "account-change" && account_reads == 1 {
                    send(json!({"method":"account/updated","params":{"email":"SECRET"}}));
                }
                if mode == "auth-required" || (mode == "switch-account" && account_reads > 1) {
                    send(
                        json!({"id":request["id"],"result":{"requiresOpenaiAuth":true,"account":null}}),
                    );
                } else if mode == "unsupported-account" {
                    send(
                        json!({"id":request["id"],"result":{"requiresOpenaiAuth":true,"account":{"type":"apiKey", "secret":"SECRET"}}}),
                    );
                } else if mode != "silent" {
                    send(
                        json!({"id":request["id"],"result":{"requiresOpenaiAuth":true,"account":{"type":"chatgpt", "email":"SECRET", "token":"SECRET"}}}),
                    );
                }
            }
            "account/rateLimits/read" => {
                assert!(
                    mode != "auth-required" && mode != "unsupported-account",
                    "must not query quota without an eligible account"
                );
                assert!(request.get("params").is_none());
                limits_reads += 1;
                match mode.as_str() {
                    "silent" => continue,
                    "malformed" => {
                        io::stdout().write_all(b"not JSON SECRET\n").unwrap();
                        io::stdout().flush().unwrap();
                        continue;
                    }
                    "oversized" => {
                        let mut bytes = vec![b' '; 1_048_577];
                        bytes.push(b'\n');
                        io::stdout().write_all(&bytes).unwrap();
                        io::stdout().flush().unwrap();
                        continue;
                    }
                    "truncated" => {
                        io::stdout().write_all(b"{\"secret\":\"SECRET\"}").unwrap();
                        return;
                    }
                    "invalid-shape" => {
                        send(
                            json!({"id":request["id"],"result":{},"error":{"code":401,"message":"SECRET"}}),
                        );
                        continue;
                    }
                    "late" if limits_reads == 1 => {
                        std::thread::sleep(Duration::from_millis(10_200))
                    }
                    "normal" => {
                        send(json!({"method":"account/rateLimits/updated","params":limits(40)}));
                        send(json!({"method":"thread/sensitive","params":{"text":"SECRET"}}));
                        send(json!({"id":"old-epoch:19","result":{"secret":"SECRET"}}));
                        send(
                            json!({"id":777,"method":"item/commandExecution/requestApproval","params":{"command":"DO NOT RUN"}}),
                        );
                        assert_eq!(
                            read(&mut stdin),
                            json!({"id":777,"error":{"code":-32601,"message":"Unsupported method"}})
                        );
                    }
                    _ => {}
                }
                if mode == "service" {
                    send(json!({"id":request["id"],"result":{"rateLimitsByLimitId":{
                        "codex":{"limitId":"codex","primary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":4102444800_i64}},
                        "other":{"limitId":"other","primary":{"usedPercent":80,"windowDurationMins":10080,"resetsAt":4102444800_i64}}
                    },"rateLimits":null}}));
                    continue;
                }
                send(
                    json!({"id":request["id"],"result":limits(if limits_reads == 1 { 50 } else { 20 })}),
                );
                if mode == "exit-after-read" {
                    std::thread::sleep(Duration::from_millis(400));
                    return;
                }
                if mode == "switch-account" && limits_reads == 1 {
                    send(json!({"method":"account/updated","params":{"email":"SECRET"}}));
                    send(json!({"method":"account/rateLimits/updated","params":limits(0)}));
                }
            }
            _ => panic!("non-allowlisted outgoing method"),
        }
    }
}
