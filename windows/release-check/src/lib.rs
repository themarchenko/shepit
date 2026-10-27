//! Release gate for the Windows updater feed. `feed` writes latest.json, `verify` refuses a build
//! the installed app's updater would refuse, plus the config rules the pin depends on.

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde_json::{json, Value};
use std::fmt;

/// The updater public key (minisign, base64 of the .pub file), key id FC48EEB6B111E059. Release
/// verification uses this constant, not the config file, like the hard-coded leaf hash on macOS.
pub const PINNED_UPDATER_PUBKEY: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEZDNDhFRUI2QjExMUUwNTkKUldSWjRCR3h0dTVJL0xZMTBWaDFQNnRzWnZRV21vbFpHdU9oMnR4RWxLVFRwRVdJWWhtc0FwdEgK";

/// The one platform key the feed may carry.
pub const PLATFORM: &str = "windows-x86_64";

#[derive(Debug)]
pub struct VerifyError(pub String);

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VerifyError {}

fn err<T>(msg: impl Into<String>) -> Result<T, VerifyError> {
    Err(VerifyError(msg.into()))
}

/// tauri-plugin-updater 2.14.0 updater.rs:1980-1986 (`base64_to_string`): standard base64, then UTF-8.
fn base64_to_string(b64: &str) -> Result<String, VerifyError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| VerifyError(format!("not valid base64: {e}")))?;
    String::from_utf8(bytes).map_err(|_| VerifyError("decoded text is not UTF-8".into()))
}

/// Decodes the key and signature and verifies `data`; returns the trusted comment. Same steps as
/// `verify_signature` in tauri-plugin-updater 2.14.0 (updater.rs:1893-1908): decode the public key,
/// decode the signature, then `public_key.verify(data, &signature, true)`, where `true` allows the
/// legacy (non-prehashed) signature form exactly as the plugin does.
fn verify_and_comment(
    data: &[u8],
    signature_b64: &str,
    pubkey_b64: &str,
) -> Result<String, VerifyError> {
    let key_text = base64_to_string(pubkey_b64)?;
    let public_key = PublicKey::decode(&key_text)
        .map_err(|e| VerifyError(format!("public key does not decode: {e}")))?;
    let sig_text = base64_to_string(signature_b64)?;
    let signature = Signature::decode(&sig_text)
        .map_err(|e| VerifyError(format!("signature does not decode: {e}")))?;
    public_key
        .verify(data, &signature, true)
        .map_err(|e| VerifyError(format!("signature does not match: {e}")))?;
    Ok(signature.trusted_comment().to_string())
}

/// Decodes the base64 key and signature and checks the signature over `data`. Same steps as
/// `verify_signature` in tauri-plugin-updater 2.14.0 (updater.rs:1893-1908), without the signed
/// version check that follows there; see [`verify_signature_for_version`].
pub fn verify_signature(
    data: &[u8],
    signature_b64: &str,
    pubkey_b64: &str,
) -> Result<(), VerifyError> {
    verify_and_comment(data, signature_b64, pubkey_b64).map(|_| ())
}

/// The `version:` field of the trusted comment, updater.rs:1962-1966 (`signed_version`). The CLI
/// writes tab separated `key:value` pairs.
fn signed_version(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
}

/// The whole of plugin `verify_signature` (updater.rs:1893-1920 and `verify_signed_version`
/// :1927-1958). The version is read from the trusted comment only after the signature checked out,
/// as the plugin does. A missing field passes unless `require_signed_version`, which is the plugin's
/// `requireSignedVersion` setting. The plugin compares as semver; the feed version here is always a
/// plain `x.y.z`, for which equality and string equality agree.
pub fn verify_signature_for_version(
    data: &[u8],
    signature_b64: &str,
    pubkey_b64: &str,
    announced_version: &str,
    require_signed_version: bool,
) -> Result<(), VerifyError> {
    let comment = verify_and_comment(data, signature_b64, pubkey_b64)?;
    match signed_version(&comment) {
        None if require_signed_version => err("signature does not carry the version it signs"),
        None => Ok(()),
        Some(signed) => {
            if signed.trim_start_matches('v') == announced_version.trim_start_matches('v') {
                Ok(())
            } else {
                err(format!(
                    "signature is for version {signed}, the feed announces {announced_version}"
                ))
            }
        }
    }
}

/// `v` followed by `x.y.z` of digits only. Anything else would put odd text into a download url.
pub fn version_of_tag(tag: &str) -> Result<&str, VerifyError> {
    let version = tag.strip_prefix('v').unwrap_or("");
    let parts: Vec<&str> = version.split('.').collect();
    let ok = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        Ok(version)
    } else {
        err(format!("tag {tag:?} is not vX.Y.Z"))
    }
}

fn installer_url(tag: &str) -> String {
    format!("https://github.com/themarchenko/shepit/releases/download/{tag}/Shepit-setup.exe")
}

/// Builds latest.json for `tag` (for example `v0.3.1`). The url is the versioned release asset.
pub fn build_feed(tag: &str, notes: &str, pub_date: &str, signature_b64: &str) -> String {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    let feed = json!({
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": {
            PLATFORM: { "signature": signature_b64.trim(), "url": installer_url(tag) }
        }
    });
    serde_json::to_string_pretty(&feed).expect("a json value always serializes")
}

/// Checks the feed fields (version equals the tag without `v`, a single `windows-x86_64` entry,
/// versioned url) and the signature over `installer` against `pubkey_b64`. The signed version is
/// checked when the signature carries one, as the plugin does by default.
pub fn verify_feed(
    feed_json: &str,
    installer: &[u8],
    tag: &str,
    pubkey_b64: &str,
) -> Result<(), VerifyError> {
    verify_feed_with(feed_json, installer, tag, pubkey_b64, false)
}

/// [`verify_feed`] with the plugin's `requireSignedVersion` choice.
pub fn verify_feed_with(
    feed_json: &str,
    installer: &[u8],
    tag: &str,
    pubkey_b64: &str,
    require_signed_version: bool,
) -> Result<(), VerifyError> {
    let version = version_of_tag(tag)?;
    let feed: Value = serde_json::from_str(feed_json)
        .map_err(|e| VerifyError(format!("feed is not JSON: {e}")))?;
    if feed["version"].as_str() != Some(version) {
        return err(format!(
            "feed version is {}, expected {version}",
            feed["version"]
        ));
    }
    let platforms = feed["platforms"]
        .as_object()
        .ok_or_else(|| VerifyError("feed has no platforms".into()))?;
    if platforms.len() != 1 || !platforms.contains_key(PLATFORM) {
        let keys: Vec<&String> = platforms.keys().collect();
        return err(format!(
            "feed platforms must be only {PLATFORM}, found {keys:?}"
        ));
    }
    let entry = &platforms[PLATFORM];
    let expected_url = installer_url(tag);
    if entry["url"].as_str() != Some(expected_url.as_str()) {
        return err(format!(
            "feed url is {}, expected {expected_url}",
            entry["url"]
        ));
    }
    let signature = entry["signature"]
        .as_str()
        .ok_or_else(|| VerifyError("feed entry has no signature".into()))?;
    verify_signature_for_version(
        installer,
        signature,
        pubkey_b64,
        version,
        require_signed_version,
    )
}

/// `verify_feed` against [`PINNED_UPDATER_PUBKEY`].
pub fn verify_release(feed_json: &str, installer: &[u8], tag: &str) -> Result<(), VerifyError> {
    verify_feed(feed_json, installer, tag, PINNED_UPDATER_PUBKEY)
}

/// [`verify_release`] with `requireSignedVersion`, what the release workflow runs: `tauri build`
/// writes `version:` into the trusted comment, so a release without it was not built the expected way.
pub fn verify_release_strict(
    feed_json: &str,
    installer: &[u8],
    tag: &str,
) -> Result<(), VerifyError> {
    verify_feed_with(feed_json, installer, tag, PINNED_UPDATER_PUBKEY, true)
}

/// The one update endpoint the app may carry.
pub const UPDATE_ENDPOINT: &str =
    "https://github.com/themarchenko/shepit/releases/latest/download/latest.json";

/// The config rules the updater trust depends on, over the parsed `tauri.conf.json` and
/// `tauri.runtime.conf.json`. The release build uses the runtime config and signs the installer in a
/// separate step, so the signing key is never in the environment of `tauri build`.
pub fn check_config(conf: &Value, runtime: &Value) -> Result<(), VerifyError> {
    let updater = &conf["plugins"]["updater"];
    if updater["pubkey"].as_str() != Some(PINNED_UPDATER_PUBKEY) {
        return err("tauri.conf.json plugins.updater.pubkey is not the pinned key");
    }
    let obj = updater
        .as_object()
        .ok_or_else(|| VerifyError("plugins.updater is missing".into()))?;
    if let Some(k) = obj.keys().find(|k| k.starts_with("dangerous")) {
        return err(format!("plugins.updater.{k} is not allowed"));
    }
    if updater["requireSignedVersion"] != true {
        return err("plugins.updater.requireSignedVersion must be true");
    }
    if updater["endpoints"].as_array().map(Vec::as_slice) != Some(&[Value::from(UPDATE_ENDPOINT)]) {
        return err(format!(
            "plugins.updater.endpoints must be exactly [{UPDATE_ENDPOINT}]"
        ));
    }
    if updater["windows"]["installMode"] != "passive" {
        return err("plugins.updater.windows.installMode must be passive");
    }
    if conf["bundle"]["windows"]["nsis"]["installMode"] != "currentUser" {
        return err("bundle.windows.nsis.installMode must be currentUser");
    }
    if conf["bundle"].get("createUpdaterArtifacts").is_some() {
        return err("tauri.conf.json must not create updater artifacts (PR builds have no key)");
    }
    if runtime["bundle"].get("createUpdaterArtifacts").is_some() {
        return err(
            "tauri.runtime.conf.json (the release build config) must not create updater artifacts",
        );
    }
    if runtime.get("plugins").is_some() {
        return err("tauri.runtime.conf.json must not carry a plugins key");
    }
    Ok(())
}
