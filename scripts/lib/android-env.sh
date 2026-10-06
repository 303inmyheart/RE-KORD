#!/usr/bin/env bash
# Checks shared by the Android commands (init, build). Meant to be used with `source`.
# Every message says what is missing and what to install: the Android chain is long
# (SDK, NDK, JDK, Rust targets) and a Gradle error halfway through a build doesn't explain it.

RK_ANDROID_DOCS="docs/ANDROID.md"

rk_die() {
  echo "ERROR: $*" >&2
  echo "See $RK_ANDROID_DOCS" >&2
  exit 1
}

rk_android_sdk() {
  local candidates=(
    "${ANDROID_HOME:-}"
    "${ANDROID_SDK_ROOT:-}"
    "$HOME/Android/Sdk"
    "$HOME/Library/Android/sdk"
    "/usr/lib/android-sdk"
  )
  local dir
  for dir in "${candidates[@]}"; do
    if [[ -n "$dir" && -d "$dir/platforms" ]]; then
      export ANDROID_HOME="$dir"
      export ANDROID_SDK_ROOT="$dir"
      echo "SDK:  $dir"
      return 0
    fi
  done
  rk_die "Android SDK not found. Install the command line tools and export ANDROID_HOME."
}

rk_android_ndk() {
  if [[ -n "${NDK_HOME:-}" && -d "$NDK_HOME" ]]; then
    echo "NDK:  $NDK_HOME (from NDK_HOME)"
    return 0
  fi
  # Several versions installed: take the highest, sorting numerically
  # (alphabetically, 9 would beat 28).
  local newest
  newest="$(ls -1 "$ANDROID_HOME/ndk" 2>/dev/null | sort -V | tail -1 || true)"
  if [[ -n "$newest" ]]; then
    export NDK_HOME="$ANDROID_HOME/ndk/$newest"
    echo "NDK:  $NDK_HOME"
    return 0
  fi
  if [[ -d "$ANDROID_HOME/ndk-bundle" ]]; then
    export NDK_HOME="$ANDROID_HOME/ndk-bundle"
    echo "NDK:  $NDK_HOME"
    return 0
  fi
  rk_die "NDK not found. Install it with: sdkmanager 'ndk;28.2.13676358'"
}

rk_android_jdk() {
  if [[ -z "${JAVA_HOME:-}" ]]; then
    local dir
    for dir in /usr/lib/jvm/java-21-openjdk-* /usr/lib/jvm/java-17-openjdk-*; do
      if [[ -x "$dir/bin/javac" ]]; then
        export JAVA_HOME="$dir"
        break
      fi
    done
  fi
  local javac="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
  command -v "$javac" >/dev/null 2>&1 || rk_die "JDK not found (17 or newer required). On Ubuntu: apt install openjdk-17-jdk"
  local major
  major="$("$javac" -version 2>&1 | sed -E 's/^javac ([0-9]+).*/\1/')"
  [[ "$major" =~ ^[0-9]+$ ]] || rk_die "Unreadable JDK version: $("$javac" -version 2>&1)"
  (( major >= 17 )) || rk_die "JDK $major is too old: Gradle 8 and AGP require at least 17."
  echo "JDK:  ${JAVA_HOME:-system} (javac $major)"
}

# The Rust targets are needed for the native library inside the APK: without them, cargo
# fails mid-build. They are added automatically; the command is idempotent.
rk_android_rust_targets() {
  command -v rustup >/dev/null 2>&1 || rk_die "rustup not found: it is needed for the Rust Android targets."
  local installed missing=()
  installed="$(rustup target list --installed)"
  local target
  for target in "$@"; do
    grep -qx "$target" <<<"$installed" || missing+=("$target")
  done
  if (( ${#missing[@]} )); then
    echo "==> Adding missing Rust targets: ${missing[*]}"
    rustup target add "${missing[@]}"
  fi
}

rk_android_preflight() {
  echo "==> Toolchain Android"
  rk_android_sdk
  rk_android_ndk
  rk_android_jdk
  command -v pnpm >/dev/null 2>&1 || rk_die "pnpm not found: corepack enable or npm i -g pnpm"
  rk_android_rust_targets "$@"
}
