//! The child's environment and its stderr log, as plain functions so tests need no process state.

use shepit_core::polish::{claude_wire, codex_wire};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// A log that grows past this is started over on the next spawn (the macOS rule, 1 MB).
pub const STDERR_LOG_LIMIT: u64 = 1_000_000;

fn same_key(a: &str, b: &str) -> bool {
    // Windows environment names ignore case, and `Path` is how Windows spells PATH.
    a.eq_ignore_ascii_case(b)
}

/// The environment of an agent process: `base` without the variables that make Claude Code think
/// it runs inside another session, the executable's own folder first on PATH (an npm install is a
/// Node script and `node` sits next to it), then `extra` on top. HOME is left alone: Windows
/// programs read the profile from USERPROFILE.
pub fn child_env(
    base: impl IntoIterator<Item = (String, String)>,
    executable: &Path,
    extra: &[(String, String)],
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = base
        .into_iter()
        .filter(|(k, _)| !claude_wire::SCRUBBED_ENV.iter().any(|s| same_key(k, s)))
        .collect();
    let path_at = env.iter().position(|(k, _)| same_key(k, "PATH"));
    let inherited = path_at.map(|i| env[i].1.as_str());
    let path = codex_wire::child_path(&executable.to_string_lossy(), inherited);
    match path_at {
        Some(i) => env[i].1 = path,
        None if !path.is_empty() => env.push(("PATH".to_string(), path)),
        None => {}
    }
    for (k, v) in extra {
        match env.iter().position(|(ek, _)| same_key(ek, k)) {
            Some(i) => env[i] = (k.clone(), v.clone()),
            None => env.push((k.clone(), v.clone())),
        }
    }
    env
}

/// Removes `path` when it is larger than `limit`, so the next open starts an empty file.
pub fn restart_if_large(path: &Path, limit: u64) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > limit => std::fs::remove_file(path).is_ok(),
        _ => false,
    }
}

/// Opens the stderr log for appending, after the size check, and writes a header line that marks
/// where this spawn starts. `None` when the file cannot be opened: the child then gets no stderr.
pub fn open_stderr_log(path: &Path, limit: u64, label: &str) -> Option<File> {
    restart_if_large(path, limit);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(file, "\n--- {label} spawned at unix {secs} ---");
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn kv(k: &str, v: &str) -> (String, String) {
        (k.to_string(), v.to_string())
    }

    fn get<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn the_claude_session_variables_are_removed() {
        let base = vec![
            kv("CLAUDECODE", "1"),
            kv("CLAUDE_CODE_ENTRYPOINT", "cli"),
            kv("CLAUDE_CODE_SSE_PORT", "1234"),
            kv("CLAUDE_CONFIG_DIR", r"C:\keep"),
            kv("USERPROFILE", r"C:\Users\me"),
        ];
        let env = child_env(base, &PathBuf::from(r"C:\x\claude.exe"), &[]);
        for gone in claude_wire::SCRUBBED_ENV {
            assert_eq!(get(&env, gone), None, "{gone}");
        }
        assert_eq!(get(&env, "CLAUDE_CONFIG_DIR"), Some(r"C:\keep"));
        assert_eq!(get(&env, "USERPROFILE"), Some(r"C:\Users\me"));
    }

    #[test]
    fn the_scrub_ignores_case() {
        let env = child_env(vec![kv("ClaudeCode", "1")], Path::new(r"C:\x\c.exe"), &[]);
        assert_eq!(get(&env, "ClaudeCode"), None);
    }

    #[test]
    fn the_executable_folder_goes_first_on_path() {
        let base = vec![kv("PATH", r"C:\Windows\System32;C:\Windows")];
        let env = child_env(
            base,
            Path::new(r"C:\Users\me\AppData\Roaming\npm\codex.cmd"),
            &[],
        );
        assert_eq!(
            get(&env, "PATH"),
            Some(r"C:\Users\me\AppData\Roaming\npm;C:\Windows\System32;C:\Windows")
        );
    }

    #[test]
    fn the_path_variable_keeps_its_own_spelling() {
        let env = child_env(
            vec![kv("Path", r"C:\Windows")],
            Path::new(r"C:\x\c.exe"),
            &[],
        );
        assert_eq!(get(&env, "Path"), Some(r"C:\x;C:\Windows"));
        assert_eq!(get(&env, "PATH"), None);
    }

    #[test]
    fn a_missing_path_is_created_from_the_executable_folder() {
        let env = child_env(Vec::new(), Path::new(r"C:\x\c.exe"), &[]);
        assert_eq!(get(&env, "PATH"), Some(r"C:\x"));
    }

    #[test]
    fn home_is_not_set() {
        let env = child_env(vec![kv("USERPROFILE", "u")], Path::new(r"C:\x\c.exe"), &[]);
        assert_eq!(get(&env, "HOME"), None);
    }

    #[test]
    fn extra_variables_are_added_after_the_scrub_and_override_equal_names() {
        let base = vec![kv("A", "old"), kv("CLAUDECODE", "1")];
        let extra = vec![kv("A", "new"), kv("CLAUDECODE", "back"), kv("B", "b")];
        let env = child_env(base, Path::new(r"C:\x\c.exe"), &extra);
        assert_eq!(get(&env, "A"), Some("new"));
        assert_eq!(get(&env, "B"), Some("b"));
        assert_eq!(get(&env, "CLAUDECODE"), Some("back"));
    }

    #[test]
    fn a_log_over_the_limit_is_started_over() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("worker-stderr.log");
        std::fs::write(&log, vec![b'x'; 1_000_001]).unwrap();
        assert!(restart_if_large(&log, STDERR_LOG_LIMIT));
        assert!(!log.exists());
    }

    #[test]
    fn a_log_at_the_limit_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("worker-stderr.log");
        std::fs::write(&log, vec![b'x'; 1_000_000]).unwrap();
        assert!(!restart_if_large(&log, STDERR_LOG_LIMIT));
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 1_000_000);
        assert!(!restart_if_large(
            &dir.path().join("missing.log"),
            STDERR_LOG_LIMIT
        ));
    }

    #[test]
    fn opening_appends_a_header_and_keeps_the_old_text() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("codex-stderr.log");
        std::fs::write(&log, "old line\n").unwrap();
        drop(open_stderr_log(&log, STDERR_LOG_LIMIT, "codex app-server").unwrap());
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.starts_with("old line\n"), "{text}");
        assert!(text.contains("--- codex app-server spawned"), "{text}");
    }

    #[test]
    fn opening_a_large_log_leaves_only_the_new_header() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("codex-stderr.log");
        std::fs::write(&log, vec![b'x'; 1_000_001]).unwrap();
        drop(open_stderr_log(&log, STDERR_LOG_LIMIT, "worker").unwrap());
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.len() < 200 && !text.contains("xxx"), "{}", text.len());
    }
}
