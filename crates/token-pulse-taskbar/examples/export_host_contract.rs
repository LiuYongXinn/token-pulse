use std::{fs, path::PathBuf};
use token_pulse_taskbar::{Envelope, HostMessage, HostReply};
fn main() {
    let schema = serde_json::json!({"protocol_version":1,"maximum_frame_bytes":65536,"main_to_host":schemars::generate::SchemaSettings::default().for_serialize().into_generator().into_root_schema_for::<Envelope<HostMessage>>(),"host_to_main":schemars::generate::SchemaSettings::default().for_serialize().into_generator().into_root_schema_for::<Envelope<HostReply>>()});
    let expected = serde_json::to_string_pretty(&schema).unwrap() + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/taskbar-host-v1.json");
    if std::env::args().any(|arg| arg == "--check") {
        assert_eq!(
            fs::read_to_string(path).unwrap().replace("\r\n", "\n"),
            expected,
            "host contract drift"
        );
    } else {
        fs::write(path, expected).unwrap();
    }
}
