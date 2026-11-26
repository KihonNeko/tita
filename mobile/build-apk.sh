#!/usr/bin/env bash
# Builds the Android app: target/apk/tita.apk
#
# Needs the Android SDK (build-tools and one platform) and NDK, a JDK, and the
# Rust targets for the ABIs built:
#   rustup target add aarch64-linux-android armv7-linux-androideabi
#
# Environment:
#   ANDROID_HOME         SDK root (required)
#   ANDROID_NDK_HOME     NDK root (default: the newest one in the SDK)
#   ABIS                 ABIs to build (default: "arm64-v8a armeabi-v7a")
#   VERSION_CODE         integer that must grow with every release (default: 1)
#   ANDROID_KEYSTORE     keystore to sign with; ANDROID_KEYSTORE_PASSWORD and
#                        ANDROID_KEY_ALIAS go with it. Without one, a
#                        throwaway key is created in target/apk: fine for
#                        trying the app, but Android refuses to update an
#                        installed app signed with a different key.
set -euo pipefail
cd "$(dirname "$0")/.."

SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:?set ANDROID_HOME to the Android SDK}}
NDK=${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-$(ls -d "$SDK"/ndk/* | sort -V | tail -1)}}
BUILD_TOOLS=$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)
PLATFORM=$(ls -d "$SDK"/platforms/android-* | sort -V | tail -1)
case "$(uname -s)" in
  Darwin) HOST=darwin-x86_64 ;;
  *) HOST=linux-x86_64 ;;
esac
TOOLCHAIN=$NDK/toolchains/llvm/prebuilt/$HOST/bin
ABIS=${ABIS:-"arm64-v8a armeabi-v7a"}
MIN_SDK=26
TARGET_SDK=34
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' mobile/Cargo.toml | head -1)
VERSION_CODE=${VERSION_CODE:-1}
OUT=target/apk

echo "SDK $SDK, NDK $NDK, $(basename "$BUILD_TOOLS"), $(basename "$PLATFORM")"
rm -rf "$OUT/lib" "$OUT"/*.apk "$OUT/res.zip"
mkdir -p "$OUT"

for abi in $ABIS; do
  case "$abi" in
    arm64-v8a) triple=aarch64-linux-android; clang=aarch64-linux-android ;;
    armeabi-v7a) triple=armv7-linux-androideabi; clang=armv7a-linux-androideabi ;;
    x86_64) triple=x86_64-linux-android; clang=x86_64-linux-android ;;
    *) echo "unknown ABI: $abi" >&2; exit 1 ;;
  esac
  cc=$TOOLCHAIN/$clang$MIN_SDK-clang
  upper=$(echo "$triple" | tr 'a-z-' 'A-Z_')
  lower=$(echo "$triple" | tr '-' '_')
  env "CARGO_TARGET_${upper}_LINKER=$cc" "CC_$lower=$cc" "AR_$lower=$TOOLCHAIN/llvm-ar" \
    cargo build -p tita-mobile --lib --release --target "$triple"
  mkdir -p "$OUT/lib/$abi"
  "$TOOLCHAIN/llvm-strip" -o "$OUT/lib/$abi/libtita_mobile.so" "target/$triple/release/libtita_mobile.so"
done

"$BUILD_TOOLS/aapt2" compile --dir mobile/android/res -o "$OUT/res.zip"
"$BUILD_TOOLS/aapt2" link -o "$OUT/unaligned.apk" \
  --manifest mobile/android/AndroidManifest.xml -I "$PLATFORM/android.jar" \
  --min-sdk-version $MIN_SDK --target-sdk-version $TARGET_SDK \
  --version-code "$VERSION_CODE" --version-name "$VERSION" \
  "$OUT/res.zip"
(cd "$OUT" && zip -q -r unaligned.apk lib)
"$BUILD_TOOLS/zipalign" -f 4 "$OUT/unaligned.apk" "$OUT/aligned.apk"

if [ -z "${ANDROID_KEYSTORE:-}" ]; then
  ANDROID_KEYSTORE=$OUT/throwaway.keystore
  ANDROID_KEYSTORE_PASSWORD=android
  ANDROID_KEY_ALIAS=tita
  if [ ! -f "$ANDROID_KEYSTORE" ]; then
    keytool -genkeypair -keystore "$ANDROID_KEYSTORE" -storepass android -keypass android \
      -alias tita -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=tita throwaway key"
  fi
  echo "warning: signed with a throwaway key (see the top of this script)" >&2
fi
export ANDROID_KEYSTORE_PASSWORD
"$BUILD_TOOLS/apksigner" sign --ks "$ANDROID_KEYSTORE" --ks-pass env:ANDROID_KEYSTORE_PASSWORD \
  --ks-key-alias "${ANDROID_KEY_ALIAS:-tita}" --out "$OUT/tita.apk" "$OUT/aligned.apk"
"$BUILD_TOOLS/apksigner" verify --print-certs "$OUT/tita.apk" | head -3
rm -f "$OUT/unaligned.apk" "$OUT/aligned.apk" "$OUT/tita.apk.idsig"
ls -l "$OUT/tita.apk"
