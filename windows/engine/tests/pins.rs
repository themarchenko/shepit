//! Checks on the pin table. They live outside src/pins.rs so the CI cache key (a hash of that file)
//! changes only when a pin does.

use shepit_engine::pins::PARAKEET_V3_INT8;

/// The files transcribe-rs 0.3.12 opens for Parakeet with Quantization::Int8
/// (onnx/parakeet/mod.rs:96-105, onnx/session.rs:228-242).
const OPENED: [&str; 4] = [
    "decoder_joint-model.int8.onnx",
    "encoder-model.int8.onnx",
    "nemo128.onnx",
    "vocab.txt",
];

#[test]
fn names_equal_the_files_the_recognizer_opens() {
    let mut names: Vec<&str> = PARAKEET_V3_INT8.files.iter().map(|f| f.name).collect();
    names.sort_unstable();
    assert_eq!(names, OPENED);
}

#[test]
fn every_url_uses_the_same_40_hex_commit() {
    let prefix = "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/";
    let mut commits = std::collections::HashSet::new();
    for f in PARAKEET_V3_INT8.files {
        let rest = f.url.strip_prefix(prefix).expect("pinned repo url");
        let (commit, file) = rest.split_once('/').expect("commit/file");
        assert_eq!(commit.len(), 40, "{}", f.name);
        assert!(commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        assert_eq!(file, f.name, "url must end with the file name");
        commits.insert(commit);
    }
    assert_eq!(commits.len(), 1);
}

#[test]
fn every_sha256_is_64_lowercase_hex_and_every_size_is_set() {
    assert!(!PARAKEET_V3_INT8.files.is_empty());
    for f in PARAKEET_V3_INT8.files {
        assert_eq!(f.sha256.len(), 64, "{}", f.name);
        assert!(f
            .sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        assert!(f.size > 0, "{}", f.name);
    }
}
