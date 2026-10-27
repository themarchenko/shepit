//! One dictation from accepted transcript to final text: profile, mode, route, agent, cleanup,
//! fallback. The agent sits behind `Polisher`, so this runs against a fake in tests.

use super::prompt::FormatRequest;
use super::route::Source;
use crate::dictation::FormatJob;
use crate::settings::Settings;

/// Why an agent call failed. The pipeline treats every variant the same way: local text and
/// source `fallback`. `BadReply` carries a byte length, never the reply, so a log line cannot leak
/// dictated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolishError {
    NotRunning,
    Timeout,
    Cancelled,
    Exited(i32),
    Api(String),
    BadReply(usize),
    Paused,
}

/// The agent behind the pipeline: Claude Code or Codex.
pub trait Polisher {
    /// `Source::Claude` or `Source::Codex`: what a successful result reports.
    fn source(&self) -> Source;
    /// Sends one user message and returns the agent's raw reply text. `cancelled` turns true when
    /// Esc dropped the dictation; the call then returns `PolishError::Cancelled`.
    fn polish(
        &mut self,
        message: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, PolishError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    pub source: Source,
    pub language: Option<String>,
    /// Parsed but not learned yet (task 10).
    pub new_terms: Vec<String>,
    /// The error text of a fallback. Never the transcript or the reply.
    pub note: Option<String>,
}

/// The request for a job: the profile from the exe name and window title, the mode from the
/// settings and the word count, the context keys, the dictionary. `language_hint` stays empty
/// (Windows has no language lock). `secure` is true only when the captured password flag is true.
pub fn build_request(job: &FormatJob, settings: &Settings) -> FormatRequest {
    let _ = (job, settings);
    todo!("task 9 chunk A: build_request")
}

/// Routes one request. `polisher` is `None` when no agent is available. A cancelled job does not
/// call the polisher.
pub fn format_request(
    request: &FormatRequest,
    fast_path_max_words: i64,
    polisher: Option<&mut dyn Polisher>,
    cancelled: &dyn Fn() -> bool,
) -> Formatted {
    let _ = (request, fast_path_max_words, polisher, cancelled);
    todo!("task 9 chunk A: format_request")
}

/// `build_request` then `format_request` with the settings' word limit.
pub fn format_dictation(
    job: &FormatJob,
    settings: &Settings,
    polisher: Option<&mut dyn Polisher>,
    cancelled: &dyn Fn() -> bool,
) -> Formatted {
    let _ = (job, settings, polisher, cancelled);
    todo!("task 9 chunk A: format_dictation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TargetContext;
    use crate::polish::prompt::PromptContext;
    use crate::profile::{profile_by_id, Mode};
    use std::cell::Cell;

    struct Fake {
        source: Source,
        reply: Result<String, PolishError>,
        messages: Vec<String>,
    }

    impl Fake {
        fn replying(text: &str) -> Fake {
            let reply = format!(
                r#"{{"text":{},"language":"en","new_terms":["Vercel"]}}"#,
                json_str(text)
            );
            Fake {
                source: Source::Claude,
                reply: Ok(reply),
                messages: Vec::new(),
            }
        }
        fn failing(error: PolishError) -> Fake {
            Fake {
                source: Source::Claude,
                reply: Err(error),
                messages: Vec::new(),
            }
        }
    }

    impl Polisher for Fake {
        fn source(&self) -> Source {
            self.source
        }
        fn polish(&mut self, message: &str, _: &dyn Fn() -> bool) -> Result<String, PolishError> {
            self.messages.push(message.to_string());
            self.reply.clone()
        }
    }

    fn json_str(s: &str) -> String {
        serde_json::to_string(s).unwrap()
    }

    fn never() -> bool {
        false
    }

    fn request(transcript: &str, mode: Mode, profile: &str) -> FormatRequest {
        FormatRequest {
            transcript: transcript.into(),
            language_hint: None,
            mode,
            profile: profile_by_id(profile).unwrap(),
            context: PromptContext::default(),
            dictionary: vec!["Vercel".into()],
            examples: Vec::new(),
            secure: false,
        }
    }

    const NINE: &str = "one two three four five six seven eight nine";

    #[test]
    fn raw_mode_cleans_the_whitespace_and_skips_the_agent() {
        let mut fake = Fake::replying("agent");
        let r = format_request(
            &request("задеплой  на версел", Mode::Raw, "generic"),
            8,
            Some(&mut fake),
            &never,
        );
        assert_eq!(
            (r.text.as_str(), r.source),
            ("Задеплой на версел", Source::Passthrough)
        );
        assert!(fake.messages.is_empty());
    }

    #[test]
    fn a_short_clean_dictation_takes_the_fast_path_with_the_dictionary() {
        let mut fake = Fake::replying("agent");
        let r = format_request(
            &request("задеплой на версел", Mode::Clean, "generic"),
            8,
            Some(&mut fake),
            &never,
        );
        assert_eq!(
            (r.text.as_str(), r.source),
            ("Задеплой на Vercel", Source::FastPath)
        );
        assert!(fake.messages.is_empty());
    }

    #[test]
    fn a_long_dictation_goes_to_the_agent_and_reports_its_source() {
        let mut fake = Fake::replying("Agent text");
        let r = format_request(
            &request(NINE, Mode::Clean, "generic"),
            8,
            Some(&mut fake),
            &never,
        );
        assert_eq!((r.text.as_str(), r.source), ("Agent text", Source::Claude));
        assert_eq!(r.language.as_deref(), Some("en"));
        assert_eq!(r.new_terms, ["Vercel"]);
        assert_eq!(r.note, None);
        assert_eq!(fake.messages.len(), 1);
        assert!(fake.messages[0].ends_with(&format!("Transcript:\n{NINE}")));
    }

    #[test]
    fn a_codex_polisher_reports_codex() {
        let mut fake = Fake::replying("Agent text");
        fake.source = Source::Codex;
        let r = format_request(
            &request(NINE, Mode::Clean, "generic"),
            8,
            Some(&mut fake),
            &never,
        );
        assert_eq!(r.source, Source::Codex);
    }

    #[test]
    fn no_polisher_gives_local_text_as_passthrough() {
        let r = format_request(
            &request(
                &format!("задеплой на версел {NINE}"),
                Mode::Clean,
                "generic",
            ),
            8,
            None,
            &never,
        );
        assert_eq!(r.source, Source::Passthrough);
        assert_eq!(r.text, format!("Задеплой на Vercel {NINE}"));
    }

    #[test]
    fn an_agent_error_falls_back_to_local_text_with_a_note() {
        for error in [
            PolishError::Timeout,
            PolishError::Exited(1),
            PolishError::Api("usage limit".into()),
            PolishError::Paused,
            PolishError::NotRunning,
        ] {
            let mut fake = Fake::failing(error.clone());
            let r = format_request(
                &request(
                    &format!("задеплой на версел {NINE}"),
                    Mode::Clean,
                    "generic",
                ),
                8,
                Some(&mut fake),
                &never,
            );
            assert_eq!(r.source, Source::Fallback, "{error:?}");
            assert_eq!(r.text, format!("Задеплой на Vercel {NINE}"), "{error:?}");
            assert!(r.note.is_some(), "{error:?}");
        }
    }

    #[test]
    fn the_note_never_holds_the_transcript() {
        let mut fake = Fake::failing(PolishError::BadReply(321));
        let r = format_request(
            &request(
                "secret words here and more of them for the agent",
                Mode::Clean,
                "generic",
            ),
            2,
            Some(&mut fake),
            &never,
        );
        let note = r.note.unwrap();
        assert!(!note.contains("secret"), "{note}");
    }

    #[test]
    fn a_reply_that_is_not_json_falls_back() {
        let mut fake = Fake {
            source: Source::Claude,
            reply: Ok("sorry, I cannot".into()),
            messages: Vec::new(),
        };
        let r = format_request(
            &request(NINE, Mode::Clean, "generic"),
            8,
            Some(&mut fake),
            &never,
        );
        assert_eq!(r.source, Source::Fallback);
        assert_eq!(r.text, "One two three four five six seven eight nine");
        let note = r.note.unwrap();
        assert!(!note.contains("sorry"), "{note}");
    }

    #[test]
    fn the_agent_text_gets_the_finish_cleanup() {
        let mut fake = Fake::replying("\nзроби це\n\n");
        let r = format_request(
            &request(NINE, Mode::Prompt, "claude-code-terminal"),
            8,
            Some(&mut fake),
            &never,
        );
        assert_eq!(r.text, "Зроби це");
    }

    #[test]
    fn a_secure_field_skips_the_agent_and_uses_local_text() {
        let mut fake = Fake::replying("agent");
        let mut req = request(
            &format!("задеплой на версел {NINE}"),
            Mode::Clean,
            "generic",
        );
        req.secure = true;
        let r = format_request(&req, 8, Some(&mut fake), &never);
        assert_eq!(r.source, Source::Passthrough);
        assert_eq!(r.text, format!("Задеплой на Vercel {NINE}"));
        assert!(fake.messages.is_empty(), "the polisher must not be called");
    }

    #[test]
    fn a_cancelled_job_does_not_call_the_polisher() {
        let mut fake = Fake::replying("agent");
        let calls = Cell::new(0);
        let cancelled = || {
            calls.set(calls.get() + 1);
            true
        };
        let r = format_request(
            &request(NINE, Mode::Clean, "generic"),
            8,
            Some(&mut fake),
            &cancelled,
        );
        assert!(fake.messages.is_empty());
        assert_ne!(r.source, Source::Claude);
        assert!(calls.get() >= 1);
    }

    // Building the request from a job.

    fn job(transcript: &str, context: Option<TargetContext>) -> FormatJob {
        FormatJob {
            id: 7,
            transcript: transcript.into(),
            context,
        }
    }

    fn words(n: usize) -> String {
        vec!["word"; n].join(" ")
    }

    #[test]
    fn the_exe_name_and_title_pick_the_profile() {
        let ctx = TargetContext {
            app_id: Some("WindowsTerminal.exe".into()),
            window_title: Some("claude - project".into()),
            ..Default::default()
        };
        let req = build_request(&job("hello", Some(ctx)), &Settings::default());
        assert_eq!(req.profile.id, "claude-code-terminal");
        let req = build_request(
            &job(
                "hello",
                Some(TargetContext {
                    app_id: Some("notepad.exe".into()),
                    ..Default::default()
                }),
            ),
            &Settings::default(),
        );
        assert_eq!(req.profile.id, "notes");
    }

    #[test]
    fn no_context_means_the_generic_profile() {
        let req = build_request(&job("hello", None), &Settings::default());
        assert_eq!(req.profile.id, "generic");
        assert!(!req.secure);
    }

    #[test]
    fn a_long_dictation_into_an_ai_target_becomes_prompt_mode() {
        let ctx = TargetContext {
            app_id: Some("claude.exe".into()),
            ..Default::default()
        };
        let long = build_request(&job(&words(40), Some(ctx.clone())), &Settings::default());
        assert_eq!(long.mode, Mode::Prompt);
        let short = build_request(&job(&words(39), Some(ctx)), &Settings::default());
        assert_eq!(short.mode, Mode::Clean);
    }

    #[test]
    fn the_default_mode_and_the_word_threshold_come_from_the_settings() {
        let mut s = Settings::default();
        s.default_mode = Mode::Raw;
        assert_eq!(build_request(&job("hello", None), &s).mode, Mode::Raw);
        let mut s = Settings::default();
        s.prompt_mode_min_words = 10;
        let ctx = TargetContext {
            app_id: Some("claude.exe".into()),
            ..Default::default()
        };
        assert_eq!(
            build_request(&job(&words(10), Some(ctx)), &s).mode,
            Mode::Prompt
        );
    }

    #[test]
    fn the_message_carries_the_exe_profile_and_mode() {
        let ctx = TargetContext {
            app_id: Some("WindowsTerminal.exe".into()),
            app_name: Some("Windows Terminal".into()),
            window_title: Some("claude".into()),
            text_before_caret: Some("before".into()),
            selected_text: Some("picked".into()),
            ..Default::default()
        };
        let mut fake = Fake::replying("Done");
        let r = format_dictation(
            &job(&words(45), Some(ctx)),
            &Settings::default(),
            Some(&mut fake),
            &never,
        );
        assert_eq!(r.source, Source::Claude);
        let m = &fake.messages[0];
        assert!(m.contains(r#""profile":"claude-code-terminal""#), "{m}");
        assert!(m.contains(r#""mode":"prompt""#), "{m}");
        assert!(m.contains(r#""bundle_id":"WindowsTerminal.exe""#), "{m}");
        assert!(m.contains(r#""app":"Windows Terminal""#), "{m}");
        assert!(m.contains(r#""language_hint":"auto""#), "{m}");
        assert!(m.contains(r#""text_before_caret":"before""#), "{m}");
        assert!(m.contains(r#""selected_text":"picked""#), "{m}");
        assert!(!m.contains("\"url\""), "{m}");
    }

    #[test]
    fn the_settings_dictionary_goes_into_the_request_and_the_message() {
        let mut s = Settings::default();
        s.dictionary = vec!["Shepit".into(), "Vercel".into()];
        let mut fake = Fake::replying("Done");
        format_dictation(&job(&words(12), None), &s, Some(&mut fake), &never);
        assert!(fake.messages[0].contains("Dictionary (preferred spellings): Shepit, Vercel"));
    }

    #[test]
    fn only_a_true_password_flag_makes_the_request_secure() {
        for (flag, secure) in [(Some(true), true), (Some(false), false), (None, false)] {
            let ctx = TargetContext {
                password: flag,
                ..Default::default()
            };
            assert_eq!(
                build_request(&job("hello", Some(ctx)), &Settings::default()).secure,
                secure,
                "{flag:?}"
            );
        }
    }

    #[test]
    fn a_dictation_into_a_password_field_never_calls_the_polisher() {
        let ctx = TargetContext {
            password: Some(true),
            ..Default::default()
        };
        let mut fake = Fake::replying("agent");
        let r = format_dictation(
            &job(&words(30), Some(ctx)),
            &Settings::default(),
            Some(&mut fake),
            &never,
        );
        assert_eq!(r.source, Source::Passthrough);
        assert!(fake.messages.is_empty());
    }

    #[test]
    fn the_fast_path_limit_comes_from_the_settings() {
        let mut s = Settings::default();
        s.fast_path_max_words = 3;
        let mut fake = Fake::replying("Agent");
        let r = format_dictation(&job(&words(4), None), &s, Some(&mut fake), &never);
        assert_eq!(r.source, Source::Claude);
        let r = format_dictation(&job(&words(3), None), &s, Some(&mut fake), &never);
        assert_eq!(r.source, Source::FastPath);
    }
}
