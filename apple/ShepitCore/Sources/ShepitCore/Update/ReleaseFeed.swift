import Foundation

/// Dotted version numbers like "0.2.1", compared number by number.
public enum AppVersion {
    public static var current: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
    }

    public static func isNewer(_ a: String, than b: String) -> Bool {
        let x = numbers(a), y = numbers(b)
        for i in 0..<max(x.count, y.count) {
            let l = i < x.count ? x[i] : 0, r = i < y.count ? y[i] : 0
            if l != r { return l > r }
        }
        return false
    }

    private static func numbers(_ version: String) -> [Int] {
        version.trimmingCharacters(in: CharacterSet(charactersIn: "vV")).split(separator: ".").map { Int($0) ?? 0 }
    }
}

/// A published release of Shepit on GitHub.
public struct AppRelease: Equatable, Sendable {
    public let version: String
    public let pageURL: URL
    public let diskImageURL: URL
    /// Bytes, for the download progress.
    public let diskImageSize: Int

    public init(version: String, pageURL: URL, diskImageURL: URL, diskImageSize: Int) {
        self.version = version
        self.pageURL = pageURL
        self.diskImageURL = diskImageURL
        self.diskImageSize = diskImageSize
    }
}

/// The latest release, read from the GitHub API. Drafts and pre-releases never show up there.
public enum ReleaseFeed {
    public static let latestURL = URL(string: "https://api.github.com/repos/themarchenko/shepit/releases/latest")!
    public static let releasesPage = URL(string: "https://github.com/themarchenko/shepit/releases/latest")!
    static let diskImageName = "Shepit.dmg"

    public enum FeedError: LocalizedError {
        case http(Int)
        case noDiskImage(String)
        case insecureDiskImage(String)

        public var errorDescription: String? {
            switch self {
            case .http(403), .http(429): return "GitHub is rate limiting update checks. Try again in an hour."
            case .http(let code): return "GitHub answered with HTTP \(code)."
            case .noDiskImage(let version): return "Release \(version) has no \(ReleaseFeed.diskImageName)."
            case .insecureDiskImage(let version): return "Release \(version) points at a download that is not https."
            }
        }
    }

    public static func latest(session: URLSession = .shared) async throws -> AppRelease {
        var request = URLRequest(url: latestURL, timeoutInterval: 20)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("Shepit/\(AppVersion.current)", forHTTPHeaderField: "User-Agent")
        let (data, response) = try await session.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard status == 200 else { throw FeedError.http(status) }
        return try parse(data)
    }

    static func parse(_ data: Data) throws -> AppRelease {
        struct Payload: Decodable {
            let tag_name: String
            let html_url: URL
            let assets: [Asset]
        }
        struct Asset: Decodable {
            let name: String
            let browser_download_url: URL
            let size: Int
        }
        let payload = try JSONDecoder().decode(Payload.self, from: data)
        let version = payload.tag_name.trimmingCharacters(in: CharacterSet(charactersIn: "vV"))
        guard let image = payload.assets.first(where: { $0.name == diskImageName }) else {
            throw FeedError.noDiskImage(version)
        }
        // The signature check is the real boundary; refusing plain http is defence in depth.
        guard image.browser_download_url.scheme?.lowercased() == "https" else {
            throw FeedError.insecureDiskImage(version)
        }
        // The page opens in the browser from "What changed"; anything but https gets the releases list.
        let page = payload.html_url.scheme?.lowercased() == "https" ? payload.html_url : releasesPage
        return AppRelease(version: version, pageURL: page, diskImageURL: image.browser_download_url,
                          diskImageSize: image.size)
    }
}
