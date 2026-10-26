# Shepit build helpers. Requires Xcode 26 and XcodeGen (brew install xcodegen).
DERIVED := .build/DerivedData
APP := $(DERIVED)/Build/Products/Release/Shepit.app
VERSION := $(shell sed -n 's/.*CFBundleShortVersionString: "\(.*\)"/\1/p' project.yml)
DIST := .build/dist

.PHONY: generate build install package run test bench clean logs

generate:
	xcodegen generate --spec project.yml

build: generate
	@set -o pipefail; xcodebuild -project Shepit.xcodeproj -scheme Shepit -configuration Release \
	  -derivedDataPath $(DERIVED) -skipPackagePluginValidation -skipMacroValidation build 2>&1 \
	  | { grep -E 'error:|warning: .*Shepit/|BUILD (SUCCEEDED|FAILED)' || true; }
	@echo "built $(APP)"

install: build
	rm -rf /Applications/Shepit.app
	ditto "$(APP)" /Applications/Shepit.app
	@echo "installed /Applications/Shepit.app"

# The zip attached to a GitHub release.
package: build
	rm -rf $(DIST) && mkdir -p $(DIST)
	ditto -c -k --keepParent "$(APP)" "$(DIST)/Shepit-$(VERSION).zip"
	@echo "packaged $(DIST)/Shepit-$(VERSION).zip"

run:
	open /Applications/Shepit.app

test:
	swift test --package-path Packages/ShepitCore

bench:
	spike/bench.sh

logs:
	log stream --predicate 'subsystem == "com.themarchenko.shepit"' --style compact --level debug

clean:
	rm -rf .build Shepit.xcodeproj Packages/ShepitCore/.build
