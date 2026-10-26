//! Loader for the language-neutral fixtures in `shared/fixtures`. Mirrors `SharedFixtures.swift`.
#![allow(dead_code)]

use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Case<I, E> {
    pub name: String,
    pub input: I,
    pub expected: E,
}

#[derive(Debug, Deserialize)]
pub struct Envelope<I, E> {
    pub version: u32,
    pub description: String,
    pub cases: Vec<Case<I, E>>,
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/fixtures")
}

pub fn load<I, E>(name: &str, minimum_cases: usize) -> Result<Envelope<I, E>, String>
where
    I: DeserializeOwned,
    E: DeserializeOwned,
{
    load_from(&fixtures_dir().join(name), minimum_cases)
}

pub fn load_from<I, E>(path: &Path, minimum_cases: usize) -> Result<Envelope<I, E>, String>
where
    I: DeserializeOwned,
    E: DeserializeOwned,
{
    let shown = path.display();
    if minimum_cases < 1 {
        return Err(format!("minimum_cases must be at least 1 for {shown}"));
    }
    if !path.exists() {
        return Err(format!("missing fixture {shown}"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{shown}: {e}"))?;
    let file: Envelope<I, E> =
        serde_json::from_str(&text).map_err(|e| format!("{shown}: parse error: {e}"))?;
    if file.version != 1 {
        return Err(format!("{shown}: unsupported version {}", file.version));
    }
    if file.cases.len() < minimum_cases {
        return Err(format!(
            "{shown}: {} cases, expected at least {minimum_cases}",
            file.cases.len()
        ));
    }
    Ok(file)
}
