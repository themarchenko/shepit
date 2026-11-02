import Foundation

/// Which language the package's own strings use. The app's language is whatever macOS picked for the main
/// bundle (the AppleLanguages override included); the package follows it only if it ships that language.
enum CoreLocalization {
    /// Tests set this to look at one language without relaunching.
    nonisolated(unsafe) static var override: String?
    /// Tests only. SwiftPM's default build copies the catalog without compiling it, so tests compile it
    /// themselves and point lookups at the result. Production always reads `Bundle.module`.
    nonisolated(unsafe) static var overrideBundle: Bundle?

    static var language: String {
        if let override { return override }
        guard let first = Bundle.main.preferredLocalizations.first,
              Bundle.main.localizations.contains(first),
              Bundle.module.localizations.contains(first) else { return "en" }
        return first
    }
}

/// Looks `value` up in the package catalog in the current core language.
func L(_ value: String.LocalizationValue) -> String {
    String(localized: LocalizedStringResource(value, table: nil, locale: Locale(identifier: CoreLocalization.language),
                                              bundle: .atURL((CoreLocalization.overrideBundle ?? Bundle.module).bundleURL)))
}
