import AppKit
import SwiftUI
import ShepitCore

/// Drives the voice calibration wizard. The flow value decides the order of steps; this class owns the
/// recording state, the two agent rounds and what gets saved. Everything outside the model (the dictation
/// session, the agent) comes in as closures, so a view can be rendered without them.
@MainActor
final class CalibrationModel: ObservableObject {
    enum Phase { case idle, listening, checking }

    struct Term: Identifiable {
        let id = UUID()
        let text: String
        var on: Bool
    }

    let settings: AppSettings
    @Published private(set) var flow = CalibrationFlow()
    @Published var phase: Phase = .idle
    @Published var policy: LanguagePolicy
    @Published var draft = ""
    @Published var terms: [Term] = []
    @Published var errorText = ""
    /// False when round 1 fell back to the fixed sentences; only changes the subtitle.
    @Published var usedAgent = false
    @Published private(set) var targetCount = 0

    private let setSink: (((CalibrationTake) -> Void)?) -> Void
    private let startRecording: () -> Void
    private let stopRecording: () -> Void
    private let cancelRecording: () -> Void
    private let makeCoach: (LanguagePolicy) -> CalibrationCoach
    private var task: Task<Void, Never>?
    private var closed = false
    /// The prompt on screen when the key went down; a take that finishes later is stored under this one.
    private var pressedPromptID: String?
    private var tooFew = false
    var onClose: () -> Void = {}
    /// Called whenever a reading step comes up, so the window can take focus back from another app.
    var onReading: () -> Void = {}

    init(settings: AppSettings,
         setSink: @escaping (((CalibrationTake) -> Void)?) -> Void,
         startRecording: @escaping () -> Void, stopRecording: @escaping () -> Void,
         cancelRecording: @escaping () -> Void,
         makeCoach: @escaping (LanguagePolicy) -> CalibrationCoach) {
        self.settings = settings
        self.policy = settings.language
        self.setSink = setSink
        self.startRecording = startRecording
        self.stopRecording = stopRecording
        self.cancelRecording = cancelRecording
        self.makeCoach = makeCoach
    }

    var stage: CalibrationFlow.Stage { flow.stage }
    var agentName: String { settings.polishing.displayName }
    var currentTake: CalibrationTake? { flow.currentTake }
    /// Back, Again and Next change which prompt a take belongs to, so they wait for the take to finish.
    var canStep: Bool { phase == .idle }

    /// 0 intro, 1 words, 2 phrases, 3 the picked sentences, 4 notes.
    var progress: Int {
        switch flow.stage {
        case .intro: return 0
        case .reading(.words, _): return 1
        case .reading(.phrases, _): return 2
        case .reading(.targeted, _), .pickingSentences: return 3
        case .writingNotes, .review, .failed: return 4
        }
    }

    var canGoBack: Bool {
        guard case let .reading(section, index) = flow.stage else { return false }
        return !(section == .targeted && index == 0)
    }

    /// "Words · 3 of 8" and so on.
    var caption: String {
        guard case let .reading(section, index) = flow.stage else { return "" }
        switch section {
        case .words: return String(localized: "Words · \(index + 1) of \(CalibrationScript.words(for: policy).count)")
        case .phrases: return String(localized: "Phrases · \(index + 1) of \(CalibrationScript.phrases(for: policy).count)")
        case .targeted: return String(localized: "A few more · \(index + 1) of \(targetCount)")
        }
    }

    var status: String? {
        switch phase {
        case .listening: return String(localized: "Listening…")
        case .checking: return String(localized: "Checking…")
        case .idle:
            guard let take = flow.currentTake else { return nil }
            return take.cleaned.isEmpty ? String(localized: "Did not catch that. Try again.") : String(localized: "Heard: \(take.cleaned)")
        }
    }

    // MARK: Steps

    /// The sink is set only while a prompt is on screen: a dictation made on any other step goes the normal way.
    private func syncSink() {
        if case .reading = flow.stage, !closed {
            onReading()
            setSink { [weak self] take in
                MainActor.assumeIsolated { self?.received(take) }
            }
        } else {
            setSink(nil)
        }
    }

    func start() {
        settings.language = policy
        flow.start(policy: policy)
        syncSink()
    }

    private func received(_ take: CalibrationTake) {
        guard case .reading = flow.stage, let id = pressedPromptID else { return }
        pressedPromptID = nil
        flow.record(take, for: id)
        phase = .idle
    }

    func pressBegan() {
        guard phase == .idle, case .reading = flow.stage else { return }
        pressedPromptID = flow.currentPrompt?.id
        startRecording()
    }

    func pressEnded() {
        if phase == .listening { stopRecording() }
    }

    /// Bridge from the dictation session: recording is listening, recognition is checking.
    func sessionChanged(_ state: DictationSession.State) {
        guard case .reading = flow.stage else { phase = .idle; return }
        switch state {
        case .recording: phase = .listening
        case .loadingModel, .transcribing: phase = .checking
        default: phase = .idle
        }
    }

    func again() {
        flow.again()
    }

    func next() {
        flow.next()
        advance()
    }

    func back() {
        flow.back()
        syncSink()
    }

    private func advance() {
        syncSink()
        switch flow.stage {
        case .pickingSentences: beginTargets()
        case .writingNotes: beginProfile()
        default: break
        }
    }

    private func beginTargets() {
        let pairs = flow.pairs(includeTargeted: false)
        let coach = makeCoach(policy)
        task?.cancel()
        task = Task { [weak self] in
            let (prompts, used) = await coach.targets(pairs: pairs)
            guard let self, !Task.isCancelled, !self.closed else { return }
            self.usedAgent = used
            self.targetCount = prompts.count
            self.flow.setTargeted(prompts)
            self.syncSink()
        }
    }

    private func beginProfile() {
        let pairs = flow.pairs(includeTargeted: true)
        let coach = makeCoach(policy)
        let existing = settings.dictionary
        task?.cancel()
        task = Task { [weak self] in
            do {
                let profile = try await coach.profile(pairs: pairs, existingDictionary: existing)
                guard let self, !Task.isCancelled, !self.closed else { return }
                self.draft = profile.notes
                self.terms = profile.terms.map { Term(text: $0, on: true) }
                self.flow.profileReady()
            } catch {
                guard let self, !Task.isCancelled, !self.closed else { return }
                self.fail(error)
            }
        }
    }

    private func fail(_ error: Error) {
        if let error = error as? CalibrationError, error == .tooFewTakes {
            tooFew = true
            errorText = String(localized: "Read at least three items, then try again.")
        } else {
            tooFew = false
            errorText = Doctor.agentFailureText(kind: CalibrationCoach.failureKind(error))
        }
        flow.profileFailed()
    }

    /// With too few readings another request would fail the same way, so the wizard starts over instead.
    func retry() {
        if tooFew {
            tooFew = false
            flow.start(policy: policy)
            syncSink()
        } else {
            flow.retry()
            advance()
        }
    }

    func save() {
        let notes = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        settings.speakerNotes = notes
        let picked = terms.filter(\.on).map(\.text)
        if !picked.isEmpty {
            settings.dictionary = CalibrationMessages.mergeDictionary(existing: settings.dictionary, adding: picked)
        }
        close()
    }

    /// Clears the sink and stops whatever is running, so nothing keeps feeding a window that is gone.
    func close() {
        guard !closed else { return }
        closed = true
        task?.cancel()
        task = nil
        setSink(nil)
        if phase != .idle { cancelRecording() }
        phase = .idle
        onClose()
    }

    // MARK: Render support

    /// Puts the model in a given state without running anything. Only the offscreen renders use it.
    func preview(flow: CalibrationFlow, take: CalibrationTake? = nil, phase: Phase = .idle, targetCount: Int = 0) {
        self.flow = flow
        self.targetCount = targetCount
        if let take { self.flow.record(take) }
        self.phase = phase
    }
}

struct CalibrationView: View {
    @ObservedObject var model: CalibrationModel
    @ObservedObject var settings: AppSettings
    @State private var pressing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            ShepitLockup(size: 20).opacity(0.9)
            Group {
                switch model.stage {
                case .intro: intro
                case .reading(let section, _): reading(section)
                case .pickingSentences: thinking("Picking sentences for you…")
                case .writingNotes: thinking("Writing your notes…")
                case .review: review
                case .failed: failed
                }
            }
            .frame(maxWidth: .infinity, minHeight: 420, maxHeight: 420, alignment: .topLeading)
            .id(String(describing: model.stage))
            .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 8)), removal: .opacity))
            footer
        }
        .padding(.horizontal, 28)
        .padding(.top, 26)
        .padding(.bottom, 18)
        .frame(width: 620)
        .foregroundStyle(WidgetPalette.text)
        .glass(RoundedRectangle(cornerRadius: 18, style: .continuous), opacity: 0.76)
        .gesture(WindowDragGesture())
        .overlay(alignment: .topTrailing) {
            Button("Close") { model.close() }.buttonStyle(QuietButtonStyle()).padding(10)
        }
        .environment(\.colorScheme, .dark)
        .animation(.easeOut(duration: 0.3), value: model.stage)
        // Esc while recording belongs to the dictation key handler, which cancels only the take.
        .onExitCommand { if model.phase == .idle { model.close() } }
    }

    // MARK: Stages

    private var intro: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Teach Shepit your voice")
            Text("Read a few words and phrases aloud. \(model.agentName) compares them with what Shepit heard and writes notes that help polishing. About 3 minutes.")
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 8) {
                Text("Languages you speak").font(.system(size: 13, weight: .medium))
                HStack(spacing: 2) {
                    ForEach(LanguagePolicy.allCases, id: \.self) { segment($0) }
                }
                .padding(3)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.white.opacity(0.06)))
            }
            .padding(.top, 6)
            Text("Only the phrases and what Shepit heard go to \(model.agentName). Never audio.")
                .font(.system(size: 12))
                .foregroundStyle(WidgetPalette.text.opacity(0.55))
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func segment(_ policy: LanguagePolicy) -> some View {
        let selected = model.policy == policy
        return Button {
            withAnimation(WidgetPalette.spring) { model.policy = policy }
        } label: {
            Text(policy.displayName)
                .font(.system(size: 12.5, weight: selected ? .semibold : .regular))
                .foregroundStyle(WidgetPalette.text.opacity(selected ? 1 : 0.6))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
                .background {
                    if selected {
                        RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Color.white.opacity(0.14))
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func reading(_ section: CalibrationFlow.Section) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text(model.caption).font(.system(size: 13, weight: .medium))
                if section == .targeted {
                    Text(model.usedAgent ? LocalizedStringKey("Picked for the sounds that went wrong") : LocalizedStringKey("Sounds recognizers often miss"))
                        .font(.system(size: 12))
                        .foregroundStyle(WidgetPalette.text.opacity(0.55))
                }
            }
            let prompt = model.flow.currentPrompt?.text ?? ""
            Text(prompt)
                .font(.system(size: section == .words ? 34 : 24, weight: .medium))
                .multilineTextAlignment(.center)
                .minimumScaleFactor(0.7)
                .frame(maxWidth: .infinity, minHeight: 150, maxHeight: 150)
                .padding(.horizontal, 20)
                .background(RoundedRectangle(cornerRadius: 12).fill(Color.white.opacity(0.05)))
            KeyCapLine(sentence: String(localized: "Hold \(KeyCapLine.slot) and read it aloud, or hold the button"),
                       key: settings.hotkey.keyLabel, font: .body, color: WidgetPalette.text.opacity(0.7))
                .frame(maxWidth: .infinity)
            micButton.frame(maxWidth: .infinity)
            Text(model.status ?? " ")
                .font(.system(size: 13))
                .foregroundStyle(WidgetPalette.text.opacity(model.currentTake?.cleaned.isEmpty == true && model.phase == .idle ? 0.9 : 0.6))
                .lineLimit(2)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity, minHeight: 34, alignment: .top)
        }
    }

    private var micButton: some View {
        let listening = model.phase == .listening
        return Image(systemName: "mic.fill")
            .font(.system(size: 20))
            .foregroundStyle(listening ? Color.white : WidgetPalette.text)
            .frame(width: 56, height: 56)
            .background(Circle().fill(listening ? WidgetPalette.rec : Color.white.opacity(0.12)))
            .scaleEffect(listening ? 1.08 : 1)
            .animation(WidgetPalette.spring, value: listening)
            .opacity(model.phase == .checking ? 0.5 : 1)
            .gesture(DragGesture(minimumDistance: 0)
                .onChanged { _ in
                    guard !pressing else { return }
                    pressing = true
                    model.pressBegan()
                }
                .onEnded { _ in
                    pressing = false
                    model.pressEnded()
                })
            .accessibilityLabel("Hold to record")
    }

    private func thinking(_ text: LocalizedStringKey) -> some View {
        VStack(spacing: 14) {
            Spacer()
            ProgressView().controlSize(.large)
            Text(text).font(.system(size: 16, weight: .medium))
            Text("This can take up to a minute.")
                .font(.system(size: 12))
                .foregroundStyle(WidgetPalette.text.opacity(0.55))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private var review: some View {
        VStack(alignment: .leading, spacing: 12) {
            title("Your speaker notes")
            Text("Polishing reads these before every dictation. Edit anything.")
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
            TextEditor(text: $model.draft)
                .font(.system(size: 13))
                .scrollContentBackground(.hidden)
                .foregroundStyle(Color(red: 0.17, green: 0.17, blue: 0.16))
                .padding(8)
                .frame(height: model.terms.isEmpty ? 240 : 170)
                .background(RoundedRectangle(cornerRadius: 10).fill(Color(red: 0.984, green: 0.98, blue: 0.969)))
                .environment(\.colorScheme, .light)
            if !model.terms.isEmpty {
                Text("Add to dictionary").font(.system(size: 13, weight: .medium)).padding(.top, 2)
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 170), spacing: 8, alignment: .leading)],
                          alignment: .leading, spacing: 6) {
                    ForEach($model.terms) { $term in
                        Button { term.on.toggle() } label: {
                            HStack(spacing: 6) {
                                Image(systemName: term.on ? "checkmark.square.fill" : "square")
                                    .foregroundStyle(WidgetPalette.text.opacity(term.on ? 1 : 0.5))
                                Text(term.text).font(.system(size: 12.5)).lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(term.on ? .isSelected : [])
                    }
                }
            }
        }
    }

    private var failed: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Could not write the notes")
            Text(model.errorText)
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    // MARK: Pieces

    private var footer: some View {
        HStack {
            HStack(spacing: 6) {
                ForEach(0..<5, id: \.self) { i in
                    Capsule().fill(i == model.progress ? WidgetPalette.text : Color.white.opacity(0.2))
                        .frame(width: i == model.progress ? 18 : 6, height: 6)
                }
            }
            .animation(WidgetPalette.spring, value: model.progress)
            Spacer()
            switch model.stage {
            case .intro:
                Button("Start") { model.start() }
                    .buttonStyle(PrimaryButtonStyle()).keyboardShortcut(.defaultAction)
            case .reading:
                if model.canGoBack {
                    Button("Back") { model.back() }.buttonStyle(QuietButtonStyle()).disabled(!model.canStep).opacity(model.canStep ? 1 : 0.4)
                }
                if model.currentTake != nil {
                    Button("Again") { model.again() }.buttonStyle(QuietButtonStyle()).disabled(!model.canStep).opacity(model.canStep ? 1 : 0.4)
                }
                Button(model.currentTake == nil ? LocalizedStringKey("Skip") : LocalizedStringKey("Next")) { model.next() }
                    .buttonStyle(PrimaryButtonStyle()).keyboardShortcut(.defaultAction).disabled(!model.canStep).opacity(model.canStep ? 1 : 0.4)
            case .pickingSentences, .writingNotes:
                EmptyView()
            case .review:
                Button("Cancel") { model.close() }.buttonStyle(QuietButtonStyle())
                Button("Save") { model.save() }.buttonStyle(PrimaryButtonStyle()).keyboardShortcut(.defaultAction)
            case .failed:
                Button("Close") { model.close() }.buttonStyle(QuietButtonStyle())
                Button("Try again") { model.retry() }.buttonStyle(PrimaryButtonStyle()).keyboardShortcut(.defaultAction)
            }
        }
        .frame(minHeight: 28)
    }

    private func title(_ text: LocalizedStringKey) -> some View {
        Text(text).font(.system(size: 22, weight: .semibold))
    }
}

/// The wizard's window: the tour's borderless glass window, so the two look alike.
@MainActor
final class CalibrationWindowController: NSWindowController {
    let model: CalibrationModel
    var onClose: () -> Void = {}

    init(model: CalibrationModel) {
        self.model = model
        let window = TourWindow(contentRect: NSRect(x: 0, y: 0, width: 680, height: 560),
                                styleMask: [.borderless], backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.isReleasedWhenClosed = false
        window.level = .normal
        super.init(window: window)
        let view = CalibrationView(model: model, settings: model.settings).padding(30)
        let hosting = NSHostingView(rootView: view)
        hosting.sizingOptions = [.intrinsicContentSize]
        window.contentView = hosting
        window.setContentSize(hosting.fittingSize)
        model.onClose = { [weak self] in self?.finish() }
        model.onReading = { [weak self] in self?.bringToFront() }
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Brings the window forward whenever a reading step starts: while one is open every dictation becomes a take,
    /// so the user should not be typing into another app.
    func bringToFront() {
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    func show() {
        if let window {
            let mouse = NSEvent.mouseLocation
            let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
            if let visible = screen?.visibleFrame, !window.isVisible {
                let size = window.frame.size
                window.setFrameOrigin(NSPoint(x: visible.midX - size.width / 2,
                                              y: visible.midY - size.height / 2 + visible.height * 0.06))
            }
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    private func finish() {
        window?.orderOut(nil)
        onClose()
    }
}
