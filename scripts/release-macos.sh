#!/usr/bin/env bash
# Builds the macOS app (universal), signs it with a Developer ID, notarizes and staples it.
# Run on a Mac from a logged-in session (codesign needs the unlocked login keychain).
#
#   APPLE_SIGNING_IDENTITY="Developer ID Application: Name (TEAMID)" \
#   NOTARY_PROFILE=beaconsync scripts/release-macos.sh
#
# NOTARY_PROFILE is a `xcrun notarytool store-credentials` keychain profile.
# Output: target/universal-apple-darwin/release/bundle/dmg/BeaconSync_<version>_universal.dmg
set -euo pipefail
: "${APPLE_SIGNING_IDENTITY:?set APPLE_SIGNING_IDENTITY}"
: "${NOTARY_PROFILE:?set NOTARY_PROFILE}"
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

(cd apps/desktop && pnpm install --frozen-lockfile && pnpm tauri build --target universal-apple-darwin --bundles app,dmg)

bundle=target/universal-apple-darwin/release/bundle
dmg=$(ls "$bundle"/dmg/*.dmg | head -1)
codesign --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dmg"
xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$dmg"
xcrun stapler staple "$bundle/macos/BeaconSync.app"
spctl --assess --type open --context context:primary-signature -v "$dmg"
echo "$dmg"
