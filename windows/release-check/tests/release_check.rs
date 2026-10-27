//! Tests for the release gate. Fixtures are throwaway minisign keys; the private halves were never
//! committed. `payload.bin.sig` is the .sig file made by `tauri signer sign` with key A.

use shepit_release_check::*;
use std::fs;
use std::path::PathBuf;

fn path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}
fn fixture(name: &str) -> Vec<u8> {
    fs::read(path(&format!("tests/fixtures/{name}"))).unwrap()
}
fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap().trim().to_string()
}
fn json(rel: &str) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path(rel)).unwrap()).unwrap()
}

const TAG: &str = "v0.3.1";

fn feed() -> String {
    build_feed(
        TAG,
        "notes",
        "2026-10-27T00:00:00Z",
        &fixture_text("payload.bin.sig"),
    )
}
fn feed_json() -> serde_json::Value {
    serde_json::from_str(&feed()).unwrap()
}

// signature

#[test]
fn signature_by_key_a_passes_against_key_a() {
    verify_signature(
        &fixture("payload.bin"),
        &fixture_text("payload.bin.sig"),
        &fixture_text("key-a.pub"),
    )
    .unwrap();
}

#[test]
fn signature_by_key_a_is_refused_for_key_b() {
    assert!(verify_signature(
        &fixture("payload.bin"),
        &fixture_text("payload.bin.sig"),
        &fixture_text("key-b.pub")
    )
    .is_err());
}

#[test]
fn a_tampered_payload_is_refused() {
    let mut data = fixture("payload.bin");
    data[0] ^= 1;
    assert!(verify_signature(
        &data,
        &fixture_text("payload.bin.sig"),
        &fixture_text("key-a.pub")
    )
    .is_err());
}

#[test]
fn a_signature_that_is_not_base64_is_refused() {
    assert!(verify_signature(
        &fixture("payload.bin"),
        "not base64 !!",
        &fixture_text("key-a.pub")
    )
    .is_err());
}

#[test]
fn a_signature_that_decodes_to_garbage_is_refused() {
    // Valid base64, but not a minisign signature.
    assert!(verify_signature(
        &fixture("payload.bin"),
        "aGVsbG8gd29ybGQ=",
        &fixture_text("key-a.pub")
    )
    .is_err());
}

// feed

#[test]
fn feed_carries_the_tag_version_one_platform_and_a_versioned_url() {
    let f = feed_json();
    assert_eq!(f["version"], "0.3.1");
    let platforms = f["platforms"].as_object().unwrap();
    assert_eq!(platforms.len(), 1);
    let entry = &platforms[PLATFORM];
    assert_eq!(
        entry["url"],
        "https://github.com/themarchenko/shepit/releases/download/v0.3.1/Shepit-setup.exe"
    );
    assert_eq!(
        entry["signature"].as_str().unwrap(),
        fixture_text("payload.bin.sig")
    );
}

#[test]
fn a_built_feed_round_trips_through_verify() {
    verify_feed(
        &feed(),
        &fixture("payload.bin"),
        TAG,
        &fixture_text("key-a.pub"),
    )
    .unwrap();
}

#[test]
fn a_feed_with_the_wrong_version_is_refused() {
    let mut f = feed_json();
    f["version"] = "0.3.0".into();
    assert!(verify_feed(
        &f.to_string(),
        &fixture("payload.bin"),
        TAG,
        &fixture_text("key-a.pub")
    )
    .is_err());
}

#[test]
fn a_feed_with_an_unversioned_url_is_refused() {
    let mut f = feed_json();
    f["platforms"][PLATFORM]["url"] =
        "https://github.com/themarchenko/shepit/releases/latest/download/Shepit-setup.exe".into();
    assert!(verify_feed(
        &f.to_string(),
        &fixture("payload.bin"),
        TAG,
        &fixture_text("key-a.pub")
    )
    .is_err());
}

#[test]
fn a_feed_with_an_extra_platform_is_refused() {
    let mut f = feed_json();
    let entry = f["platforms"][PLATFORM].clone();
    f["platforms"]["darwin-aarch64"] = entry;
    assert!(verify_feed(
        &f.to_string(),
        &fixture("payload.bin"),
        TAG,
        &fixture_text("key-a.pub")
    )
    .is_err());
}

#[test]
fn a_feed_with_the_wrong_platform_is_refused() {
    let mut f = feed_json();
    let entry = f["platforms"][PLATFORM].clone();
    f["platforms"] = serde_json::json!({ "windows-aarch64": entry });
    assert!(verify_feed(
        &f.to_string(),
        &fixture("payload.bin"),
        TAG,
        &fixture_text("key-a.pub")
    )
    .is_err());
}

#[test]
fn a_feed_over_a_tampered_installer_is_refused() {
    let mut data = fixture("payload.bin");
    data[0] ^= 1;
    assert!(verify_feed(&feed(), &data, TAG, &fixture_text("key-a.pub")).is_err());
}

// the pin

#[test]
fn pinned_key_is_the_updater_key_with_the_expected_id() {
    let decoded = String::from_utf8(
        <base64::engine::GeneralPurpose as base64::Engine>::decode(
            &base64::engine::general_purpose::STANDARD,
            PINNED_UPDATER_PUBKEY,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(decoded.contains("FC48EEB6B111E059"), "{decoded}");
    minisign_verify::PublicKey::decode(&decoded).unwrap();
}

#[test]
fn verify_release_refuses_a_build_not_signed_with_the_pinned_key() {
    // Signed by throwaway key A, so it is valid for key A and not for the pin. verify_feed shows the
    // build itself is sound; verify_release must still refuse it.
    verify_feed(
        &feed(),
        &fixture("payload.bin"),
        TAG,
        &fixture_text("key-a.pub"),
    )
    .unwrap();
    assert!(verify_release(&feed(), &fixture("payload.bin"), TAG).is_err());
}

// config files

#[test]
fn conf_pubkey_is_the_pinned_key() {
    let conf = json("../app/src-tauri/tauri.conf.json");
    assert_eq!(conf["plugins"]["updater"]["pubkey"], PINNED_UPDATER_PUBKEY);
}

#[test]
fn conf_endpoints_are_https_on_the_shepit_repo() {
    let conf = json("../app/src-tauri/tauri.conf.json");
    let endpoints = conf["plugins"]["updater"]["endpoints"].as_array().unwrap();
    assert!(!endpoints.is_empty());
    for e in endpoints {
        assert!(
            e.as_str()
                .unwrap()
                .starts_with("https://github.com/themarchenko/shepit/"),
            "{e}"
        );
    }
}

#[test]
fn conf_has_no_dangerous_updater_keys() {
    let conf = json("../app/src-tauri/tauri.conf.json");
    let updater = conf["plugins"]["updater"]
        .as_object()
        .expect("plugins.updater is missing");
    assert!(
        updater.keys().all(|k| !k.starts_with("dangerous")),
        "{updater:?}"
    );
}

#[test]
fn conf_installs_passively_and_per_user() {
    let conf = json("../app/src-tauri/tauri.conf.json");
    assert_eq!(
        conf["plugins"]["updater"]["windows"]["installMode"],
        "passive"
    );
    assert_eq!(
        conf["bundle"]["windows"]["nsis"]["installMode"],
        "currentUser"
    );
}

// The release build uses tauri.runtime.conf.json and signs the staged installer in a separate step
// (`tauri signer sign`), so the signing key never reaches `tauri build`, the Vite build or a build.rs.

#[test]
fn build_conf_never_creates_updater_artifacts() {
    let runtime = json("../app/src-tauri/tauri.runtime.conf.json");
    assert!(runtime["bundle"].get("createUpdaterArtifacts").is_none());
}

#[test]
fn build_conf_has_no_plugins_key() {
    let runtime = json("../app/src-tauri/tauri.runtime.conf.json");
    assert!(runtime.get("plugins").is_none());
}

#[test]
fn there_is_no_release_conf_that_turns_signing_into_the_build() {
    assert!(!path("../app/src-tauri/tauri.release.conf.json").exists());
}

fn conf_pair() -> (serde_json::Value, serde_json::Value) {
    (
        json("../app/src-tauri/tauri.conf.json"),
        json("../app/src-tauri/tauri.runtime.conf.json"),
    )
}

#[test]
fn check_config_passes_for_the_repo_files() {
    let (conf, runtime) = conf_pair();
    check_config(&conf, &runtime).unwrap();
}

#[test]
fn check_config_requires_signed_version() {
    let (mut conf, runtime) = conf_pair();
    conf["plugins"]["updater"]["requireSignedVersion"] = false.into();
    assert!(check_config(&conf, &runtime).is_err());
    conf["plugins"]["updater"]
        .as_object_mut()
        .unwrap()
        .remove("requireSignedVersion");
    assert!(check_config(&conf, &runtime).is_err());
}

#[test]
fn check_config_accepts_only_the_exact_endpoint() {
    let (mut conf, runtime) = conf_pair();
    conf["plugins"]["updater"]["endpoints"] = serde_json::json!([
        "https://github.com/themarchenko/shepit/releases/latest/download/latest.json",
        "https://github.com/themarchenko/shepit/raw/main/latest.json"
    ]);
    assert!(check_config(&conf, &runtime).is_err());
    conf["plugins"]["updater"]["endpoints"] = serde_json::json!([
        "https://github.com/themarchenko/shepit/releases/latest/download/latest.json?x=1"
    ]);
    assert!(check_config(&conf, &runtime).is_err());
}

#[test]
fn check_config_refuses_updater_artifacts_in_the_build_conf() {
    let (conf, mut runtime) = conf_pair();
    runtime["bundle"]["createUpdaterArtifacts"] = true.into();
    assert!(check_config(&conf, &runtime).is_err());
}

#[test]
fn base_conf_does_not_create_updater_artifacts() {
    // PR builds have no signing secret and must stay green.
    let conf = json("../app/src-tauri/tauri.conf.json");
    assert!(conf["bundle"].get("createUpdaterArtifacts").is_none());
}
