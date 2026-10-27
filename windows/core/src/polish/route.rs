//! Which path a dictation takes. Mirrors `FormattingService.format` (FormattingService.swift:21-44)
//! plus the manager decision of 2026-10-27: a secure field never reaches the agent.

use super::prompt::FormatRequest;

/// Where the source id of a result comes from. The ids are the Swift raw values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Claude,
    Codex,
    Passthrough,
    FastPath,
    Fallback,
}

impl Source {
    /// `FormatterSource.rawValue`: `claude`, `codex`, `passthrough`, `fastPath`, `fallback`.
    pub fn as_str(&self) -> &'static str {
        todo!("task 9 chunk A: source ids")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Raw mode: whitespace cleanup only, no dictionary.
    Raw,
    /// Short clean dictation: local text, source `fastPath`.
    FastPath,
    /// Local text, source `passthrough`: no agent, or a secure field.
    Local,
    /// Ask the agent; an error falls back to local text.
    Agent,
}

/// `fast_path_max_words` below 0 counts as 0. Order: raw, fast path, no agent or secure field,
/// agent. A secure field in raw mode keeps the raw result, and in fast-path size keeps the fast
/// path: both already stay local.
pub fn route(request: &FormatRequest, fast_path_max_words: i64, agent_present: bool) -> Route {
    let _ = (request, fast_path_max_words, agent_present);
    todo!("task 9 chunk A: route")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polish::prompt::PromptContext;
    use crate::profile::{profile_by_id, Mode};

    fn req(mode: Mode, profile: &str, words: usize, secure: bool) -> FormatRequest {
        FormatRequest {
            transcript: vec!["word"; words].join(" "),
            language_hint: None,
            mode,
            profile: profile_by_id(profile).unwrap(),
            context: PromptContext::default(),
            dictionary: Vec::new(),
            examples: Vec::new(),
            secure,
        }
    }

    #[test]
    fn source_ids_are_the_swift_raw_values() {
        assert_eq!(Source::Claude.as_str(), "claude");
        assert_eq!(Source::Codex.as_str(), "codex");
        assert_eq!(Source::Passthrough.as_str(), "passthrough");
        assert_eq!(Source::FastPath.as_str(), "fastPath");
        assert_eq!(Source::Fallback.as_str(), "fallback");
    }

    #[test]
    fn raw_mode_is_raw_with_or_without_an_agent() {
        assert_eq!(
            route(&req(Mode::Raw, "generic", 30, false), 8, true),
            Route::Raw
        );
        assert_eq!(
            route(&req(Mode::Raw, "generic", 30, false), 8, false),
            Route::Raw
        );
    }

    #[test]
    fn a_clean_dictation_at_the_limit_takes_the_fast_path() {
        assert_eq!(
            route(&req(Mode::Clean, "generic", 8, false), 8, true),
            Route::FastPath
        );
        assert_eq!(
            route(&req(Mode::Clean, "generic", 9, false), 8, true),
            Route::Agent
        );
    }

    #[test]
    fn ai_targets_and_prompt_mode_never_take_the_fast_path() {
        assert_eq!(
            route(&req(Mode::Clean, "ai-chat", 3, false), 8, true),
            Route::Agent
        );
        assert_eq!(
            route(&req(Mode::Prompt, "generic", 2, false), 8, true),
            Route::Agent
        );
    }

    #[test]
    fn a_negative_limit_counts_as_zero() {
        assert_eq!(
            route(&req(Mode::Clean, "generic", 1, false), -3, true),
            Route::Agent
        );
        assert_eq!(
            route(&req(Mode::Clean, "generic", 0, false), -3, true),
            Route::FastPath
        );
    }

    #[test]
    fn no_agent_means_local_text() {
        assert_eq!(
            route(&req(Mode::Clean, "generic", 20, false), 8, false),
            Route::Local
        );
    }

    #[test]
    fn a_secure_field_never_reaches_the_agent() {
        assert_eq!(
            route(&req(Mode::Clean, "generic", 20, true), 8, true),
            Route::Local
        );
        assert_eq!(
            route(&req(Mode::Prompt, "ai-chat", 20, true), 8, true),
            Route::Local
        );
    }

    #[test]
    fn a_secure_field_keeps_the_raw_and_fast_paths() {
        assert_eq!(
            route(&req(Mode::Raw, "generic", 20, true), 8, true),
            Route::Raw
        );
        assert_eq!(
            route(&req(Mode::Clean, "generic", 2, true), 8, true),
            Route::FastPath
        );
    }
}
