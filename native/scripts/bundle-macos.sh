#!/bin/sh
set -eu

if [ "$(uname -s)" != Darwin ]; then
    echo 'This bundle script requires macOS.' >&2
    exit 1
fi

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cargo build --locked --manifest-path "$repo_dir/native/Cargo.toml"
bundle_dir="$repo_dir/out/VGit Preview.app"
mkdir -p "$bundle_dir/Contents/MacOS"
cp "$repo_dir/native/target/debug/vgit-native" "$bundle_dir/Contents/MacOS/vgit-native"
cat > "$bundle_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>VGit Preview</string>
<key>CFBundleDisplayName</key><string>VGit Preview</string>
<key>CFBundleIdentifier</key><string>dev.vgit.native-preview</string>
<key>CFBundleExecutable</key><string>vgit-native</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "Built $bundle_dir"
