# Shepit build helpers. The macOS app needs Xcode 26 and XcodeGen (brew install xcodegen).
MACOS := apple/macOS
CORE := apple/ShepitCore
DERIVED := .build/DerivedData
APP := $(DERIVED)/Build/Products/Release/Shepit.app
VERSION := $(shell sed -n 's/.*CFBundleShortVersionString: "\(.*\)"/\1/p' $(MACOS)/project.yml)
DIST := .build/dist
VENV := .build/venv
# Release builds are re-signed with this identity before packing (CI passes it; see release.yml).
SIGN_IDENTITY ?=

.PHONY: generate build install package run test bench clean logs

generate:
	xcodegen generate --spec $(MACOS)/project.yml

build: generate
	@set -o pipefail; xcodebuild -project $(MACOS)/Shepit.xcodeproj -scheme Shepit -configuration Release \
	  -derivedDataPath $(DERIVED) -skipPackagePluginValidation -skipMacroValidation build 2>&1 \
	  | { grep -E 'error:|warning: .*Shepit/|BUILD (SUCCEEDED|FAILED)' || true; }
	@echo "built $(APP)"

install: build
	rm -rf /Applications/Shepit.app
	ditto "$(APP)" /Applications/Shepit.app
	@echo "installed /Applications/Shepit.app"

# The disk image attached to a GitHub release. dmgbuild writes the Finder window layout itself, so this
# runs on a CI runner where scripting Finder would not. The background comes from
# apple/macOS/scripts/make-dmg-background.swift.
package: build $(VENV)/bin/dmgbuild
	rm -rf $(DIST) && mkdir -p $(DIST)
ifneq ($(SIGN_IDENTITY),)
	codesign --force --sign "$(SIGN_IDENTITY)" "$(APP)"
	codesign --verify --strict "$(APP)"
endif
	$(VENV)/bin/dmgbuild -s $(MACOS)/dmg/settings.py -D app="$(APP)" Shepit "$(DIST)/Shepit.dmg"
	@echo "packaged $(DIST)/Shepit.dmg ($(VERSION))"

$(VENV)/bin/dmgbuild:
	python3 -m venv $(VENV)
	$(VENV)/bin/pip install --quiet --disable-pip-version-check dmgbuild==1.6.5

run:
	open /Applications/Shepit.app

test:
	swift test --package-path $(CORE)

bench:
	apple/spike/bench.sh

logs:
	log stream --predicate 'subsystem == "com.themarchenko.shepit"' --style compact --level debug

clean:
	rm -rf .build $(MACOS)/Shepit.xcodeproj $(CORE)/.build
