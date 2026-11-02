# Interface language

macOS only. Windows lags: its UI stays English until a port reads this file. Everything the user sees on macOS has an English and a Ukrainian version. Prompts, schemas, logs, product names, the golden and the CalibrationScript and WidgetDemo sentences are not translated.

## Languages and the follow-system rule

- Two languages ship: English (`en`, the source) and Ukrainian (`uk`).
- Default: the app follows macOS. The first language in the system list that Shepit ships is used; if none matches, English. So a Ukrainian-first or Ukrainian-second system gets Ukrainian only when Ukrainian comes before any other shipped language (`InterfaceLanguage.target(system:)`, apple/ShepitCore/Sources/ShepitCore/Settings/AppSettings.swift:111).
- The language is fixed at launch. Changing it needs a restart.

## Override

- General > System > "App language": System, English, Ukrainian. Labels for the two languages are their own names in their own language (endonyms from `Locale`), the same in every interface language. System is localized ("Як у системі").
- The setting is not stored in `settings.json`. It is the `AppleLanguages` key of the app's own defaults domain (the bundle id), the same place System Settings > Language & Region > Applications writes. System removes the key, English writes `["en"]`, Ukrainian `["uk"]`. The two UIs therefore always agree.
- Reading uses `persistentDomain(forName:)` of the app domain, not `UserDefaults.standard`, because the latter merges the global list in and would make a system value look like an override. A first entry that is not shipped (for example `de`) reads as System and is never written back (AppSettings.swift:260).
- Caption "Applies after Shepit restarts." A "Restart Shepit" button shows when the language the app is running in differs from the language it would start in (`InterfaceLanguage.needsRestart(running:system:)`). It relaunches through the updater's relaunch (`UpdateInstaller.relaunch`).

## Catalogs and the English-source rule

- UI strings are written in English in the source. Translations live only in String Catalogs. No Cyrillic in Swift files outside the content and prompt files listed in `LocalizationCatalogTests`.
- Three catalogs: `apple/macOS/Shepit/Localizable.xcstrings` (app), `apple/macOS/Shepit/InfoPlist.xcstrings` (microphone prompt), `apple/ShepitCore/Sources/ShepitCore/Resources/Localizable.xcstrings` (package, read through `L()`).
- `make strings` adds new keys (state `new`) and marks removed ones stale, from the `.stringsdata` of the last build. A catalog with a key that has no finished Ukrainian value, or a stale key, fails `LocalizationCatalogTests`.
- Text a view builds from a `String` does not localize: use `String(localized:)`, or a `LocalizedStringKey` parameter on render helpers. Text that is not UI is `Text(verbatim:)`.
- Spec files that quote UI copy (doctor.md, widget.md, settings.md) quote the English source.

## Key caps in a sentence

A sentence with a key cap in it ("Hold [Right Option] to talk") is one catalog key with a placeholder, never fragments around the cap. The code builds `String(localized: "Hold \(KeyCapLine.slot) to talk")`; `KeyCapLine` splits on the slot and draws text, cap, text. Word order is then the translator's.

## Plurals

Every count that shows a noun is a plural key. English has one and other, Ukrainian one, few, many and other. Counts that are not quantities (Hz, HTTP codes) are interpolated as text so the locale adds no grouping space.

## Glossary (for a port)

| English | Ukrainian |
|---|---|
| dictation / dictate / dictation key | диктування / диктувати / клавіша диктування |
| hands-free | без рук |
| polishing / polish / polishing agent | шліфування / шліфувати / агент шліфування |
| recognizer / recognition / speech model | розпізнавач / розпізнавання / модель мовлення |
| speaker notes / About the speaker | нотатки про мовця / Про мовця |
| voice calibration | калібрування голосу |
| Doctor | Діагностика |
| widget | віджет |
| paste / Review before pasting | вставити / Перегляд перед вставленням |
| Settings / tabs | Параметри / Загальні, Вигляд, Шліфування, Словник, Про програму |
| System Settings / Privacy & Security / Accessibility / Microphone / Input Monitoring / Keyboard | Системні параметри / Приватність і безпека / Універсальний доступ / Мікрофон / Моніторинг введення / Клавіатура |
| Clean / Prompt / Raw | Чистовий / Промпт / Як є |
| Flow pill / Caption bar / Island | Пігулка / Смуга субтитрів / Острів |
| What's new / Welcome tour / History / Dictionary | Що нового / Знайомство / Історія / Словник |

Rules: apostrophe U+2019, quotes «», no em dashes, short wording in fixed-width spots. Banned stems: налаштуван, диктовк, полірув. Product names (Shepit, Claude Code, Codex, Parakeet, Whisper) stay Latin.
