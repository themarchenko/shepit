use shepit_agent::{claude_work_dir_in, codex_work_dir_in, Timings};
use std::path::Path;
use std::time::Duration;

#[test]
fn the_work_dirs_sit_next_to_the_models_folder() {
    let base = Path::new("base");
    assert_eq!(
        claude_work_dir_in(base),
        base.join("Shepit").join("claude-work")
    );
    assert_eq!(
        codex_work_dir_in(base),
        base.join("Shepit").join("codex-work")
    );
}

#[test]
fn the_default_timings_are_the_macos_values() {
    let t = Timings::default();
    assert_eq!(t.request, Duration::from_secs(25));
    assert_eq!(t.warm_up, Duration::from_secs(40));
    assert_eq!((t.standby_after_turns, t.max_turns), (20, 25));
    assert_eq!(t.pause, Duration::from_secs(300));
}
