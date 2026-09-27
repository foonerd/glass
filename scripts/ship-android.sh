#!/bin/bash
# Build the Android app of the display: SDL2 for Android from the SDL
# source (pinned by version and checksum), the display as libmain.so for
# each ABI with cargo-ndk, and the app around them with Gradle, signed
# with the keystore the environment names, else with a throwaway key for
# a build nobody publishes. The result is dist/glass-<version>-android.apk.
#
# Runs where the Android SDK (with the NDK 27, build tools 35 and platform
# 35), a JDK, Gradle, the Android Rust targets and cargo-ndk are installed,
# such as the image scripts/android/Dockerfile describes:
#
#   docker build -t glass-android scripts/android
#   docker run --rm -v "$PWD:/glass" -w /glass -e CARGO_TARGET_DIR=/glass/target/android glass-android scripts/ship-android.sh
#
# (A target directory of its own keeps the container's build scripts apart
# from the host's, which were linked against another glibc.)
#
# Signing, for a published build:
#   ANDROID_KEYSTORE        path of the keystore (a .jks)
#   ANDROID_KEYSTORE_PASS   its password
#   ANDROID_KEY_ALIAS       the key's alias (default glass)

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
APP=$ROOT/remote/android/app

SDL_VERSION=2.32.10
SDL_SHA256=5f5993c530f084535c65a6879e9b26ad441169b3e25d789d83287040a9ca5165
# The SDL source lives beside the app, not under target/: the CI cache walks
# target/ and trips over folders in the source named target and trybuild.
SDL_HOME=$ROOT/remote/android/.sdl
SDL_SRC=$SDL_HOME/SDL2-$SDL_VERSION
ABIS="arm64-v8a armeabi-v7a x86_64"
PLATFORM=24

: "${ANDROID_HOME:?ANDROID_HOME must name the Android SDK}"
NDK=${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/27.* 2>/dev/null | sort -V | tail -n1)}
[ -x "$NDK/ndk-build" ] || { echo "ship-android: no NDK at $NDK" >&2; exit 1; }
BUILD_TOOLS=$(ls -d "$ANDROID_HOME"/build-tools/35.* 2>/dev/null | sort -V | tail -n1)
[ -x "$BUILD_TOOLS/apksigner" ] || { echo "ship-android: no build tools 35 under $ANDROID_HOME" >&2; exit 1; }
command -v cargo-ndk >/dev/null 2>&1 || cargo ndk --version >/dev/null 2>&1 || { echo "ship-android: cargo-ndk is missing (cargo install cargo-ndk)" >&2; exit 1; }
command -v gradle >/dev/null 2>&1 || { echo "ship-android: gradle is missing" >&2; exit 1; }
export ANDROID_NDK_HOME=$NDK

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
# 0.7.20 -> 720, one code per version, rising with it.
major=${VERSION%%.*}
rest=${VERSION#*.}
minor=${rest%%.*}
patch=${rest#*.}
export GLASS_VERSION=$VERSION
export GLASS_VERSION_CODE=$((major * 100000 + minor * 1000 + patch))

# SDL's source: the Android build of libSDL2.so and the Java side of the
# activity both come from it.
if [ ! -f "$SDL_SRC/Android.mk" ]; then
  mkdir -p "$SDL_HOME"
  tarball=$SDL_HOME/SDL2-$SDL_VERSION.tar.gz
  curl -fsSL -o "$tarball" "https://github.com/libsdl-org/SDL/releases/download/release-$SDL_VERSION/SDL2-$SDL_VERSION.tar.gz"
  echo "$SDL_SHA256  $tarball" | sha256sum -c - >/dev/null
  tar -C "$SDL_HOME" -xzf "$tarball"
fi
rm -rf "$APP/jni/SDL" && ln -s "$SDL_SRC" "$APP/jni/SDL"
rm -rf "$APP/src/main/java/org" && mkdir -p "$APP/src/main/java/org/libsdl/app"
cp "$SDL_SRC"/android-project/app/src/main/java/org/libsdl/app/*.java "$APP/src/main/java/org/libsdl/app/"

echo "ship-android: SDL2 for $ABIS"
JNI_LIBS=$APP/src/main/jniLibs
rm -rf "$JNI_LIBS" "$TARGET_DIR/android/obj"
"$NDK/ndk-build" -j"$(nproc)" \
  NDK_PROJECT_PATH=null \
  APP_BUILD_SCRIPT="$APP/jni/Android.mk" \
  NDK_APPLICATION_MK="$APP/jni/Application.mk" \
  NDK_OUT="$TARGET_DIR/android/obj" \
  NDK_LIBS_OUT="$JNI_LIBS" >/dev/null

echo "ship-android: the display for $ABIS"
# Each Rust target links against the libSDL2.so of its ABI.
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-L native=$JNI_LIBS/arm64-v8a"
export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS="-L native=$JNI_LIBS/armeabi-v7a"
export CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS="-L native=$JNI_LIBS/x86_64"
targets=""
for abi in $ABIS; do targets="$targets -t $abi"; done
# shellcheck disable=SC2086
cargo ndk $targets -P "$PLATFORM" -o "$JNI_LIBS" build --release --locked -p glass-android

echo "ship-android: the app"
(cd "$ROOT/remote/android" && gradle --quiet --no-daemon assembleRelease)
unsigned=$(ls "$APP"/build/outputs/apk/release/*.apk | head -n1)
[ -f "$unsigned" ] || { echo "ship-android: Gradle produced no apk" >&2; exit 1; }

mkdir -p dist
out=dist/glass-$VERSION-android.apk
aligned=$TARGET_DIR/android/aligned.apk
"$BUILD_TOOLS/zipalign" -f -p 4 "$unsigned" "$aligned"
if [ -n "${ANDROID_KEYSTORE:-}" ]; then
  echo "ship-android: signing with $ANDROID_KEYSTORE"
  "$BUILD_TOOLS/apksigner" sign --ks "$ANDROID_KEYSTORE" --ks-pass "pass:${ANDROID_KEYSTORE_PASS:?}" \
    --ks-key-alias "${ANDROID_KEY_ALIAS:-glass}" --out "$out" "$aligned"
else
  # A key of this build alone: the apk installs, and a later build signed
  # with the project's key does not update it. Not for a release.
  throwaway=$TARGET_DIR/android/throwaway.jks
  [ -f "$throwaway" ] || keytool -genkeypair -keystore "$throwaway" -storepass throwaway -alias glass \
    -keyalg RSA -keysize 2048 -validity 30 -dname "CN=Glass throwaway build" >/dev/null 2>&1
  echo "ship-android: no ANDROID_KEYSTORE, signing with a throwaway key"
  "$BUILD_TOOLS/apksigner" sign --ks "$throwaway" --ks-pass pass:throwaway --ks-key-alias glass --out "$out" "$aligned"
fi
"$BUILD_TOOLS/apksigner" verify --print-certs "$out" | sed 's/^/ship-android:   /' | head -n 3
echo "ship-android: payload"
ls -l "$out"
