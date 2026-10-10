#!/bin/sh
# muguet - make-ios.sh
# Constantin Clerc (c22dev) - 0.1

set -eu
cd "$(dirname "$0")"

ZIP=""
MODE=""
DEVICE=""
for a in "$@"; do
  case "$a" in
    *.zip|*.wasm) ZIP="$a" ;;
    --install|--ipa) MODE="$a" ;;
    *) DEVICE="$a" ;;
  esac
done

TEAM="${TEAM:-}"
if [ "$MODE" != "--ipa" ] && [ -z "$TEAM" ]; then echo "set TEAM=<your Apple developer team id>"; exit 1; fi
BUNDLE_ID="${BUNDLE_ID:-ch.cclerc.muguet.ios}"
ENTITLEMENTS=Muguet.entitlements
HEAP_GB=8
if [ "${FREE:-0}" = 1 ] || [ "$MODE" = "--ipa" ]; then
  ENTITLEMENTS=Muguet-free.entitlements
  HEAP_GB=5
fi
export MUGUET_IOS_HEAP_GB="${MUGUET_IOS_HEAP_GB:-$HEAP_GB}"
TOOLCHAIN=1.98.1
# Intel migration: ARM Homebrew lives in /opt/homebrew, Intel in /usr/local.
# Prefer existing rustup on PATH, then each known location, then $HOME/.cargo.
if command -v rustup >/dev/null 2>&1; then :;
elif [ -x "/opt/homebrew/opt/rustup/bin/rustup" ]; then export PATH="/opt/homebrew/opt/rustup/bin:$PATH";
elif [ -x "/usr/local/opt/rustup/bin/rustup" ]; then export PATH="/usr/local/opt/rustup/bin:$PATH";
elif [ -d "$HOME/.cargo/bin" ]; then export PATH="$HOME/.cargo/bin:$PATH"; fi
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home --remap-path-prefix=$PWD=/muguet"

if [ -n "$ZIP" ]; then
  cargo build --release
  mkdir -p target/ios-game
  case "$ZIP" in
    *.zip)
      ENTRY="$(unzip -Z1 "$ZIP" | grep -E '(^|/)b/[^/]+/game\.wasm$' | head -1)"
      [ -n "$ENTRY" ] || { echo "$ZIP has no b/<build>/game.wasm"; exit 1; }
      unzip -p "$ZIP" "$ENTRY" > target/ios-game/game.wasm ;;
    *) cp "$ZIP" target/ios-game/game.wasm ;;
  esac
  ./target/release/muguet --precompile-ios target/ios-game/game.wasm target/ios-game/game.cwasm
  rm target/ios-game/game.wasm
  export MUGUET_GAME_CODE="$PWD/target/ios-game/game.cwasm"
else
  echo "no game zip given: building without the game's code"
  unset MUGUET_GAME_CODE || true
fi

IPHONEOS_DEPLOYMENT_TARGET=15.0 cargo "+$TOOLCHAIN" build --release --no-default-features --target aarch64-apple-ios --target-dir target/ios

(cd ios && xcodegen generate --quiet)
rm -rf target/ios-xcode/Build/Products
APP="$PWD/target/ios-xcode/Build/Products/Release-iphoneos/Muguet.app"
if [ "$MODE" = "--ipa" ]; then
  xcodebuild -quiet -project ios/Muguet.xcodeproj -scheme Muguet -configuration Release \
    -destination 'generic/platform=iOS' -derivedDataPath target/ios-xcode build \
    PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" CODE_SIGNING_ALLOWED=NO
  rm -f "$APP/embedded.mobileprovision"
  codesign --force --sign - --entitlements ios/Muguet-sideload.entitlements --generate-entitlement-der "$APP"
else
  xcodebuild -quiet -project ios/Muguet.xcodeproj -scheme Muguet -configuration Release \
    -destination 'generic/platform=iOS' -derivedDataPath target/ios-xcode -allowProvisioningUpdates build \
    PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" CODE_SIGN_ENTITLEMENTS="$ENTITLEMENTS" DEVELOPMENT_TEAM="$TEAM"
fi
codesign --verify "$APP"
echo "built: $APP ($(du -sh "$APP" | cut -f1))"

if [ "$MODE" = "--ipa" ]; then
  STAGE="$(mktemp -d)"
  mkdir "$STAGE/Payload"
  cp -cRp "$APP" "$STAGE/Payload/"
  mkdir -p build
  IPA="$PWD/build/Muguet.ipa"
  rm -f "$IPA"
  (cd "$STAGE" && zip -qry "$IPA" Payload)
  rm -rf "$STAGE"
  echo "built: $IPA ($(du -h "$IPA" | cut -f1))"
fi

if [ "$MODE" = "--install" ]; then
  if [ -z "$DEVICE" ]; then
    DEVICE="$(xcrun devicectl list devices 2>/dev/null | awk '/available \(paired\)|connected/ && /iPhone|iPad/ {for (i=1;i<=NF;i++) if ($i ~ /^[0-9A-F]{8}-[0-9A-F]{16}$/) {print $i; exit}}')"
  fi
  [ -n "$DEVICE" ] || { echo "no iPhone / iPad connected (unlocked, Developer Mode on)"; exit 1; }
  xcrun devicectl device install app --device "$DEVICE" "$APP"
  echo "installed on $DEVICE"
fi
