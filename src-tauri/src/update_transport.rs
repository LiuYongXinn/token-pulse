//! Native-only publication and verified downloads. No renderer URL, key or file inputs.
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Url;
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};
use token_pulse_core::{
    numeric::EpochMs,
    updates::{UpdateIssue, UpdateRelease},
};

const ENDPOINT: &str =
    "https://github.com/LiuYongXinn/token-pulse/releases/latest/download/latest.json";
const REPOSITORY_RELEASES: &str = "/LiuYongXinn/token-pulse/releases/";

#[derive(Clone)]
enum SourcePolicy {
    Production,
    #[cfg(test)]
    Fixture(u16),
}
impl SourcePolicy {
    fn credentials_absent(url: &Url) -> bool {
        url.username().is_empty() && url.password().is_none() && url.fragment().is_none()
    }
    fn artifact_allowed(&self, url: &Url) -> bool {
        if !Self::credentials_absent(url) {
            return false;
        }
        match self {
            Self::Production => {
                url.scheme() == "https"
                    && url.host_str() == Some("github.com")
                    && url.port_or_known_default() == Some(443)
                    && url
                        .path()
                        .starts_with(&format!("{REPOSITORY_RELEASES}download/"))
                    && url.path().ends_with(".exe")
                    && url.query().is_none()
            }
            #[cfg(test)]
            Self::Fixture(port) => {
                url.scheme() == "http"
                    && url.host_str() == Some("127.0.0.1")
                    && url.port() == Some(*port)
                    && url.path() == "/installer.exe"
                    && url.query().is_none()
            }
        }
    }
    fn redirect_allowed(&self, url: &Url) -> bool {
        if !Self::credentials_absent(url) {
            return false;
        }
        match self {
            Self::Production => {
                url.scheme() == "https"
                    && url.port_or_known_default() == Some(443)
                    && (url.host_str() == Some("github.com")
                        && url.path().starts_with(REPOSITORY_RELEASES)
                        || matches!(
                            url.host_str(),
                            Some(
                                "release-assets.githubusercontent.com"
                                    | "objects.githubusercontent.com"
                            )
                        ))
            }
            #[cfg(test)]
            Self::Fixture(port) => {
                url.scheme() == "http"
                    && url.host_str() == Some("127.0.0.1")
                    && url.port() == Some(*port)
            }
        }
    }
    fn https_only(&self) -> bool {
        match self {
            Self::Production => true,
            #[cfg(test)]
            Self::Fixture(_) => false,
        }
    }
}

#[derive(Clone)]
pub(super) struct Publication {
    endpoint: Url,
    public_key: String,
    policy: SourcePolicy,
}
impl Publication {
    pub fn production(public_key: &str) -> Result<Self, UpdateIssue> {
        validate_public_key(public_key)?;
        Ok(Self {
            endpoint: Url::parse(ENDPOINT).map_err(|_| UpdateIssue::PublicationNotConfigured)?,
            public_key: public_key.into(),
            policy: SourcePolicy::Production,
        })
    }
    #[cfg(test)]
    fn fixture(port: u16, public_key: &str) -> Result<Self, UpdateIssue> {
        validate_public_key(public_key)?;
        Ok(Self {
            endpoint: Url::parse(&format!("http://127.0.0.1:{port}/latest.json")).unwrap(),
            public_key: public_key.into(),
            policy: SourcePolicy::Fixture(port),
        })
    }
    pub async fn check<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<Option<Candidate>, UpdateIssue> {
        // Require signed version binding even for legacy signatures; never silently downgrade
        // signature policy through a missing or incompatible application configuration.
        let config = app
            .config()
            .plugins
            .0
            .get("updater")
            .ok_or(UpdateIssue::PublicationNotConfigured)?;
        if config
            .get("requireSignedVersion")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
            || [
                "dangerousInsecureTransportProtocol",
                "dangerousAcceptInvalidCerts",
                "dangerousAcceptInvalidHostnames",
                "allowDowngrades",
            ]
            .iter()
            .any(|field| config.get(*field).and_then(serde_json::Value::as_bool) == Some(true))
        {
            return Err(UpdateIssue::InvalidRelease);
        }
        let policy = self.policy.clone();
        let updater = app
            .updater_builder()
            .pubkey(self.public_key.clone())
            .endpoints(vec![self.endpoint.clone()])
            .map_err(issue)?
            .timeout(Duration::from_secs(30))
            .version_comparator(|current, release| release.version > current)
            .configure_client(move |client| {
                let redirect_policy = policy.clone();
                client
                    .https_only(policy.https_only())
                    .connect_timeout(Duration::from_secs(10))
                    .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                        if attempt.previous().len() < 8
                            && redirect_policy.redirect_allowed(attempt.url())
                        {
                            attempt.follow()
                        } else {
                            attempt.stop()
                        }
                    }))
            })
            .build()
            .map_err(issue)?;
        let Some(mut update) = updater.check().await.map_err(issue)? else {
            return Ok(None);
        };
        if !self.policy.artifact_allowed(&update.download_url) || update.signature.len() > 16384 {
            return Err(UpdateIssue::InvalidRelease);
        }
        let signature =
            decode_text(&update.signature).map_err(|_| UpdateIssue::SignatureInvalid)?;
        minisign_verify::Signature::decode(&signature)
            .map_err(|_| UpdateIssue::SignatureInvalid)?;
        // Downloads can take longer than the small release-manifest request.
        update.timeout = Some(Duration::from_secs(300));
        let metadata = UpdateRelease {
            version: update.version.clone(),
            notes: update.body.clone(),
            published_at_ms: update
                .date
                .map(|date| {
                    let ms = i64::try_from(date.unix_timestamp_nanos() / 1_000_000)
                        .map_err(|_| UpdateIssue::InvalidRelease)?;
                    EpochMs::new(ms).map_err(|_| UpdateIssue::InvalidRelease)
                })
                .transpose()?,
        };
        metadata
            .validate()
            .map_err(|_| UpdateIssue::InvalidRelease)?;
        Ok(Some(Candidate { update, metadata }))
    }
}
fn decode_text(encoded: &str) -> Result<String, ()> {
    if encoded.is_empty() || encoded.len() > 16384 {
        return Err(());
    }
    String::from_utf8(STANDARD.decode(encoded).map_err(|_| ())?).map_err(|_| ())
}
fn validate_public_key(encoded: &str) -> Result<(), UpdateIssue> {
    if encoded.len() > 4096 {
        return Err(UpdateIssue::PublicationNotConfigured);
    }
    let text = decode_text(encoded).map_err(|_| UpdateIssue::PublicationNotConfigured)?;
    minisign_verify::PublicKey::decode(&text).map_err(|_| UpdateIssue::PublicationNotConfigured)?;
    Ok(())
}
pub(super) fn issue(error: tauri_plugin_updater::Error) -> UpdateIssue {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => UpdateIssue::SignatureInvalid,
        Error::Reqwest(_) | Error::Network(_) | Error::ReleaseNotFound => UpdateIssue::Network,
        Error::EmptyEndpoints => UpdateIssue::PublicationNotConfigured,
        Error::Io(_) | Error::TempDirNotFound | Error::FailedToDetermineExtractPath => {
            UpdateIssue::InstallerUnavailable
        }
        Error::AuthenticationFailed | Error::PackageInstallFailed => UpdateIssue::InstallFailed,
        _ => UpdateIssue::InvalidRelease,
    }
}
#[derive(Clone)]
pub(super) struct Candidate {
    update: Update,
    metadata: UpdateRelease,
}
impl Candidate {
    pub fn metadata(&self) -> UpdateRelease {
        self.metadata.clone()
    }
    pub async fn download<C: FnMut(usize, Option<u64>), F: FnOnce()>(
        &self,
        chunk: C,
        finish: F,
    ) -> Result<VerifiedDownload, UpdateIssue> {
        let bytes = self.update.download(chunk, finish).await.map_err(issue)?;
        Ok(VerifiedDownload {
            update: self.update.clone(),
            bytes,
        })
    }
}
/// Constructed only by the real download/signature/version verifier. Not serializable.
pub(super) struct VerifiedDownload {
    update: Update,
    bytes: Vec<u8>,
}
impl VerifiedDownload {
    pub fn launch_installer(&self) -> Result<(), UpdateIssue> {
        super::update_installer::launch(&self.bytes)
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn version(&self) -> &str {
        &self.update.version
    }
}

#[cfg(test)]
mod tests;
