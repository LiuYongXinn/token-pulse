//! Public trust anchor only; private signing material is never read by the application.
pub(crate) fn public_key() -> &'static str {
    option_env!("TOKENPULSE_UPDATER_PUBLIC_KEY")
        .unwrap_or(include_str!("../resources/updater-public-key.txt"))
        .trim()
}
