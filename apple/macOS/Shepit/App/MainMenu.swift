import AppKit

/// A main menu that is never drawn: Shepit is an accessory app. It exists so the standard Edit key equivalents
/// reach text fields in Settings and the tour, and Cmd-W closes the key window. No Quit item on purpose: the app
/// lives in the menu bar and quits from there.
enum MainMenu {
    @MainActor
    static func make() -> NSMenu {
        let main = NSMenu()
        let appItem = NSMenuItem()
        appItem.submenu = NSMenu(title: "Shepit")
        main.addItem(appItem)

        let edit = NSMenu(title: String(localized: "Edit"))
        edit.addItem(item(String(localized: "Undo"), Selector(("undo:")), "z"))
        edit.addItem(item(String(localized: "Redo"), Selector(("redo:")), "z", [.command, .shift]))
        edit.addItem(.separator())
        edit.addItem(item(String(localized: "Cut"), #selector(NSText.cut(_:)), "x"))
        edit.addItem(item(String(localized: "Copy"), #selector(NSText.copy(_:)), "c"))
        edit.addItem(item(String(localized: "Paste"), #selector(NSText.paste(_:)), "v"))
        edit.addItem(item(String(localized: "Select All"), #selector(NSText.selectAll(_:)), "a"))
        let editItem = NSMenuItem()
        editItem.submenu = edit
        main.addItem(editItem)

        let window = NSMenu(title: String(localized: "Window"))
        window.addItem(item(String(localized: "Close"), #selector(NSWindow.performClose(_:)), "w"))
        let windowItem = NSMenuItem()
        windowItem.submenu = window
        main.addItem(windowItem)
        return main
    }

    private static func item(_ title: String, _ action: Selector, _ key: String,
                             _ mask: NSEvent.ModifierFlags = .command) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = mask
        return item
    }
}
