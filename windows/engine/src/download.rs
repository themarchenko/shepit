//! Resumable, checksum-verified model download.

use crate::pins::{ModelFile, ModelSpec};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub struct Response {
    /// 200 for a full body, 206 for the range that was asked for.
    pub status: u16,
    pub body: Box<dyn Read + Send>,
}

pub trait Fetch {
    /// GET `url`; when `offset` is above zero ask for `Range: bytes=offset-`.
    fn get(&self, url: &str, offset: u64) -> Result<Response, String>;
}

pub struct UreqFetch;

impl Fetch for UreqFetch {
    fn get(&self, url: &str, offset: u64) -> Result<Response, String> {
        // No Authorization header and no token lookup: the model repo is public.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            // The hash makes other hosts safe, but never plain http (also after a redirect).
            .https_only(true)
            .build()
            .into();
        let mut req = agent.get(url);
        if offset > 0 {
            req = req.header("Range", format!("bytes={offset}-"));
        }
        let resp = req.call().map_err(|e| e.to_string())?;
        Ok(Response {
            status: resp.status().as_u16(),
            body: Box::new(resp.into_body().into_reader()),
        })
    }
}

pub fn default_models_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("Shepit").join("models"))
}

fn installed(dir: &Path, file: &ModelFile) -> bool {
    std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.is_file() && m.len() == file.size)
}

/// Existence and pinned size only. A full re-hash at every launch would read 650 MB for nothing;
/// the hash was checked before the final name existed.
pub fn is_installed(dir: &Path, spec: &ModelSpec) -> bool {
    spec.files.iter().all(|f| installed(dir, f))
}

/// No data for this long fails the download. Not a total limit: the model is 670 MB.
const STALL_LIMIT: Duration = Duration::from_secs(60);

/// Re-hash every pinned file that exists under `dir` and delete the ones that do not match, so a
/// corrupted or planted file of the right size cannot be loaded. Call `ensure` afterwards to fetch
/// what was removed. Streams each file, so memory stays flat.
pub fn verify_installed(dir: &Path, spec: &ModelSpec) -> Result<(), String> {
    for file in spec.files {
        let path = dir.join(file.name);
        if !path.is_file() {
            continue;
        }
        let hash = sha256_of(&path).map_err(|e| format!("{}: {e}", file.name))?;
        if hash != file.sha256 {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", file.name))?;
        }
    }
    Ok(())
}

/// Turns a body that goes quiet into an error. ureq has no per-read timeout (its body timeout is
/// a budget for the whole body), so a thread does the blocking reads and this side waits with a
/// limit. After a stall the thread stays blocked until the socket dies and then exits on its own.
struct StallReader {
    chunks: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    pending: Vec<u8>,
    at: usize,
    limit: Duration,
}

impl StallReader {
    fn new(mut inner: Box<dyn Read + Send>, limit: Duration) -> Self {
        let (tx, chunks) = mpsc::sync_channel(4);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let item = match inner.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => Ok(buf[..n].to_vec()),
                    Err(e) => Err(e),
                };
                let failed = item.is_err();
                if tx.send(item).is_err() || failed {
                    return;
                }
            }
        });
        Self {
            chunks,
            pending: Vec::new(),
            at: 0,
            limit,
        }
    }
}

impl Read for StallReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.at >= self.pending.len() {
            match self.chunks.recv_timeout(self.limit) {
                Ok(Ok(chunk)) => {
                    self.pending = chunk;
                    self.at = 0;
                }
                Ok(Err(e)) => return Err(e),
                // The sender hung up after a clean end of body.
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("stalled: no data for {} s", self.limit.as_secs_f32()),
                    ))
                }
            }
        }
        let n = out.len().min(self.pending.len() - self.at);
        out[..n].copy_from_slice(&self.pending[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Download every missing file of `spec` into `dir`. `progress(done, total)` counts bytes over the
/// whole spec, files already installed included.
pub fn ensure(
    dir: &Path,
    spec: &ModelSpec,
    fetch: &dyn Fetch,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    ensure_with(dir, spec, fetch, STALL_LIMIT, progress)
}

fn ensure_with(
    dir: &Path,
    spec: &ModelSpec,
    fetch: &dyn Fetch,
    stall: Duration,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let total: u64 = spec.files.iter().map(|f| f.size).sum();
    let mut done = 0u64;
    for file in spec.files {
        if !installed(dir, file) {
            download_file(dir, file, fetch, stall, &mut |n| progress(done + n, total))?;
        }
        done += file.size;
        progress(done, total);
    }
    Ok(())
}

fn download_file(
    dir: &Path,
    file: &ModelFile,
    fetch: &dyn Fetch,
    stall: Duration,
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    let final_path = dir.join(file.name);
    let part_path = dir.join(format!("{}.part", file.name));
    let io = |e: std::io::Error| format!("{}: {e}", file.name);

    let mut have = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
    if have > file.size {
        std::fs::remove_file(&part_path).map_err(io)?;
        have = 0;
    }

    if have < file.size {
        let resp = fetch.get(file.url, have)?;
        let mut out = match (have, resp.status) {
            (_, 200) => {
                // The server ignored the range (or this is a fresh start): begin again.
                have = 0;
                File::create(&part_path).map_err(io)?
            }
            (_, 206) => OpenOptions::new()
                .append(true)
                .open(&part_path)
                .map_err(io)?,
            (_, s) => return Err(format!("{}: unexpected HTTP status {s}", file.name)),
        };
        progress(have);
        let mut body = StallReader::new(resp.body, stall).take(file.size - have + 1);
        let mut buf = vec![0u8; 256 * 1024];
        let mut written = have;
        loop {
            let n = body.read(&mut buf).map_err(io)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(io)?;
            written += n as u64;
            if written > file.size {
                drop(out);
                let _ = std::fs::remove_file(&part_path);
                return Err(format!(
                    "{}: body is longer than the pinned size",
                    file.name
                ));
            }
            progress(written);
        }
        out.flush().map_err(io)?;
        drop(out);
        if written < file.size {
            // Keep the .part file so the next attempt resumes.
            return Err(format!(
                "{}: connection ended after {written} of {} bytes",
                file.name, file.size
            ));
        }
    }

    // Flushed to disk before the final name exists, so a crash cannot leave a short file there.
    OpenOptions::new()
        .write(true)
        .open(&part_path)
        .and_then(|f| f.sync_all())
        .map_err(io)?;
    if sha256_of(&part_path).map_err(io)? != file.sha256 {
        let _ = std::fs::remove_file(&part_path);
        return Err(format!("{}: checksum mismatch", file.name));
    }
    std::fs::rename(&part_path, &final_path).map_err(io)
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pins::ModelFile;
    use sha2::{Digest, Sha256};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::io::Cursor;

    fn sha_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    struct Fake {
        bodies: HashMap<String, Vec<u8>>,
        honor_range: bool,
        /// Serve only this many bytes of the body, as if the connection dropped.
        cut_at: Option<usize>,
        calls: RefCell<Vec<(String, u64)>>,
    }

    impl Fake {
        fn new(url: &str, body: &[u8]) -> Self {
            Fake {
                bodies: HashMap::from([(url.to_string(), body.to_vec())]),
                honor_range: true,
                cut_at: None,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Fetch for Fake {
        fn get(&self, url: &str, offset: u64) -> Result<Response, String> {
            self.calls.borrow_mut().push((url.to_string(), offset));
            let body = self.bodies.get(url).ok_or("404")?;
            let (status, start) = if offset > 0 && self.honor_range {
                (206, offset as usize)
            } else {
                (200, 0)
            };
            let mut bytes = body[start..].to_vec();
            if let Some(cut) = self.cut_at {
                bytes.truncate(cut);
            }
            Ok(Response {
                status,
                body: Box::new(Cursor::new(bytes)),
            })
        }
    }

    const URL: &str = "https://example.test/a.bin";

    fn payload() -> Vec<u8> {
        (0..10_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn files<'a>(body: &[u8], sha: &'a str, name: &'a str) -> Vec<ModelFile<'a>> {
        vec![ModelFile {
            name,
            url: URL,
            size: body.len() as u64,
            sha256: sha,
        }]
    }

    fn run(dir: &Path, files: &[ModelFile], fake: &Fake) -> (Result<(), String>, Vec<(u64, u64)>) {
        let spec = ModelSpec {
            dir_name: "m",
            files,
        };
        let mut seen = Vec::new();
        let r = ensure(dir, &spec, fake, &mut |d, t| seen.push((d, t)));
        (r, seen)
    }

    #[test]
    fn fresh_download_writes_final_file_and_reports_progress() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &sha, "a.bin");
        let (r, seen) = run(tmp.path(), &f, &Fake::new(URL, &body));
        r.unwrap();
        assert_eq!(std::fs::read(tmp.path().join("a.bin")).unwrap(), body);
        assert!(!tmp.path().join("a.bin.part").exists());
        assert_eq!(seen.last(), Some(&(body.len() as u64, body.len() as u64)));
        assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    #[test]
    fn checksum_mismatch_leaves_no_model_file() {
        let body = payload();
        let wrong = sha_hex(b"something else");
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &wrong, "a.bin");
        let (r, _) = run(tmp.path(), &f, &Fake::new(URL, &body));
        assert!(r.unwrap_err().contains("checksum"));
        assert!(!tmp.path().join("a.bin").exists());
        assert!(!tmp.path().join("a.bin.part").exists());
    }

    #[test]
    fn resume_sends_part_length_as_offset_and_accepts_206() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.bin.part"), &body[..4000]).unwrap();
        let fake = Fake::new(URL, &body);
        let f = files(&body, &sha, "a.bin");
        let (r, seen) = run(tmp.path(), &f, &fake);
        r.unwrap();
        assert_eq!(fake.calls.borrow().as_slice(), &[(URL.to_string(), 4000)]);
        assert_eq!(std::fs::read(tmp.path().join("a.bin")).unwrap(), body);
        assert_eq!(seen.last().unwrap().0, body.len() as u64);
    }

    #[test]
    fn status_200_on_resume_restarts_from_zero() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.bin.part"), vec![9u8; 4000]).unwrap();
        let mut fake = Fake::new(URL, &body);
        fake.honor_range = false;
        let f = files(&body, &sha, "a.bin");
        let (r, seen) = run(tmp.path(), &f, &fake);
        r.unwrap();
        assert_eq!(std::fs::read(tmp.path().join("a.bin")).unwrap(), body);
        assert_eq!(seen.last().unwrap().0, body.len() as u64);
    }

    #[test]
    fn body_longer_than_pinned_size_is_an_error() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let mut f = files(&body, &sha, "a.bin");
        f[0].size -= 10;
        let (r, _) = run(tmp.path(), &f, &Fake::new(URL, &body));
        assert!(r.unwrap_err().contains("longer"));
        assert!(!tmp.path().join("a.bin").exists());
    }

    #[test]
    fn short_body_is_an_error_and_keeps_the_part_file() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let mut fake = Fake::new(URL, &body);
        fake.cut_at = Some(3000);
        let f = files(&body, &sha, "a.bin");
        let (r, _) = run(tmp.path(), &f, &fake);
        assert!(r.is_err());
        assert!(!tmp.path().join("a.bin").exists());
        assert_eq!(
            std::fs::metadata(tmp.path().join("a.bin.part"))
                .unwrap()
                .len(),
            3000
        );
    }

    #[test]
    fn is_installed_ignores_part_files_and_wrong_sizes() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &sha, "a.bin");
        let spec = ModelSpec {
            dir_name: "m",
            files: &f,
        };
        assert!(!is_installed(tmp.path(), &spec));
        std::fs::write(tmp.path().join("a.bin.part"), &body).unwrap();
        assert!(!is_installed(tmp.path(), &spec));
        std::fs::write(tmp.path().join("a.bin"), &body[..10]).unwrap();
        assert!(!is_installed(tmp.path(), &spec));
        std::fs::write(tmp.path().join("a.bin"), &body).unwrap();
        assert!(is_installed(tmp.path(), &spec));
    }

    #[test]
    fn ensure_requests_nothing_for_installed_files() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.bin"), &body).unwrap();
        let fake = Fake::new(URL, &body);
        let f = files(&body, &sha, "a.bin");
        let (r, seen) = run(tmp.path(), &f, &fake);
        r.unwrap();
        assert!(fake.calls.borrow().is_empty());
        assert_eq!(seen.last(), Some(&(body.len() as u64, body.len() as u64)));
    }

    #[test]
    fn fetch_failure_is_reported() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &sha, "a.bin");
        let (r, _) = run(tmp.path(), &f, &Fake::new("https://other.test/x", &body));
        assert!(r.is_err());
    }

    #[test]
    fn verify_installed_removes_a_corrupted_file_of_the_right_size() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &sha, "a.bin");
        let spec = ModelSpec {
            dir_name: "m",
            files: &f,
        };
        let mut bad = body.clone();
        bad[100] ^= 0xff;
        std::fs::write(tmp.path().join("a.bin"), &bad).unwrap();
        assert!(is_installed(tmp.path(), &spec), "size alone looks fine");
        verify_installed(tmp.path(), &spec).unwrap();
        assert!(!tmp.path().join("a.bin").exists());

        let fake = Fake::new(URL, &body);
        let (r, _) = run(tmp.path(), &f, &fake);
        r.unwrap();
        assert_eq!(fake.calls.borrow().len(), 1, "ensure re-downloads it");
        assert_eq!(std::fs::read(tmp.path().join("a.bin")).unwrap(), body);
    }

    #[test]
    fn verify_installed_keeps_good_files_and_ignores_missing_ones() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &sha, "a.bin");
        let spec = ModelSpec {
            dir_name: "m",
            files: &f,
        };
        verify_installed(tmp.path(), &spec).unwrap();
        std::fs::write(tmp.path().join("a.bin"), &body).unwrap();
        verify_installed(tmp.path(), &spec).unwrap();
        assert!(is_installed(tmp.path(), &spec));
    }

    /// Yields `first`, then blocks far longer than the test's stall limit.
    struct Stalls {
        first: Cursor<Vec<u8>>,
    }

    impl Read for Stalls {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.first.read(buf)?;
            if n == 0 {
                std::thread::sleep(Duration::from_secs(30));
            }
            Ok(n)
        }
    }

    struct StallFetch(Vec<u8>);

    impl Fetch for StallFetch {
        fn get(&self, _url: &str, _offset: u64) -> Result<Response, String> {
            Ok(Response {
                status: 200,
                body: Box::new(Stalls {
                    first: Cursor::new(self.0.clone()),
                }),
            })
        }
    }

    #[test]
    fn a_stalled_body_fails_and_keeps_the_part_file() {
        let body = payload();
        let sha = sha_hex(&body);
        let tmp = tempfile::tempdir().unwrap();
        let f = files(&body, &sha, "a.bin");
        let spec = ModelSpec {
            dir_name: "m",
            files: &f,
        };
        let started = std::time::Instant::now();
        let r = ensure_with(
            tmp.path(),
            &spec,
            &StallFetch(body[..3000].to_vec()),
            Duration::from_millis(200),
            &mut |_, _| {},
        );
        assert!(r.unwrap_err().contains("stalled"));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!tmp.path().join("a.bin").exists());
        assert_eq!(
            std::fs::metadata(tmp.path().join("a.bin.part"))
                .unwrap()
                .len(),
            3000
        );
    }
}
