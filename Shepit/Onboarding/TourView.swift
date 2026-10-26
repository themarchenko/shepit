import AppKit
import SwiftUI
import ShepitCore

/// The welcome tour on first launch: five short steps in the widget's black glass.
struct TourView: View {
    @ObservedObject var settings: AppSettings
    var requestMicrophone: () async -> Bool
    var finish: () -> Void

    @State private var step = 0
    @StateObject private var demo = WidgetDemo(style: .flowPill)
    @State private var demoText = ""
    @State private var microphone = AudioRecorder.permissionGranted
    @State private var accessibility = ContextProbe.isAccessibilityTrusted
    @State private var tryText = ""
    @FocusState private var tryFocused: Bool
    private let poll = Timer.publish(every: 1, on: .main, in: .common).autoconnect()
    private static let stepCount = 5

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            ShepitLockup(size: 20).opacity(0.9)
            Group {
                switch step {
                case 0: welcome
                case 1: permissions
                case 2: look
                case 3: polishing
                default: tryIt
                }
            }
            .frame(maxWidth: .infinity, minHeight: 350, alignment: .topLeading)
            .id(step)
            .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 8)), removal: .opacity))
            footer
        }
        .padding(.horizontal, 28)
        .padding(.top, 26)
        .padding(.bottom, 18)
        .frame(width: 620)
        .foregroundStyle(WidgetPalette.text)
        .glass(RoundedRectangle(cornerRadius: 18, style: .continuous), opacity: 0.76)
        .overlay(alignment: .topTrailing) {
            Button("Skip", action: finish).buttonStyle(QuietButtonStyle()).padding(10)
        }
        .environment(\.colorScheme, .dark)
        .animation(.easeOut(duration: 0.3), value: step)
        .onAppear {
            demo.model.style = settings.widgetStyle
            demo.onInserted = { text in demoText = text }
            demo.onReset = { demoText = "" }
            demo.start()
        }
        .onDisappear { demo.stop() }
        .onChange(of: settings.widgetStyle) { _, style in demo.model.style = style }
        .onReceive(poll) { _ in
            microphone = AudioRecorder.permissionGranted
            accessibility = ContextProbe.isAccessibilityTrusted
        }
        .onExitCommand(perform: finish)
    }

    // MARK: Steps

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Speak instead of typing")
            HStack(spacing: 5) {
                Text("Hold")
                KeyCap(label: settings.hotkey.keyLabel)
                Text(", say what you want to write, let go.")
            }
            .foregroundStyle(WidgetPalette.text.opacity(0.7))
            Text("Shepit types it into the app you are in, in Ukrainian or English.")
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
            fieldMock(demoText.isEmpty ? nil : demoText)
            DemoStage(model: demo.model, maxScale: 0.8, height: 150)
        }
    }

    private var permissions: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Two permissions")
            Text("Speech recognition runs on this Mac. The audio never leaves it.")
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
            permissionRow(symbol: "mic.fill", name: "Microphone", detail: "To hear you while the key is held.",
                          granted: microphone, button: "Allow") {
                Task { microphone = await requestMicrophone() }
            }
            permissionRow(symbol: "accessibility", name: "Accessibility",
                          detail: "To notice the key in any app and paste the text where your cursor is.",
                          granted: accessibility, button: "Open System Settings") {
                ContextProbe.requestAccessibility()
            }
        }
    }

    private var look: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Pick a look")
            Text("The widget sits at the bottom of the screen. Drag it anywhere and it stays there. You can switch later in Settings.")
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
                .fixedSize(horizontal: false, vertical: true)
            StylePicker(settings: settings)
        }
    }

    private var polishing: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Polish the text")
            Text("An AI agent you already use can fix punctuation, names and code terms. It gets the text only, never the audio.")
                .foregroundStyle(WidgetPalette.text.opacity(0.7))
                .fixedSize(horizontal: false, vertical: true)
            ProviderList(settings: settings)
        }
    }

    private var tryIt: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Try it")
            HStack(spacing: 5) {
                Text("Click the box, hold")
                KeyCap(label: settings.hotkey.keyLabel)
                Text("and say a sentence.")
            }
            .foregroundStyle(WidgetPalette.text.opacity(0.7))
            TextEditor(text: $tryText)
                .font(.system(size: 14))
                .scrollContentBackground(.hidden)
                .foregroundStyle(Color(red: 0.17, green: 0.17, blue: 0.16))
                .padding(10)
                .frame(height: 120)
                .background(RoundedRectangle(cornerRadius: 10).fill(Color(red: 0.984, green: 0.98, blue: 0.969)))
                .focused($tryFocused)
                .environment(\.colorScheme, .light)
            Text("The words show up in the widget while you speak; the finished text lands here.")
                .font(.system(size: 12))
                .foregroundStyle(WidgetPalette.text.opacity(0.55))
        }
        .onAppear { tryFocused = true }
    }

    // MARK: Pieces

    private var footer: some View {
        HStack {
            HStack(spacing: 6) {
                ForEach(0..<Self.stepCount, id: \.self) { i in
                    Capsule().fill(i == step ? WidgetPalette.text : Color.white.opacity(0.2))
                        .frame(width: i == step ? 18 : 6, height: 6)
                }
            }
            .animation(WidgetPalette.spring, value: step)
            Spacer()
            if step > 0 {
                Button("Back") { step -= 1 }.buttonStyle(QuietButtonStyle())
            }
            Button(step == Self.stepCount - 1 ? "Start dictating" : "Continue") {
                if step == Self.stepCount - 1 { finish() } else { step += 1 }
            }
            .buttonStyle(PrimaryButtonStyle())
            .keyboardShortcut(.defaultAction)
        }
    }

    private func title(_ text: String) -> some View {
        Text(text).font(.system(size: 22, weight: .semibold))
    }

    private func fieldMock(_ text: String?) -> some View {
        HStack(alignment: .top, spacing: 2) {
            if let text {
                Text(text).transition(.opacity)
            }
            Rectangle().frame(width: 1.5, height: 18)
        }
        .font(.system(size: 14))
        .foregroundStyle(Color(red: 0.17, green: 0.17, blue: 0.16))
        .padding(.horizontal, 14).padding(.vertical, 12)
        .frame(maxWidth: .infinity, minHeight: 64, alignment: .topLeading)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(red: 0.984, green: 0.98, blue: 0.969)))
        .animation(.easeOut(duration: 0.3), value: text)
    }

    private func permissionRow(symbol: String, name: String, detail: String, granted: Bool, button: String,
                               action: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 20)).frame(width: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(name).font(.system(size: 13, weight: .medium))
                Text(detail).font(.system(size: 12.5)).foregroundStyle(WidgetPalette.text.opacity(0.6))
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            if granted {
                Label("Allowed", systemImage: "checkmark.circle.fill")
                    .font(.system(size: 12.5))
                    .foregroundStyle(WidgetPalette.ok)
            } else {
                Button(button, action: action)
                    .buttonStyle(.bordered)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(RoundedRectangle(cornerRadius: 12).fill(Color.white.opacity(0.05)))
    }
}

/// A borderless window for the tour that can take keyboard focus (the last step has a text box to dictate into).
final class TourWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

@MainActor
final class TourWindowController: NSWindowController, NSWindowDelegate {
    var onFinish: () -> Void = {}

    init(settings: AppSettings, requestMicrophone: @escaping () async -> Bool) {
        let window = TourWindow(contentRect: NSRect(x: 0, y: 0, width: 680, height: 560),
                                styleMask: [.borderless], backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        window.level = .floating
        super.init(window: window)
        let view = TourView(settings: settings, requestMicrophone: requestMicrophone) { [weak self] in self?.finish() }
            .padding(30)
        let hosting = NSHostingView(rootView: view)
        hosting.sizingOptions = [.intrinsicContentSize]
        window.contentView = hosting
        window.setContentSize(hosting.fittingSize)
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Opens on the display the pointer is on, a little above the middle.
    func show() {
        if let window {
            let mouse = NSEvent.mouseLocation
            let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
            if let visible = screen?.visibleFrame {
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
        onFinish()
    }
}
