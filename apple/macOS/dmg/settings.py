# dmgbuild settings for the release disk image. `make package` runs it from the repository root:
#
#   dmgbuild -s apple/macOS/dmg/settings.py -D app=path/to/Shepit.app Shepit out.dmg
#
# Icon positions are icon centers in points from the window's top-left corner. The arrow and text in
# dmg/background.png are drawn around them by scripts/make-dmg-background.swift; change both together.

app = defines["app"]  # noqa: F821, dmgbuild provides `defines`

format = "ULFO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = app + "/Contents/Resources/AppIcon.icns"

background = "apple/macOS/dmg/background.png"
window_rect = ((200, 140), (660, 420))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False

icon_size = 112
text_size = 13
icon_locations = {
    "Shepit.app": (170, 170),
    "Applications": (490, 170),
}
