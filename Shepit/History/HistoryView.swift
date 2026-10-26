import AppKit
import SwiftUI
import ShepitCore

struct HistoryView: View {
    let history: HistoryStore?
    @State private var records: [DictationRecord] = []
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("\(records.count) dictations").foregroundStyle(.secondary)
                Spacer()
                Button("Refresh") { reload() }
            }
            .padding(10)
            Divider()
            if let error {
                Text(error).foregroundStyle(.red).padding()
            }
            List(records) { r in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(r.createdAt, style: .time).font(.caption).foregroundStyle(.secondary)
                        Text(r.appName ?? r.bundleID ?? "?").font(.caption).bold()
                        Text("\(r.profile) · \(r.mode) · \(r.formatterSource)").font(.caption).foregroundStyle(.secondary)
                        Text("stt \(r.sttMs) ms · llm \(r.llmMs) ms").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(r.formattedText, forType: .string)
                        } label: { Image(systemName: "doc.on.doc") }
                        .buttonStyle(.borderless)
                    }
                    Text(r.rawText).font(.callout).foregroundStyle(.secondary).lineLimit(3)
                    Text(r.formattedText).font(.body).textSelection(.enabled)
                }
                .padding(.vertical, 4)
            }
        }
        .frame(minWidth: 620, minHeight: 420)
        .onAppear { reload() }
    }

    private func reload() {
        guard let history else {
            error = "History store is unavailable"
            return
        }
        do {
            records = try history.recent(limit: 200)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

@MainActor
final class HistoryWindowController: NSWindowController {
    init(history: HistoryStore?) {
        let hosting = NSHostingController(rootView: HistoryView(history: history))
        let window = NSWindow(contentViewController: hosting)
        window.title = "Shepit History"
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        window.isReleasedWhenClosed = false
        window.setContentSize(NSSize(width: 760, height: 520))
        window.center()
        super.init(window: window)
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    func show() {
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}
