#!/bin/sh
# Brings both String Catalogs in line with the source: new keys are added (state "new", no Ukrainian yet),
# keys no longer in the source are marked stale. Reads the .stringsdata files the last `make build` wrote,
# so run that first. The catalogs must keep the name Localizable.xcstrings: the file name is the table.
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd)
objs="$root/.build/DerivedData/Build/Intermediates.noindex"

sync() {
    catalog=$1
    target=$2
    dir="$objs/$target.build/Release/$target.build/Objects-normal/arm64"
    set -- "$dir"/*.stringsdata
    if [ ! -e "$1" ]; then
        echo "no .stringsdata for $target in $dir; run make build first" >&2
        exit 1
    fi
    xcrun xcstringstool sync "$catalog" --stringsdata "$@"
    echo "synced $catalog from $# files"
}

sync "$root/apple/macOS/Shepit/Localizable.xcstrings" Shepit
sync "$root/apple/ShepitCore/Sources/ShepitCore/Resources/Localizable.xcstrings" ShepitCore
