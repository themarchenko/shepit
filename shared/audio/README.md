# Audio fixtures

Short speech clips for recognizer tests. Format: WAV, 16 kHz, mono, 16-bit. `manifest.json` lists
each clip: `file`, `language`, `text` (what is said), `must_contain` (lowercase words the transcript
has to include after lowercasing and stripping punctuation), `source` and `license`.

Only the Rust runner reads this today (`windows/engine/tests/transcribe_fixture.rs`, ignored by
default, needs the model). The Swift side does not use it yet.

## en-librivox-preamble.wav

First 7.5 s of `wonderland_ch_01_64kb.mp3` from
https://archive.org/details/alice_in_wonderland_librivox (LibriVox recording of Alice's Adventures
in Wonderland). The reader is not named in the clip. License: public domain (LibriVox). Made with
`afconvert -f WAVE -d LEI16@16000 -c 1` and cut to 7.5 s.
