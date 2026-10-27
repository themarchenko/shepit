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
