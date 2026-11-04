#!/usr/bin/env bash
# Builds the macOS app (universal), signs it with a Developer ID, notarizes and staples it.
# Run on a Mac from a logged-in session (codesign needs the unlocked login keychain).
#
#   APPLE_SIGNING_IDENTITY="Developer ID Application: Name (TEAMID)" \
#   NOTARY_KEY_ID=ABC123 scripts/release-macos.sh
#
# Notarizes with an App Store Connect API key: ~/.appstoreconnect/private_keys/AuthKey_<id>.p8
# and its issuer id in ~/.appstoreconnect/issuer_id.
# Output: target/universal-apple-darwin/release/bundle/dmg/BeaconSync_<version>_universal.dmg
set -euo pipefail
: "${APPLE_SIGNING_IDENTITY:?set APPLE_SIGNING_IDENTITY}"
: "${NOTARY_KEY_ID:?set NOTARY_KEY_ID}"
key=~/.appstoreconnect/private_keys/AuthKey_$NOTARY_KEY_ID.p8
issuer=$(cat ~/.appstoreconnect/issuer_id)
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

(cd apps/desktop && pnpm install --frozen-lockfile && pnpm tauri build --target universal-apple-darwin --bundles app,dmg)

bundle=target/universal-apple-darwin/release/bundle
dmg=$(ls "$bundle"/dmg/*.dmg | head -1) # Tauri already signed the app and the dmg
xcrun notarytool submit "$dmg" --key "$key" --key-id "$NOTARY_KEY_ID" --issuer "$issuer" --wait
xcrun stapler staple "$dmg"
spctl --assess --type open --context context:primary-signature -v "$dmg"
echo "$dmg"
