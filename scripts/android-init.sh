#!/usr/bin/env bash
# Prepares the Android client build. With the native project already under version
# control (apps/client-shell/src-tauri/gen/android) there is almost nothing to generate
# here: it checks the toolchain and recreates the project if it was deleted.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/android-env.sh"

ANDROID_DIR="apps/client-shell/src-tauri/gen/android"

echo "RE-KORD Android client setup"
rk_android_preflight aarch64-linux-android

echo "==> Dependencies and UI"
pnpm install
pnpm --filter @rekord/client-ui build

if [[ -d "$ANDROID_DIR" ]]; then
  echo "==> Native project: already present in $ANDROID_DIR (version-controlled, leaving it alone)"
else
  # `tauri android init` writes the project from scratch: you only get here if it was
  # deleted, and the result must be compared with git because the CLI also overwrites
  # our changes (cleartext traffic to the hub, release signing, audio service).
  echo "==> Native project missing: regenerating it with tauri android init"
  pnpm --filter @rekord/client-shell exec tauri android init
  echo
  echo "Now review the differences: git diff -- $ANDROID_DIR"
  echo "The RE-KORD changes to the native project are described in $RK_ANDROID_DOCS"
fi

echo
echo "Done. APK: ./scripts/android-build.sh --install"
