// shepit-smoke: runs the core pipeline without the app. Recognizer(s) on WAV files (or a given
// transcript), then the Claude worker (or Codex with --codex), then prints what would have been inserted.
//
//   swift run shepit-smoke --audio file.wav [--engine parakeet-v3] [--profile claude-code-terminal] [--mode clean]
//   swift run shepit-smoke --text "ем зроби рефакторинг хендл сабміт" [--profile ai-chat] [--mode prompt]
//   swift run shepit-smoke --audio "$HOME/Library/Application Support/Shepit/recordings" \
//       --engine parakeet-v3 --engine parakeet-ultra --engine whisper-turbo --no-claude
//
// --audio accepts files and directories (every .wav inside, sorted). With several --engine values each
// file is transcribed by every engine and the results are printed side by side; formatting is skipped.
// --lang uk|en locks the language, --vocab "a, b" passes dictionary terms to the recognizer.
// --live plays each file in real time through the live preview (Parakeet) and prints every update.
// --progressive plays each file in real time through Whisper's piecewise transcription and compares the
// wait after release with transcribing the whole file at once.
// --codex polishes through `codex app-server` instead of Claude Code; --model then takes a Codex model slug
// (default gpt-6-luna) and --effort a reasoning effort (default low).

import Combine
import Foundation
import ShepitCore

@main
struct Smoke {
    static func main() async {
        var audio: [String] = []
        var texts: [String] = []
        var engineIDs: [EngineID] = []
        var profile = BuiltInProfiles.claudeCodeTerminal
        var mode = Mode.clean
        var model: String?
        var effort = "low"
        var useCodex = false
        var noClaude = false
        var live = false
        var progressive = false
        var before: String?
        var hints = TranscriptionHints()

        var args = Array(CommandLine.arguments.dropFirst())
        while !args.isEmpty {
            let a = args.removeFirst()
            switch a {
            case "--audio": audio.append(args.removeFirst())
            case "--text": texts.append(args.removeFirst())
            case "--engine":
                let raw = args.removeFirst()
                guard let id = EngineID(rawValue: raw) else {
                    print("unknown engine \(raw); known: \(EngineID.allCases.map(\.rawValue).joined(separator: ", "))")
                    exit(2)
                }
                engineIDs.append(id)
            case "--lang": hints.language = args.removeFirst()
            case "--vocab":
                hints.vocabulary = args.removeFirst().split(separator: ",")
                    .map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
            case "--profile": profile = BuiltInProfiles.byID(args.removeFirst()) ?? profile
            case "--mode": mode = Mode(rawValue: args.removeFirst()) ?? mode
            case "--model": model = args.removeFirst()
            case "--effort": effort = args.removeFirst()
            case "--codex": useCodex = true
            case "--no-claude": noClaude = true
            case "--live": live = true
            case "--progressive": progressive = true
            case "--before": before = args.removeFirst()
            default:
                print("unknown argument \(a)")
                exit(2)
            }
        }
        if engineIDs.isEmpty { engineIDs = [.parakeetV3] }
        let files = expand(audio)
        if live {
            await runLive(files, hints: hints)
            return
        }
        if progressive {
            await runProgressive(files, hints: hints)
            return
        }
        guard !files.isEmpty || !texts.isEmpty else {
            print("usage: shepit-smoke (--audio FILE_OR_DIR | --text TRANSCRIPT)... [--engine ID]... [--lang uk|en] [--vocab \"a, b\"] [--profile ID] [--mode clean|prompt|raw] [--model haiku|sonnet|SLUG] [--codex] [--effort low|medium] [--no-claude] [--live] [--progressive] [--before TEXT]")
            exit(2)
        }

        var transcripts: [(name: String, text: String, language: String?, seconds: Double)] = texts.map { ("text", $0, nil, 0) }
        if !files.isEmpty {
            var results: [String: [EngineID: Transcript]] = [:]
            for id in engineIDs {
                let engines = EngineManager()
                let t0 = Date()
                do {
                    let engine = try await engines.engine(for: id)
                    print(String(format: "%@ loaded in %.1f s", id.rawValue, Date().timeIntervalSince(t0)))
                    var total = 0.0
                    for url in files {
                        let samples = try AudioFile.loadSamples16k(url)
                        let transcript = try await engine.transcribe(samples, hints: hints)
                        total += transcript.processingSeconds
                        results[url.lastPathComponent, default: [:]][id] = transcript
                        if engineIDs.count == 1 {
                            let confidence = transcript.confidence.map { String(format: ", confidence %.3f", $0) } ?? ""
                            print(String(format: "\n[%@] %.1f s audio, stt %.2f s%@: %@", url.lastPathComponent,
                                         transcript.audioSeconds, transcript.processingSeconds, confidence, transcript.text))
                            transcripts.append((url.lastPathComponent, transcript.text, transcript.language, transcript.audioSeconds))
                        }
                    }
                    if engineIDs.count > 1 {
                        print(String(format: "%@: %d files, stt total %.2f s", id.rawValue, files.count, total))
                    }
                    await engines.unloadAll()
                } catch {
                    print("recognizer \(id.rawValue) failed: \(error)")
                    exit(1)
                }
            }
            if engineIDs.count > 1 {
                for url in files {
                    let name = url.lastPathComponent
                    let seconds = results[name]?.values.first?.audioSeconds ?? 0
                    print(String(format: "\n[%@] %.1f s", name, seconds))
                    for id in engineIDs {
                        guard let t = results[name]?[id] else { continue }
                        let label = id.rawValue.padding(toLength: 15, withPad: " ", startingAt: 0)
                        let lang = t.language.map { " (\($0))" } ?? ""
                        print(String(format: "  %@ %5.2f s%@  %@", label, t.processingSeconds, lang, t.text))
                    }
                }
                print("\nFormatting skipped: several engines given.")
                return
            }
        }

        var worker: ClaudeCodeWorker?
        var codex: CodexFormatter?
        if !noClaude, useCodex {
            guard let exe = PolishingProvider.codex.resolveExecutable(configured: "") else {
                print("codex executable not found"); exit(1)
            }
            let work = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-smoke-codex-\(getpid())", isDirectory: true)
            try? FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
            let config = CodexConfig(executable: exe, model: model ?? CodexConfig.defaultModel, reasoningEffort: effort,
                                     systemPrompt: PromptBuilder.systemPrompt(speakerNotes: SpeakerNotes.defaultText), workDirectory: work)
            let c = CodexFormatter(config: config)
            let t0 = Date()
            await c.warmUp()
            print(String(format: "codex warm in %.2f s (stderr: %@/codex-stderr.log)", Date().timeIntervalSince(t0), work.path))
            codex = c
        } else if !noClaude {
            let home = FileManager.default.homeDirectoryForCurrentUser.path
            let candidates = ["\(home)/.local/bin/claude", "/opt/homebrew/bin/claude", "/usr/local/bin/claude"]
            guard let exe = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else {
                print("claude executable not found"); exit(1)
            }
            let work = FileManager.default.temporaryDirectory.appendingPathComponent("shepit-smoke-\(getpid())", isDirectory: true)
            try? FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
            let config = ClaudeWorkerConfig(executable: exe, model: model ?? "haiku", systemPrompt: PromptBuilder.systemPrompt(speakerNotes: SpeakerNotes.defaultText), workDirectory: work)
            let w = ClaudeCodeWorker(config: config)
            let t0 = Date()
            await w.warmUp()
            print(String(format: "worker warm in %.2f s", Date().timeIntervalSince(t0)))
            worker = w
        }
        let formatting = FormattingService(claude: useCodex ? codex : worker)

        var context = TargetContext(bundleID: "com.apple.Terminal", appName: "Terminal", windowTitle: "Claude Code — shepit")
        context.textBeforeCaret = before
        let dictionary = Array(Set(["Shepit", "Claude Code", "handleSubmit", "UserService.swift"] + hints.vocabulary)).sorted()
        for t in transcripts {
            let resolvedMode = ModeResolver.resolve(requested: mode, profile: profile, wordCount: ModeResolver.wordCount(t.text), promptModeMinWords: 40)
            let request = FormatRequest(transcript: t.text, languageHint: hints.language ?? t.language, mode: resolvedMode,
                                        profile: profile, context: context, dictionary: dictionary)
            let t0 = Date()
            let result = await formatting.format(request)
            print(String(format: "\n[%@] %@/%@ via %@ in %.2f s%@\n%@", t.name, profile.id, resolvedMode.rawValue, result.source.rawValue,
                         Date().timeIntervalSince(t0), result.note.map { " (\($0))" } ?? "", result.text))
            if !result.newTerms.isEmpty { print("new terms: \(result.newTerms)") }
        }
        if let worker {
            let usage = await worker.lastUsage
            if !usage.isEmpty { print("\nlast usage: \(usage)") }
            await worker.shutdown()
        }
        await codex?.shutdown()
    }

    /// Files as given; directories contribute every .wav inside, sorted by name.
    @MainActor
    static func runProgressive(_ files: [URL], hints: TranscriptionHints) async {
        let engines = EngineManager()
        guard let whisper = try? await engines.engine(for: .whisperTurbo) else {
            print("whisper failed to load"); exit(1)
        }
        for url in files {
            guard let samples = try? AudioFile.loadSamples16k(url) else { continue }
            let seconds = Double(samples.count) / 16_000
            print(String(format: "\n[%@] %.1f s", url.lastPathComponent, seconds))
            if let whole = try? await whisper.transcribe(samples, hints: hints) {
                print(String(format: "  whole file:  %.2f s after release (%@)\n    %@", whole.processingSeconds,
                             whole.language ?? "?", whole.text))
            }
            let source = PlaybackSource(samples)
            let pieces = ProgressiveTranscriber()
            pieces.start(source: source, engine: { whisper }, hints: hints)
            try? await Task.sleep(for: .seconds(seconds))
            let t0 = Date()
            if let done = await pieces.complete(samples) {
                print(String(format: "  progressive: %.2f s after release (%@)\n    %@", Date().timeIntervalSince(t0),
                             done.transcript.language ?? "?", done.transcript.text))
            } else {
                print("  progressive: not used (too short or failed)")
            }
        }
    }

    /// Feeds each file to the live preview as if it were being spoken, then prints what the preview showed.
    @MainActor
    static func runLive(_ files: [URL], hints: TranscriptionHints) async {
        let engines = EngineManager()
        do {
            _ = try await engines.engine(for: .parakeetV3)
        } catch {
            print("parakeet failed: \(error)"); exit(1)
        }
        for url in files {
            guard let samples = try? AudioFile.loadSamples16k(url) else { continue }
            print(String(format: "\n[%@] %.1f s", url.lastPathComponent, Double(samples.count) / 16_000))
            let source = PlaybackSource(samples)
            let preview = LivePreview()
            var passes: [Double] = []
            let watch = preview.$text.removeDuplicates().dropFirst().sink { text in
                passes.append(preview.lastPassSeconds)
                print(String(format: "  %5.1f s  %@ [%@]", source.elapsed, text.stable, text.unstable))
            }
            preview.start(source: source, engine: { await engines.previewEngine() },
                          hints: TranscriptionHints(language: hints.language))
            try? await Task.sleep(for: .seconds(Double(samples.count) / 16_000 + 0.6))
            preview.stop()
            watch.cancel()
            if !passes.isEmpty {
                print(String(format: "  %d updates, pass time %.2f-%.2f s", passes.count, passes.min()!, passes.max()!))
            }
        }
    }

    static func expand(_ paths: [String]) -> [URL] {
        var urls: [URL] = []
        let fm = FileManager.default
        for path in paths {
            let expanded = NSString(string: path).expandingTildeInPath
            var isDir: ObjCBool = false
            guard fm.fileExists(atPath: expanded, isDirectory: &isDir) else {
                print("no such file: \(path)")
                exit(2)
            }
            if isDir.boolValue {
                let names = ((try? fm.contentsOfDirectory(atPath: expanded)) ?? []).filter { $0.lowercased().hasSuffix(".wav") }.sorted()
                urls += names.map { URL(fileURLWithPath: expanded).appendingPathComponent($0) }
            } else {
                urls.append(URL(fileURLWithPath: expanded))
            }
        }
        return urls
    }
}

/// A recording that grows at real-time speed.
@MainActor
final class PlaybackSource: LiveAudioSource {
    private let samples: [Float]
    private let start = Date()

    init(_ samples: [Float]) { self.samples = samples }

    var elapsed: Double { Date().timeIntervalSince(start) }
    var capturedSampleCount: Int { min(samples.count, Int(elapsed * 16_000)) }
    func capturedSamples(from start: Int) -> [Float] {
        let end = capturedSampleCount
        return start < end ? Array(samples[start..<end]) : []
    }
}
