import SwiftUI
import ShepitCore

/// The widget styles as a segmented choice over one live preview of the selected style, scaled so the
/// widget fits at its widest.
struct StylePicker: View {
    @ObservedObject var settings: AppSettings
    var stageHeight: CGFloat = 150
    @StateObject private var demo = WidgetDemo(style: .flowPill)
    @Namespace private var selection

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 2) {
                ForEach(WidgetStyle.allCases, id: \.self) { segment($0) }
            }
            .padding(3)
            .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.white.opacity(0.06)))
            DemoStage(model: demo.model, height: stageHeight)
            Text(settings.widgetStyle.blurb)
                .font(.system(size: 12))
                .foregroundStyle(WidgetPalette.text.opacity(0.6))
                .fixedSize(horizontal: false, vertical: true)
        }
        .onAppear {
            demo.model.style = settings.widgetStyle
            demo.start()
        }
        .onDisappear { demo.stop() }
        .onChange(of: settings.widgetStyle) { _, style in demo.model.style = style }
        .onChange(of: settings.glassOpacity, initial: true) { _, value in demo.model.glassOpacity = value }
    }

    private func segment(_ style: WidgetStyle) -> some View {
        let selected = settings.widgetStyle == style
        return Button {
            withAnimation(WidgetPalette.spring) { settings.widgetStyle = style }
        } label: {
            Text(style.displayName)
                .font(.system(size: 12.5, weight: selected ? .semibold : .regular))
                .foregroundStyle(WidgetPalette.text.opacity(selected ? 1 : 0.6))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
                .background {
                    if selected {
                        RoundedRectangle(cornerRadius: 7, style: .continuous)
                            .fill(Color.white.opacity(0.14))
                            .matchedGeometryEffect(id: "selection", in: selection)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// Claude Code, Codex or nothing, with what was found on this Mac. A missing agent cannot be picked, and when
/// the selected one is gone, the choice falls back to another agent or to off.
struct ProviderList: View {
    @ObservedObject var settings: AppSettings
    @State private var found: [PolishingProvider: String] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if found.isEmpty {
                Text("No AI agent found on this Mac, so Shepit inserts the text as heard. Install Claude Code or Codex and it shows up here.")
                    .font(.system(size: 12))
                    .foregroundStyle(WidgetPalette.amber)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(RoundedRectangle(cornerRadius: 10).fill(WidgetPalette.amber.opacity(0.1)))
            }
            ForEach(PolishingProvider.allCases, id: \.self) { provider in
                row(provider)
            }
        }
        .onAppear(perform: detect)
    }

    private func detect() {
        var result: [PolishingProvider: String] = [:]
        if let path = settings.resolvedClaudeExecutable { result[.claudeCode] = path }
        if let path = settings.resolvedCodexExecutable { result[.codex] = path }
        found = result
        if settings.polishing != .off, result[settings.polishing] == nil {
            settings.polishing = result[.claudeCode] != nil ? .claudeCode : result[.codex] != nil ? .codex : .off
        }
    }

    private func row(_ provider: PolishingProvider) -> some View {
        let available = provider == .off || found[provider] != nil
        let selected = settings.polishing == provider
        return Button { if available { settings.polishing = provider } } label: {
            HStack(alignment: .top, spacing: 12) {
                ZStack {
                    Circle().strokeBorder(selected ? WidgetPalette.text : Color.white.opacity(0.35), lineWidth: 1.5)
                    if selected {
                        Circle().fill(WidgetPalette.text)
                        Circle().fill(Color.black).frame(width: 6, height: 6)
                    }
                }
                .frame(width: 16, height: 16)
                .padding(.top, 2)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        Text(provider.displayName).font(.system(size: 13, weight: .medium))
                        if provider != .off {
                            HStack(spacing: 5) {
                                Circle().fill(available ? WidgetPalette.ok : Color.white.opacity(0.25)).frame(width: 7, height: 7)
                                Text(available ? "Found" : "Not installed")
                            }
                            .font(.system(size: 11.5))
                            .foregroundStyle(.secondary)
                        }
                    }
                    Text(detail(provider, available: available))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 13).padding(.vertical, 11)
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.04)))
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(Color.white.opacity(selected ? 0.5 : 0.09), lineWidth: selected ? 1.5 : 1)
            )
            .opacity(available ? 1 : 0.75)
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
        .disabled(!available)
    }

    private func detail(_ provider: PolishingProvider, available: Bool) -> String {
        switch provider {
        case .claudeCode:
            return available ? "Uses your Claude subscription. \(found[.claudeCode] ?? "")"
                : "Install Claude Code from claude.ai/code, then run claude once to sign in."
        case .codex:
            return available ? "Uses your ChatGPT plan. \(found[.codex] ?? "")"
                : "Install with npm i -g @openai/codex, then run codex once to sign in."
        case .off:
            return "Insert what you said. Spacing, capitals and dictionary terms are still fixed."
        }
    }
}

/// Off-white primary button that sits on black glass.
struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(Color(red: 0.08, green: 0.08, blue: 0.09))
            .padding(.horizontal, 14).padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: 7).fill(configuration.isPressed ? Color.white.opacity(0.8) : WidgetPalette.text))
    }
}

struct QuietButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13))
            .foregroundStyle(WidgetPalette.text.opacity(configuration.isPressed ? 0.5 : 0.7))
            .padding(.horizontal, 10).padding(.vertical, 6)
    }
}
