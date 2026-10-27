//! `shepit-release-check feed ...` writes latest.json; `shepit-release-check verify ...` is the
//! release gate. Both exit nonzero with one line on stderr when something is wrong.

use shepit_release_check::*;
use std::{env, fs, process::ExitCode};

fn flag(args: &[String], name: &str) -> Result<String, VerifyError> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
        .ok_or_else(|| VerifyError(format!("missing {name}")))
}

fn read(path: &str) -> Result<Vec<u8>, VerifyError> {
    fs::read(path).map_err(|e| VerifyError(format!("cannot read {path}: {e}")))
}

fn read_json(path: &str) -> Result<serde_json::Value, VerifyError> {
    serde_json::from_slice(&read(path)?).map_err(|e| VerifyError(format!("{path}: {e}")))
}

fn feed(args: &[String]) -> Result<(), VerifyError> {
    let tag = flag(args, "--tag")?;
    version_of_tag(&tag)?;
    let sig = String::from_utf8(read(&flag(args, "--sig")?)?)
        .map_err(|_| VerifyError("signature file is not UTF-8".into()))?;
    let notes = flag(args, "--notes").unwrap_or_default();
    let out = flag(args, "--out")?;
    let json = build_feed(&tag, &notes, &flag(args, "--pub-date")?, &sig);
    fs::write(&out, json + "\n").map_err(|e| VerifyError(format!("cannot write {out}: {e}")))
}

fn verify(args: &[String]) -> Result<(), VerifyError> {
    let tag = flag(args, "--tag")?;
    let feed = String::from_utf8(read(&flag(args, "--feed")?)?)
        .map_err(|_| VerifyError("feed is not UTF-8".into()))?;
    let installer = read(&flag(args, "--installer")?)?;
    let dir = flag(args, "--conf-dir").unwrap_or_else(|_| "app/src-tauri".into());
    check_config(
        &read_json(&format!("{dir}/tauri.conf.json"))?,
        &read_json(&format!("{dir}/tauri.runtime.conf.json"))?,
    )?;
    verify_release(&feed, &installer, &tag)?;
    verify_release_strict(&feed, &installer, &tag)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("feed") => feed(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => Err(VerifyError(
            "usage: shepit-release-check feed|verify --tag vX.Y.Z ...".into(),
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("release-check: {e}");
            ExitCode::FAILURE
        }
    }
}
