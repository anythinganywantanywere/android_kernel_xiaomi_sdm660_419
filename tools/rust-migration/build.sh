#!/usr/bin/env bash
set -euo pipefail

DEFCONFIG="${1:-vendor/whyred-perf_defconfig}"
OUT="${2:-out-fase0}"
NDK_BIN="${NDK_BIN:-$HOME/workspace/android-ndk/android-ndk-r27d/toolchains/llvm/prebuilt/linux-x86_64/bin}"
LOGDIR="docs/rust-migration/logs"
STAMP="$(date +%F-%H%M)"
mkdir -p "$LOGDIR"

export PATH="$NDK_BIN:$HOME/.cargo/bin:$PATH"
export ARCH=arm64 SUBARCH=arm64

MAKE_ARGS=(
  O="$OUT"
  ARCH=arm64
  LLVM=1
  CROSS_COMPILE=aarch64-linux-gnu-
  CROSS_COMPILE_ARM32=arm-linux-gnueabi-
)

make "${MAKE_ARGS[@]}" "$DEFCONFIG" 2>&1 | tee "$LOGDIR/defconfig-$STAMP.log"
make "${MAKE_ARGS[@]}" -j"$(nproc)" 2>&1 | tee "$LOGDIR/build-$STAMP.log"

echo "warnings: $(grep -c 'warning:' "$LOGDIR/build-$STAMP.log" || true)"
echo "errors:   $(grep -c 'error:'   "$LOGDIR/build-$STAMP.log" || true)"
