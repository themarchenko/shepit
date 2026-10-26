//! Pinned model files. Only data lives here, so the CI cache key (a hash of this file) changes
//! exactly when a pin changes.

/// One file of a model, fetched from a fixed repository commit.
pub struct ModelFile<'a> {
    pub name: &'a str,
    pub url: &'a str,
    pub size: u64,
    pub sha256: &'a str,
}

pub struct ModelSpec<'a> {
    /// Folder name under the models directory.
    pub dir_name: &'a str,
    pub files: &'a [ModelFile<'a>],
}

// Repo istupakov/parakeet-tdt-0.6b-v3-onnx (CC-BY-4.0), commit pinned in each url. Sizes and
// sha256 come from the Hub tree API (lfs.oid; vocab.txt is not LFS, hashed after download).
pub const PARAKEET_V3_INT8: ModelSpec<'static> = ModelSpec {
    dir_name: "parakeet-tdt-0.6b-v3-int8",
    files: &[
        ModelFile {
            name: "decoder_joint-model.int8.onnx",
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx",
            size: 18202004,
            sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
        },
        ModelFile {
            name: "encoder-model.int8.onnx",
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx",
            size: 652183999,
            sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
        },
        ModelFile {
            name: "nemo128.onnx",
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx",
            size: 139764,
            sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f",
        },
        ModelFile {
            name: "vocab.txt",
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt",
            size: 93939,
            sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

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
}
