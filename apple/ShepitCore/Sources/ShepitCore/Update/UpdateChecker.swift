import Foundation

/// Asks GitHub about once a day whether a newer release is out, and installs it when the user agrees.
@MainActor
public final class UpdateChecker: ObservableObject {
    public enum State: Equatable {
        case idle
        case checking
        case upToDate
        case available(AppRelease)
        case downloading(AppRelease, Double)
        case installing(AppRelease)
        case failed(String, AppRelease?)
    }

    @Published public private(set) var state: State = .idle
    /// A check found a release worth showing: one the user asked about, or one they have not skipped.
    public var onFound: (AppRelease) -> Void = { _ in }

    private let settings: AppSettings
    private var timer: Timer?
    private static let interval: TimeInterval = 24 * 3600

    public init(settings: AppSettings) {
        self.settings = settings
    }

    /// Looks shortly after launch, then every hour, and checks when the last check is a day old. An
    /// hourly look instead of a 24-hour timer copes with a Mac that sleeps through the night.
    public func start() {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 3600, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.checkIfDue() }
        }
        Task {
            try? await Task.sleep(for: .seconds(20))
            checkIfDue()
        }
    }

    public var isBusy: Bool {
        switch state {
        case .checking, .downloading, .installing: return true
        default: return false
        }
    }

    private func checkIfDue() {
        guard settings.checkForUpdates, Date().timeIntervalSince(settings.lastUpdateCheck) >= Self.interval else { return }
        Task { await check(userInitiated: false) }
    }

    public func check(userInitiated: Bool) async {
        guard !isBusy else { return }
        state = .checking
        do {
            let release = try await ReleaseFeed.latest()
            settings.lastUpdateCheck = Date()
            guard AppVersion.isNewer(release.version, than: AppVersion.current) else {
                state = .upToDate
                return
            }
            state = .available(release)
            Log.update.info("release \(release.version, privacy: .public) is available")
            if userInitiated || release.version != settings.skippedUpdate { onFound(release) }
        } catch {
            Log.update.error("update check failed: \(error.localizedDescription, privacy: .public)")
            // A failed background check stays silent; the next one comes in an hour.
            state = userInitiated ? .failed(error.localizedDescription, nil) : .idle
        }
    }

    public func skip(_ release: AppRelease) {
        settings.skippedUpdate = release.version
    }

    /// Downloads and installs `release`, then relaunches. On failure the state says why.
    public func install(_ release: AppRelease) async {
        guard !isBusy else { return }
        state = .downloading(release, 0)
        do {
            let app = try await UpdateInstaller.install(release) { phase in
                Task { @MainActor [weak self] in self?.apply(phase, release) }
            }
            UpdateInstaller.relaunch(app)
        } catch {
            Log.update.error("update failed: \(error.localizedDescription, privacy: .public)")
            state = .failed(error.localizedDescription, release)
        }
    }

    /// Progress arrives through separate tasks, so a late download report must not undo "installing".
    private func apply(_ phase: UpdateInstaller.Phase, _ release: AppRelease) {
        switch (phase, state) {
        case (.downloading(let fraction), .downloading): state = .downloading(release, fraction)
        case (.installing, .downloading): state = .installing(release)
        default: break
        }
    }
}
