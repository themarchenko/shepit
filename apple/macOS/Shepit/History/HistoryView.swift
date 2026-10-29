import AppKit
import SwiftUI
import ShepitCore

struct HistoryView: View {
    let history: HistoryStore?
    /// Called after a clear so the app can drop in-memory copies of the deleted text.
    var onCleared: () -> Void = {}
    @State private var records: [DictationRecord] = []
    /// Total rows in the store. `records` stops at 200, so it cannot say how much a clear deletes.
    @State private var total = 0
    @State private var confirmingClear = false
    @State private var clearing = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("\(records.count) dictations").foregroundStyle(.secondary)
                Spacer()
                Button("Clear History…") { askToClear() }
                    .disabled(total == 0 || clearing)
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
        .alert("Clear history?", isPresented: $confirmingClear) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) { clear() }
        } message: {
            Text("Delete all \(total) dictations and their recordings? This cannot be undone.")
        }
    }

    /// Reads the count now so the alert names what a Delete would remove at this moment.
    private func askToClear() {
        guard let history else { return }
        do {
            total = try history.count()
            confirmingClear = true
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func clear() {
        guard let history, !clearing else { return }
        clearing = true
        // VACUUM and the file removals can take a while, so they stay off the main thread.
        Task {
            let firstError: String? = await Task.detached {
                // Rows first, then files, even when the rows failed: a partial clear should still remove what it can.
                var firstError: Error?
                do { try history.clear() } catch { firstError = error }
                do { try RecordingStore.clear(in: AppPaths.recordingsDirectory) } catch { firstError = firstError ?? error }
                return firstError?.localizedDescription
            }.value
            reload()
            if let firstError { error = "Could not clear history: \(firstError)" }
            clearing = false
            onCleared()
        }
    }

    private func reload() {
        guard let history else {
            error = "History store is unavailable"
            return
        }
        do {
            records = try history.recent(limit: 200)
            total = try history.count()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

@MainActor
final class HistoryWindowController: NSWindowController {
    init(history: HistoryStore?, onCleared: @escaping () -> Void = {}) {
        let hosting = NSHostingController(rootView: HistoryView(history: history, onCleared: onCleared))
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
