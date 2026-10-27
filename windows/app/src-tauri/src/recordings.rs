//! The recordings folder: the last 200 dictations as 16 kHz mono 16-bit WAV files, named by UTC
//! time (RecordingStore.swift). Only `.wav` files are ever touched.

use shepit_core::audio::wav_16k_mono;
use shepit_core::history::{recording_file_name, to_prune};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The folder inside the local data directory.
pub fn dir_in(base: &Path) -> PathBuf {
    base.join("recordings")
}

/// Creates the folder if it is missing.
pub fn ensure_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

/// Writes the WAV for a dictation that ended at `now_ms` and returns its file name. Creates the
/// folder; the file appears whole or not at all.
pub fn save(dir: &Path, now_ms: i64, samples: &[f32]) -> io::Result<String> {
    ensure_dir(dir)?;
    let name = recording_file_name(now_ms);
    let tmp = dir.join(format!("{name}.tmp"));
    // A rename is atomic, so a crash never leaves a half-written .wav for prune or the user.
    fs::write(&tmp, wav_16k_mono(samples))?;
    fs::rename(&tmp, dir.join(&name)).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(name)
}

fn wav_names(dir: &Path) -> io::Result<Vec<String>> {
    match fs::read_dir(dir) {
        Ok(entries) => Ok(entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// Deletes the oldest `.wav` files beyond `keep`, in name order. A missing folder is fine.
pub fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    for name in to_prune(&wav_names(dir)?, keep) {
        let _ = fs::remove_file(dir.join(name));
    }
    Ok(())
}

/// Deletes every `.wav` file and returns how many. Other files stay. A missing folder gives 0.
pub fn clear(dir: &Path) -> io::Result<usize> {
    let names = wav_names(dir)?;
    let mut removed = 0;
    for name in names.into_iter().filter(|n| n.ends_with(".wav")) {
        if fs::remove_file(dir.join(name)).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepit_core::audio::wav_16k_mono;
    use shepit_core::history::recording_file_name;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("shepit-recordings-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> = fs::read_dir(&self.0)
                .map(|d| {
                    d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        }
        fn touch(&self, name: &str) {
            fs::create_dir_all(&self.0).unwrap();
            fs::write(self.0.join(name), b"x").unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_folder_is_recordings_inside_the_data_directory() {
        assert_eq!(
            dir_in(Path::new("data")),
            Path::new("data").join("recordings")
        );
    }

    #[test]
    fn ensure_dir_creates_the_folder_and_accepts_an_existing_one() {
        let t = TempDir::new();
        ensure_dir(&t.0).unwrap();
        assert!(t.0.is_dir());
        ensure_dir(&t.0).unwrap();
    }

    #[test]
    fn save_writes_the_wav_under_the_utc_name_and_returns_it() {
        let t = TempDir::new();
        let samples = [0.0_f32, 0.5, -0.5, 1.0];
        let name = save(&t.0, 1_700_000_000_123, &samples).unwrap();
        assert_eq!(name, "20231114-221320-123.wav");
        assert_eq!(fs::read(t.0.join(&name)).unwrap(), wav_16k_mono(&samples));
        assert_eq!(fs::read(t.0.join(&name)).unwrap().len(), 44 + 8);
    }

    #[test]
    fn save_leaves_only_the_wav_behind() {
        let t = TempDir::new();
        save(&t.0, 1_000, &[0.1; 10]).unwrap();
        save(&t.0, 2_000, &[0.1; 10]).unwrap();
        assert_eq!(
            t.names(),
            ["19700101-000001-000.wav", "19700101-000002-000.wav"]
        );
    }

    #[test]
    fn prune_keeps_the_newest_by_name() {
        let t = TempDir::new();
        for i in 0..5 {
            t.touch(&format!("2024010{i}-000000-000.wav"));
        }
        prune(&t.0, 3).unwrap();
        assert_eq!(
            t.names(),
            [
                "20240102-000000-000.wav",
                "20240103-000000-000.wav",
                "20240104-000000-000.wav"
            ]
        );
    }

    #[test]
    fn prune_ignores_other_files_and_a_missing_folder() {
        let t = TempDir::new();
        prune(&t.0, 3).unwrap();
        t.touch("a.wav");
        t.touch("notes.txt");
        t.touch("b.WAV");
        prune(&t.0, 1).unwrap();
        assert_eq!(t.names(), ["a.wav", "b.WAV", "notes.txt"]);
    }

    #[test]
    fn two_hundred_and_one_recordings_leave_the_newest_two_hundred() {
        let t = TempDir::new();
        for i in 0..201 {
            t.touch(&recording_file_name(1_700_000_000_000 + i * 1_000));
        }
        prune(&t.0, 200).unwrap();
        let names = t.names();
        assert_eq!(names.len(), 200);
        assert_eq!(names[0], recording_file_name(1_700_000_001_000));
    }

    #[test]
    fn clear_removes_every_wav_and_nothing_else() {
        let t = TempDir::new();
        t.touch("a.wav");
        t.touch("b.wav");
        t.touch("keep.txt");
        assert_eq!(clear(&t.0).unwrap(), 2);
        assert_eq!(t.names(), ["keep.txt"]);
    }

    #[test]
    fn clear_of_a_missing_folder_is_zero() {
        let t = TempDir::new();
        assert_eq!(clear(&t.0).unwrap(), 0);
    }
}
