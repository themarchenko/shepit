//! Finding the Claude Code and Codex executables on Windows (docs/spec/polishing.md, Windows
//! section). Pure: the environment and the file check are injected, so this runs on any host.
//! Paths are Windows strings with backslashes; no `std::path` here.

use crate::settings::Polishing;

/// The variables discovery reads. `None` means the variable is not set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `%USERPROFILE%`
    pub user_profile: Option<String>,
    /// `%APPDATA%`
    pub app_data: Option<String>,
    /// `%LOCALAPPDATA%`
    pub local_app_data: Option<String>,
    /// `%PATH%`, entries separated by `;`.
    pub path: Option<String>,
}

/// Every place to look for `<name>`, in search order:
/// 1. `%USERPROFILE%\.local\bin\<name>.exe`
/// 2. each absolute `PATH` entry: `<name>.exe`, then `<name>.cmd`
/// 3. `%APPDATA%\npm\<name>.cmd`
/// 4. `%LOCALAPPDATA%\Microsoft\WinGet\Links\<name>.exe`
/// 5. `%USERPROFILE%\.bun\bin\<name>.exe`
/// 6. `%USERPROFILE%\scoop\shims\<name>.exe`
///
/// A place whose variable is unset is left out. Relative and empty `PATH` entries are skipped.
/// Only `.exe` and `.cmd` appear: never an extensionless file, `.ps1` or `.bat`.
pub fn candidates(name: &str, env: &Env) -> Vec<String> {
    let _ = (name, env);
    todo!("task 9 chunk A: candidates")
}

/// The executable for `name`. A non-empty `configured` path wins when it ends in `.exe` or
/// `.cmd` (any case) and `exists` says it is a file; otherwise the agent counts as not found and
/// the search does not run. An empty `configured` searches `candidates` and returns the first
/// that exists.
pub fn find_agent(
    name: &str,
    configured: &str,
    env: &Env,
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let _ = (name, configured, env, exists);
    todo!("task 9 chunk A: find_agent")
}

/// SharedControls.swift:81-89. When the selected agent is gone the choice becomes Claude if it
/// was found, else Codex if it was found, else Off. `Off`, or a selected agent that was found,
/// stays as it is.
pub fn fallback_provider(selected: Polishing, claude_found: bool, codex_found: bool) -> Polishing {
    let _ = (selected, claude_found, codex_found);
    todo!("task 9 chunk A: fallback_provider")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env {
            user_profile: Some(r"C:\Users\me".into()),
            app_data: Some(r"C:\Users\me\AppData\Roaming".into()),
            local_app_data: Some(r"C:\Users\me\AppData\Local".into()),
            path: Some(r"C:\Tools;D:\bin\".into()),
        }
    }

    fn exists_in(files: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |p| files.contains(&p)
    }

    #[test]
    fn candidates_follow_the_search_order() {
        assert_eq!(
            candidates("claude", &env()),
            [
                r"C:\Users\me\.local\bin\claude.exe",
                r"C:\Tools\claude.exe",
                r"C:\Tools\claude.cmd",
                r"D:\bin\claude.exe",
                r"D:\bin\claude.cmd",
                r"C:\Users\me\AppData\Roaming\npm\claude.cmd",
                r"C:\Users\me\AppData\Local\Microsoft\WinGet\Links\claude.exe",
                r"C:\Users\me\.bun\bin\claude.exe",
                r"C:\Users\me\scoop\shims\claude.exe",
            ]
        );
    }

    #[test]
    fn the_agent_name_is_used_as_given() {
        let c = candidates("codex", &env());
        assert_eq!(
            c.first().map(String::as_str),
            Some(r"C:\Users\me\.local\bin\codex.exe")
        );
        assert!(c.iter().all(|p| p.contains("codex")));
    }

    #[test]
    fn exe_comes_before_cmd_inside_one_path_entry() {
        let c = candidates("claude", &env());
        let exe = c.iter().position(|p| p == r"C:\Tools\claude.exe").unwrap();
        let cmd = c.iter().position(|p| p == r"C:\Tools\claude.cmd").unwrap();
        assert_eq!(cmd, exe + 1);
    }

    #[test]
    fn a_trailing_backslash_on_a_path_entry_is_not_doubled() {
        let c = candidates("claude", &env());
        assert!(c.contains(&r"D:\bin\claude.exe".to_string()));
        assert!(c.iter().all(|p| !p.contains(r"\\")), "{c:?}");
    }

    #[test]
    fn relative_and_empty_path_entries_are_skipped() {
        let e = Env {
            path: Some(r";bin;.\tools;..\x;C:rel;/usr/bin;C:\ok;;".into()),
            ..Env::default()
        };
        assert_eq!(
            candidates("claude", &e),
            [r"C:\ok\claude.exe", r"C:\ok\claude.cmd"]
        );
    }

    #[test]
    fn drive_and_unc_paths_count_as_absolute() {
        let e = Env {
            path: Some(r"c:\lower;\\server\share\bin".into()),
            ..Env::default()
        };
        let c = candidates("claude", &e);
        assert!(c.contains(&r"c:\lower\claude.exe".to_string()), "{c:?}");
        assert!(
            c.contains(&r"\\server\share\bin\claude.exe".to_string()),
            "{c:?}"
        );
    }

    #[test]
    fn unset_variables_drop_their_places() {
        assert!(candidates("claude", &Env::default()).is_empty());
        let only_profile = Env {
            user_profile: Some(r"C:\U".into()),
            ..Env::default()
        };
        assert_eq!(
            candidates("claude", &only_profile),
            [
                r"C:\U\.local\bin\claude.exe",
                r"C:\U\.bun\bin\claude.exe",
                r"C:\U\scoop\shims\claude.exe",
            ]
        );
    }

    #[test]
    fn only_exe_and_cmd_are_ever_candidates() {
        for p in candidates("claude", &env()) {
            let lower = p.to_lowercase();
            assert!(lower.ends_with(".exe") || lower.ends_with(".cmd"), "{p}");
        }
    }

    #[test]
    fn the_first_existing_candidate_wins() {
        let e = env();
        let found = find_agent(
            "claude",
            "",
            &e,
            &exists_in(&[
                r"D:\bin\claude.cmd",
                r"C:\Users\me\AppData\Roaming\npm\claude.cmd",
                r"C:\Users\me\.bun\bin\claude.exe",
            ]),
        );
        assert_eq!(found.as_deref(), Some(r"D:\bin\claude.cmd"));
    }

    #[test]
    fn exe_beats_cmd_beside_it() {
        let found = find_agent(
            "claude",
            "",
            &env(),
            &exists_in(&[r"C:\Tools\claude.cmd", r"C:\Tools\claude.exe"]),
        );
        assert_eq!(found.as_deref(), Some(r"C:\Tools\claude.exe"));
    }

    #[test]
    fn the_native_installer_folder_beats_path() {
        let found = find_agent(
            "claude",
            "",
            &env(),
            &exists_in(&[r"C:\Tools\claude.exe", r"C:\Users\me\.local\bin\claude.exe"]),
        );
        assert_eq!(found.as_deref(), Some(r"C:\Users\me\.local\bin\claude.exe"));
    }

    #[test]
    fn an_extensionless_file_and_a_ps1_are_never_picked() {
        let exists =
            |p: &str| !(p.to_lowercase().ends_with(".exe") || p.to_lowercase().ends_with(".cmd"));
        assert_eq!(find_agent("claude", "", &env(), &exists), None);
    }

    #[test]
    fn nothing_found_is_none() {
        assert_eq!(find_agent("claude", "", &env(), &|_| false), None);
    }

    #[test]
    fn a_configured_path_wins_when_it_is_an_existing_exe_or_cmd() {
        let e = env();
        for path in [
            r"E:\mine\claude.exe",
            r"E:\mine\claude.EXE",
            r"E:\mine\run.cmd",
            r"E:\mine\run.Cmd",
        ] {
            let found = find_agent("claude", path, &e, &|p| {
                p == path || p == r"C:\Tools\claude.exe"
            });
            assert_eq!(found.as_deref(), Some(path), "{path}");
        }
    }

    #[test]
    fn a_configured_path_that_does_not_exist_means_not_found() {
        let found = find_agent(
            "claude",
            r"E:\gone\claude.exe",
            &env(),
            &exists_in(&[r"C:\Tools\claude.exe"]),
        );
        assert_eq!(found, None, "the search must not run");
    }

    #[test]
    fn a_configured_path_with_another_extension_means_not_found() {
        for path in [
            r"E:\m\claude",
            r"E:\m\claude.ps1",
            r"E:\m\claude.bat",
            r"E:\m\claude.exe.txt",
        ] {
            let found = find_agent("claude", path, &env(), &|_| true);
            assert_eq!(found, None, "{path}");
        }
    }

    #[test]
    fn an_empty_configured_path_searches() {
        let found = find_agent("claude", "", &env(), &exists_in(&[r"C:\Tools\claude.exe"]));
        assert_eq!(found.as_deref(), Some(r"C:\Tools\claude.exe"));
    }

    // The fallback table of SharedControls.swift:81-89.

    #[test]
    fn a_selected_agent_that_was_found_stays() {
        for (sel, claude, codex) in [
            (Polishing::Claude, true, false),
            (Polishing::Claude, true, true),
            (Polishing::Codex, false, true),
            (Polishing::Codex, true, true),
        ] {
            assert_eq!(
                fallback_provider(sel, claude, codex),
                sel,
                "{sel:?} {claude} {codex}"
            );
        }
    }

    #[test]
    fn a_missing_codex_falls_back_to_claude_then_off() {
        assert_eq!(
            fallback_provider(Polishing::Codex, true, false),
            Polishing::Claude
        );
        assert_eq!(
            fallback_provider(Polishing::Codex, false, false),
            Polishing::Off
        );
    }

    #[test]
    fn a_missing_claude_falls_back_to_codex_then_off() {
        assert_eq!(
            fallback_provider(Polishing::Claude, false, true),
            Polishing::Codex
        );
        assert_eq!(
            fallback_provider(Polishing::Claude, false, false),
            Polishing::Off
        );
    }

    #[test]
    fn off_stays_off_whatever_was_found() {
        for (claude, codex) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(
                fallback_provider(Polishing::Off, claude, codex),
                Polishing::Off
            );
        }
    }
}
