//! Live preview: the words heard so far, shown while the key is held (docs/spec/widget.md, task
//! 12). Port of LivePreview.swift with a scheduler that does not own a task: the controller asks
//! `next` for a pass, the recognizer thread runs it, `on_result` brings the answer back. The
//! window is smaller than on macOS because Parakeet runs on the CPU here (plan-windows-live.md,
//! decisions 1-4).

use crate::speech::join;

/// The text on screen: confirmed words, then a dimmer tail that may still change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveText {
    pub stable: String,
    pub unstable: String,
}

impl LiveText {
    pub fn is_empty(&self) -> bool {
        self.stable.is_empty() && self.unstable.is_empty()
    }

    pub fn full(&self) -> String {
        join(&self.stable, &self.unstable)
    }
}

/// LivePreview.swift:119-125 `publish`: the last two words of the pass are unstable, the rest
/// joins the frozen text.
pub fn split(frozen: &str, current: &str) -> LiveText {
    let words: Vec<&str> = current.split_whitespace().collect();
    let unstable = words.len().min(2);
    let (head, tail) = words.split_at(words.len() - unstable);
    LiveText {
        stable: join(frozen, &head.join(" ")),
        unstable: tail.join(" "),
    }
}

const FRAME: usize = 1_600;

/// Middle of the quietest 100 ms frame between `lower` and `upper` (LivePreview.swift
/// `quietestCut`). Frames step by 50 ms and the earliest wins a tie. With no frame that fits it
/// returns the end of the searched range.
pub fn quietest_cut(samples: &[f32], lower: usize, upper: usize) -> usize {
    let end = upper.min(samples.len());
    let mut best = end;
    let mut best_energy = f32::INFINITY;
    let mut start = lower;
    while start + FRAME <= end {
        let energy: f32 = samples[start..start + FRAME].iter().map(|s| s * s).sum();
        if energy < best_energy {
            best_energy = energy;
            best = start + FRAME / 2;
        }
        start += FRAME / 2;
    }
    best
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Target gap between the starts of two passes.
    pub interval_secs: f64,
    /// The shortest rest after a pass.
    pub min_rest_secs: f64,
    /// No pass before this much audio (1 s at 16 kHz).
    pub min_samples: usize,
    /// A tail longer than this is frozen, which bounds every pass.
    pub freeze_after: usize,
    /// The freeze cut is searched in `cut_from..=cut_to` of the tail.
    pub cut_from: usize,
    pub cut_to: usize,
    /// A pass slower than this stops the preview for the rest of the dictation.
    pub slow_pass_secs: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_secs: 0.5,
            min_rest_secs: 0.15,
            min_samples: 16_000,
            freeze_after: 96_000,
            cut_from: 64_000,
            cut_to: 88_000,
            slow_pass_secs: 1.0,
        }
    }
}

/// The growing recording, read while it is still being captured.
pub trait LiveSource {
    /// Samples at 16 kHz so far.
    fn captured(&self) -> usize;
    /// A copy of the recording from `start` on; empty past the end.
    fn samples_from(&self, start: usize) -> Vec<f32>;
}

/// Audio for one pass. `seq` ties the answer to the request.
#[derive(Debug, Clone, PartialEq)]
pub struct PassRequest {
    pub seq: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pass {
    /// Decodes the audio from the frozen point up to `cut` and moves the frozen point there.
    Freeze {
        cut: usize,
    },
    Tail,
}

#[derive(Debug)]
pub struct LivePreview {
    config: Config,
    running: bool,
    text: LiveText,
    frozen_text: String,
    frozen_samples: usize,
    in_flight: Option<(u64, Pass)>,
    next_seq: u64,
    not_before: f64,
}

impl LivePreview {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            running: false,
            text: LiveText::default(),
            frozen_text: String::new(),
            frozen_samples: 0,
            in_flight: None,
            next_seq: 1,
            not_before: 0.0,
        }
    }

    /// A new recording begins. `next_seq` keeps counting so an answer from before is never
    /// taken for a new one.
    pub fn start(&mut self, now: f64) {
        self.running = true;
        self.text = LiveText::default();
        self.frozen_text.clear();
        self.frozen_samples = 0;
        self.in_flight = None;
        self.not_before = now;
    }

    /// Ends the passes and leaves the last text on screen. A late answer is dropped.
    pub fn stop(&mut self) {
        self.running = false;
        self.in_flight = None;
    }

    pub fn clear(&mut self) {
        self.stop();
        self.text = LiveText::default();
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn text(&self) -> &LiveText {
        &self.text
    }

    /// The next pass to run, if one is due. At most one is in flight, and the source is not
    /// touched while one runs or the rest is not over.
    pub fn next(&mut self, now: f64, source: &dyn LiveSource) -> Option<PassRequest> {
        if !self.running || self.in_flight.is_some() || now < self.not_before {
            return None;
        }
        // The minimum applies to the first pass only: the tail after a freeze may be shorter.
        if self.frozen_samples == 0 && source.captured() < self.config.min_samples {
            return None;
        }
        let tail = source.samples_from(self.frozen_samples);
        if tail.is_empty() {
            return None;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        if tail.len() > self.config.freeze_after {
            let cut = quietest_cut(&tail, self.config.cut_from, self.config.cut_to);
            self.in_flight = Some((seq, Pass::Freeze { cut }));
            return Some(PassRequest {
                seq,
                samples: tail[..cut].to_vec(),
            });
        }
        self.in_flight = Some((seq, Pass::Tail));
        Some(PassRequest { seq, samples: tail })
    }

    /// The answer to `seq`, which took `secs` and arrived at `now`. Anything but the pass in
    /// flight is ignored.
    pub fn on_result(&mut self, seq: u64, result: Result<String, String>, secs: f64, now: f64) {
        if !self.running {
            return;
        }
        let Some((flying, pass)) = self.in_flight else {
            return;
        };
        if flying != seq {
            return;
        }
        self.in_flight = None;
        match pass {
            Pass::Freeze { cut } => {
                // The point advances on an error too, or the same window would be retried.
                if let Ok(text) = result {
                    self.frozen_text = join(&self.frozen_text, text.trim());
                }
                self.frozen_samples += cut;
                // The tail pass follows at once; the text waits for it.
                self.not_before = now;
            }
            Pass::Tail => {
                if let Ok(text) = result {
                    let next = split(&self.frozen_text, &text);
                    if next != self.text {
                        self.text = next;
                    }
                }
                let rest = (self.config.interval_secs - secs)
                    .max(self.config.min_rest_secs)
                    .max(secs);
                self.not_before = now + rest;
            }
        }
        if secs > self.config.slow_pass_secs {
            self.running = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A recording that grows by moving `limit`; counts every read so a test can prove the
    /// scheduler stayed away from the buffer.
    struct Src {
        audio: Vec<f32>,
        limit: Cell<usize>,
        reads: Cell<u32>,
    }

    impl Src {
        fn new(audio: Vec<f32>) -> Self {
            let n = audio.len();
            Self {
                audio,
                limit: Cell::new(n),
                reads: Cell::new(0),
            }
        }
        /// A tone of `n` samples with a silent gap `[gap, gap + 1600)`, so the freeze cut is
        /// at `gap + 800`.
        fn with_gap(n: usize, gap: usize) -> Self {
            let mut a: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
            for s in &mut a[gap..gap + 1_600] {
                *s = 0.0;
            }
            Self::new(a)
        }
        fn tone(n: usize) -> Self {
            Self::new((0..n).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect())
        }
        fn set(&self, captured: usize) {
            self.limit.set(captured.min(self.audio.len()));
        }
    }

    impl LiveSource for Src {
        fn captured(&self) -> usize {
            self.reads.set(self.reads.get() + 1);
            self.limit.get()
        }
        fn samples_from(&self, start: usize) -> Vec<f32> {
            self.reads.set(self.reads.get() + 1);
            let end = self.limit.get();
            self.audio[start.min(end)..end].to_vec()
        }
    }

    fn started() -> LivePreview {
        let mut p = LivePreview::new(Config::default());
        p.start(0.0);
        p
    }

    fn ok(text: &str) -> Result<String, String> {
        Ok(text.to_string())
    }

    // split

    #[test]
    fn split_of_nothing_is_empty() {
        let t = split("", "");
        assert!(t.is_empty());
        assert_eq!(t, LiveText::default());
    }

    #[test]
    fn split_keeps_one_or_two_words_unstable() {
        assert_eq!(split("", "hello").unstable, "hello");
        assert_eq!(split("", "hello").stable, "");
        assert_eq!(split("", "hello there").unstable, "hello there");
        assert_eq!(split("", "hello there").stable, "");
    }

    #[test]
    fn split_leaves_the_last_two_words_unstable() {
        let t = split("", "so the widget should");
        assert_eq!(t.stable, "so the");
        assert_eq!(t.unstable, "widget should");
    }

    #[test]
    fn split_joins_the_frozen_text_to_the_stable_part() {
        let t = split("first part", "and then more words");
        assert_eq!(t.stable, "first part and then");
        assert_eq!(t.unstable, "more words");
    }

    #[test]
    fn split_with_an_empty_pass_keeps_only_the_frozen_text() {
        let t = split("first part", "");
        assert_eq!(t.stable, "first part");
        assert_eq!(t.unstable, "");
    }

    #[test]
    fn split_collapses_whitespace() {
        let t = split("", "  a \n b\t c  ");
        assert_eq!(t.stable, "a");
        assert_eq!(t.unstable, "b c");
    }

    #[test]
    fn full_joins_stable_and_unstable_with_one_space() {
        let t = LiveText {
            stable: "a b".into(),
            unstable: "c d".into(),
        };
        assert_eq!(t.full(), "a b c d");
        let only_unstable = LiveText {
            stable: String::new(),
            unstable: "c".into(),
        };
        assert_eq!(only_unstable.full(), "c");
        assert!(!only_unstable.is_empty());
    }

    // quietest_cut

    #[test]
    fn the_cut_lands_in_the_quiet_gap() {
        let mut s: Vec<f32> = (0..12 * 16_000)
            .map(|i| 0.3 * (i as f32 * 0.05).sin())
            .collect();
        for v in &mut s[9 * 16_000..9 * 16_000 + 3_200] {
            *v = 0.0;
        }
        let cut = quietest_cut(&s, 8 * 16_000, 11 * 16_000);
        assert!(
            (9 * 16_000..=9 * 16_000 + 3_200).contains(&cut),
            "cut at {cut}"
        );
    }

    #[test]
    fn equal_frames_pick_the_earliest() {
        let s = vec![0.1; 40_000];
        assert_eq!(quietest_cut(&s, 10_000, 30_000), 10_000 + 800);
    }

    #[test]
    fn a_short_buffer_returns_the_end_or_the_upper_bound() {
        // Shorter than lower + one 1600-sample frame: no frame fits.
        let s = vec![0.1; 1_599 + 100];
        assert_eq!(quietest_cut(&s, 100, 5_000), s.len());
        assert_eq!(quietest_cut(&s, 100, 1_000), 1_000);
    }

    #[test]
    fn the_search_steps_by_800_samples() {
        let lower = 10_000;
        let mut s = vec![0.2; 40_000];
        for v in &mut s[lower + 2_400..lower + 4_000] {
            *v = 0.0;
        }
        assert_eq!(quietest_cut(&s, lower, 30_000), lower + 2_400 + 800);
    }

    // Config

    #[test]
    fn the_default_config_matches_the_plan() {
        let c = Config::default();
        assert_eq!(c.interval_secs, 0.5);
        assert_eq!(c.min_rest_secs, 0.15);
        assert_eq!(c.min_samples, 16_000);
        assert_eq!(c.freeze_after, 96_000);
        assert_eq!(c.cut_from, 64_000);
        assert_eq!(c.cut_to, 88_000);
        assert_eq!(c.slow_pass_secs, 1.0);
    }

    // Scheduler

    #[test]
    fn a_new_preview_is_not_running_and_has_no_text() {
        let p = LivePreview::new(Config::default());
        assert!(!p.is_running());
        assert!(p.text().is_empty());
    }

    #[test]
    fn nothing_runs_before_start() {
        let mut p = LivePreview::new(Config::default());
        let src = Src::tone(32_000);
        assert!(p.next(5.0, &src).is_none());
    }

    #[test]
    fn no_pass_below_one_second_of_audio() {
        let mut p = started();
        let src = Src::tone(32_000);
        src.set(15_999);
        assert!(p.next(1.0, &src).is_none());
        src.set(16_000);
        let req = p.next(1.0, &src).expect("pass at exactly 16 000 samples");
        assert_eq!(req.samples.len(), 16_000);
    }

    #[test]
    fn only_one_pass_is_in_flight() {
        let mut p = started();
        let src = Src::tone(32_000);
        assert!(p.next(1.0, &src).is_some());
        let reads = src.reads.get();
        assert!(p.next(1.5, &src).is_none());
        assert!(p.next(9.0, &src).is_none());
        assert_eq!(
            src.reads.get(),
            reads,
            "the buffer is not touched while a pass runs"
        );
    }

    #[test]
    fn a_lost_result_leaves_the_pass_in_flight_and_the_text_alone() {
        let mut p = started();
        let src = Src::tone(160_000);
        let req = p.next(1.0, &src).unwrap();
        p.on_result(req.seq, ok("one two three"), 0.1, 1.1);
        let before = p.text().full();
        assert!(p.next(2.0, &src).is_some());
        // No result for that pass ever arrives (a dropped message).
        assert!(p.next(5.0, &src).is_none());
        assert!(p.next(60.0, &src).is_none());
        assert_eq!(p.text().full(), before);
    }

    #[test]
    fn a_pass_before_the_rest_is_over_does_not_touch_the_buffer() {
        let mut p = started();
        let src = Src::tone(48_000);
        let req = p.next(1.0, &src).unwrap();
        p.on_result(req.seq, ok("one two three"), 0.1, 1.1);
        let reads = src.reads.get();
        assert!(p.next(1.2, &src).is_none());
        assert_eq!(src.reads.get(), reads);
    }

    #[test]
    fn a_tail_pass_carries_the_audio_from_the_frozen_point() {
        let mut p = started();
        let src = Src::tone(48_000);
        let req = p.next(1.0, &src).unwrap();
        assert_eq!(req.samples, src.audio[..48_000].to_vec());
    }

    #[test]
    fn each_request_has_its_own_seq() {
        let mut p = started();
        let src = Src::tone(48_000);
        let a = p.next(1.0, &src).unwrap();
        p.on_result(a.seq, ok("one two three"), 0.1, 1.1);
        let b = p.next(2.0, &src).unwrap();
        assert_ne!(a.seq, b.seq);
    }

    #[test]
    fn a_tail_result_shows_as_stable_and_unstable_text() {
        let mut p = started();
        let src = Src::tone(48_000);
        let req = p.next(1.0, &src).unwrap();
        p.on_result(req.seq, ok("so the widget should"), 0.1, 1.1);
        assert_eq!(p.text().stable, "so the");
        assert_eq!(p.text().unstable, "widget should");
    }

    #[test]
    fn rest_after_a_pass_is_the_larger_of_the_floor_the_interval_gap_and_the_pass() {
        // (pass seconds, expected rest): max(0.15, 0.5 - p, p).
        for (p_secs, rest) in [(0.1, 0.4), (0.3, 0.3), (0.6, 0.6)] {
            let mut p = started();
            let src = Src::tone(48_000);
            let req = p.next(10.0, &src).unwrap();
            p.on_result(req.seq, ok("one two three"), p_secs, 20.0);
            assert!(
                p.next(20.0 + rest - 0.001, &src).is_none(),
                "p {p_secs} must rest {rest}"
            );
            assert!(
                p.next(20.0 + rest + 0.001, &src).is_some(),
                "p {p_secs} must run after {rest}"
            );
        }
    }

    #[test]
    fn a_tail_error_keeps_the_text() {
        let mut p = started();
        let src = Src::tone(48_000);
        let a = p.next(1.0, &src).unwrap();
        p.on_result(a.seq, ok("so the widget should"), 0.1, 1.1);
        let b = p.next(2.0, &src).unwrap();
        p.on_result(b.seq, Err("boom".into()), 0.1, 2.1);
        assert_eq!(p.text().full(), "so the widget should");
        assert!(p.is_running());
        assert!(p.next(3.0, &src).is_some(), "the next pass still runs");
    }

    #[test]
    fn a_stale_seq_is_ignored() {
        let mut p = started();
        let src = Src::tone(48_000);
        let req = p.next(1.0, &src).unwrap();
        p.on_result(req.seq + 100, ok("wrong words here"), 0.1, 1.1);
        assert!(p.text().is_empty());
        // Still waiting for the real answer.
        assert!(p.next(9.0, &src).is_none());
        p.on_result(req.seq, ok("right words here"), 0.1, 9.1);
        assert_eq!(p.text().full(), "right words here");
    }

    // Slow-pass stop

    #[test]
    fn a_pass_of_exactly_one_second_continues() {
        let mut p = started();
        let src = Src::tone(48_000);
        let req = p.next(1.0, &src).unwrap();
        p.on_result(req.seq, ok("one two three"), 1.0, 2.0);
        assert!(p.is_running());
        assert!(p.next(3.5, &src).is_some());
    }

    #[test]
    fn a_pass_over_one_second_stops_the_preview_and_keeps_the_text() {
        let mut p = started();
        let src = Src::tone(64_000);
        let a = p.next(1.0, &src).unwrap();
        p.on_result(a.seq, ok("so the widget should"), 0.3, 1.3);
        let b = p.next(2.0, &src).unwrap();
        p.on_result(b.seq, Err("slow".into()), 1.01, 3.1);
        assert!(!p.is_running());
        assert_eq!(p.text().full(), "so the widget should");
        assert!(p.next(60.0, &src).is_none());
    }

    #[test]
    fn a_slow_first_pass_stops_the_preview_for_good() {
        let mut p = started();
        let src = Src::tone(64_000);
        let a = p.next(1.0, &src).unwrap();
        p.on_result(a.seq, ok("one two three"), 1.01, 2.1);
        assert!(!p.is_running());
        assert!(p.next(60.0, &src).is_none());
    }

    // Freeze

    #[test]
    fn exactly_96000_samples_is_still_a_tail_pass() {
        let mut p = started();
        let src = Src::tone(96_001);
        src.set(96_000);
        let req = p.next(1.0, &src).unwrap();
        assert_eq!(req.samples.len(), 96_000);
        assert_eq!(req.samples, src.audio[..96_000].to_vec());
    }

    #[test]
    fn one_sample_over_96000_freezes_at_the_quietest_cut() {
        let mut p = started();
        let src = Src::with_gap(96_001, 80_000);
        let req = p.next(1.0, &src).unwrap();
        assert!(
            (64_000..=88_000).contains(&req.samples.len()),
            "cut {}",
            req.samples.len()
        );
        assert_eq!(req.samples.len(), 80_800);
        assert_eq!(req.samples, src.audio[..80_800].to_vec());
    }

    #[test]
    fn a_freeze_result_joins_the_frozen_text_and_a_tail_pass_follows_at_once() {
        let mut p = started();
        let src = Src::with_gap(96_001, 80_000);
        let freeze = p.next(1.0, &src).unwrap();
        p.on_result(freeze.seq, ok("alpha beta gamma"), 0.4, 1.4);
        // No rest and no one-second minimum: the tail is 15 201 samples.
        let tail = p.next(1.4, &src).expect("tail pass right after the freeze");
        assert_eq!(tail.samples.len(), 96_001 - 80_800);
        assert_eq!(tail.samples, src.audio[80_800..].to_vec());
        p.on_result(tail.seq, ok("delta epsilon zeta"), 0.1, 1.5);
        assert_eq!(p.text().stable, "alpha beta gamma delta");
        assert_eq!(p.text().unstable, "epsilon zeta");
    }

    #[test]
    fn a_freeze_error_still_advances_the_frozen_point() {
        let mut p = started();
        let src = Src::with_gap(96_001, 80_000);
        let freeze = p.next(1.0, &src).unwrap();
        p.on_result(freeze.seq, Err("boom".into()), 0.4, 1.4);
        let tail = p.next(1.4, &src).expect("tail pass");
        assert_eq!(tail.samples.len(), 96_001 - 80_800);
        p.on_result(tail.seq, ok("delta epsilon zeta"), 0.1, 1.5);
        assert_eq!(p.text().full(), "delta epsilon zeta");
    }

    #[test]
    fn no_pass_is_longer_than_the_freeze_window() {
        let cfg = Config::default();
        let mut p = LivePreview::new(cfg.clone());
        p.start(0.0);
        let src = Src::tone(60 * 16_000);
        let pass_secs = 0.3;
        let mut running: Option<(u64, f64)> = None;
        let mut requests = 0;
        let mut longest = 0;
        let mut now = 0.0;
        for step in 1..=600 {
            now = step as f64 * 0.1;
            src.set(step * 1_600);
            if let Some((seq, due)) = running {
                if now >= due {
                    p.on_result(seq, ok("some words came out of this pass"), pass_secs, now);
                    running = None;
                }
            }
            if running.is_none() {
                if let Some(req) = p.next(now, &src) {
                    longest = longest.max(req.samples.len());
                    assert!(
                        req.samples.len() <= cfg.freeze_after,
                        "a request of {} samples at {now:.1} s",
                        req.samples.len()
                    );
                    requests += 1;
                    running = Some((req.seq, now + pass_secs));
                }
            }
        }
        assert!(now >= 59.9);
        assert!(p.is_running(), "0.3 s passes never trip the slow stop");
        assert!(requests > 60, "only {requests} requests in 60 s");
        assert!(
            longest > 64_000,
            "longest request {longest}: the freeze never ran"
        );
    }

    // Lifecycle

    #[test]
    fn stop_keeps_the_text_and_drops_a_late_result() {
        let mut p = started();
        let src = Src::tone(64_000);
        let a = p.next(1.0, &src).unwrap();
        p.on_result(a.seq, ok("so the widget should"), 0.1, 1.1);
        let b = p.next(2.0, &src).unwrap();
        p.stop();
        assert!(!p.is_running());
        assert_eq!(p.text().full(), "so the widget should");
        p.on_result(b.seq, ok("completely different words"), 0.1, 2.1);
        assert_eq!(p.text().full(), "so the widget should");
        assert!(p.next(9.0, &src).is_none());
    }

    #[test]
    fn clear_empties_the_text_and_stops() {
        let mut p = started();
        let src = Src::tone(48_000);
        let a = p.next(1.0, &src).unwrap();
        p.on_result(a.seq, ok("so the widget should"), 0.1, 1.1);
        p.clear();
        assert!(p.text().is_empty());
        assert!(!p.is_running());
        assert!(p.next(9.0, &src).is_none());
    }

    #[test]
    fn start_resets_the_text_and_the_frozen_point() {
        let mut p = started();
        let src = Src::with_gap(96_001, 80_000);
        let freeze = p.next(1.0, &src).unwrap();
        p.on_result(freeze.seq, ok("alpha beta gamma"), 0.4, 1.4);
        let tail = p.next(1.4, &src).unwrap();
        p.on_result(tail.seq, ok("delta epsilon zeta"), 0.1, 1.5);
        assert!(!p.text().is_empty());

        p.start(5.0);
        assert!(p.is_running());
        assert!(p.text().is_empty());
        let fresh = Src::tone(32_000);
        let req = p
            .next(5.0, &fresh)
            .expect("first pass of the new recording");
        assert_eq!(req.samples.len(), 32_000, "the frozen point is back at 0");
    }

    #[test]
    fn a_result_from_before_a_restart_is_ignored() {
        let mut p = started();
        let src = Src::tone(48_000);
        let old = p.next(1.0, &src).unwrap();
        p.start(5.0);
        p.on_result(old.seq, ok("stale words from before"), 0.1, 5.1);
        assert!(p.text().is_empty());
    }
}
