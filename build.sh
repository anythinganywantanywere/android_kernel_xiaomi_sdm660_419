#!/usr/bin/env bash
#
# buildstart.sh - local kernel build for whyred (sdm660, 4.19)
# Ported from the "Kernel Builder" GitHub Actions workflow.
#
# Needs a TG_BOT_TOKEN and TG_CHAT_ID (env vars or a .env file next to this
# script) if you want the zip sent to Telegram. Don't commit .env.
#
# Usage:
#   ./buildstart.sh                 thin LTO, vendor/whyred-perf_defconfig
#   ./buildstart.sh -l full         full LTO
#   ./buildstart.sh -l none -j 8    no LTO, 8 jobs
#   ./buildstart.sh -b <branch>     checkout a branch first
#   ./buildstart.sh -c              wipe out/ before building
#   ./buildstart.sh --no-tg         skip Telegram
#   ./buildstart.sh -h              this help

set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ -f "$SCRIPT_DIR/.env" ] && . "$SCRIPT_DIR/.env"

# --- config ------------------------------------------------------------------
REPO_URL="https://github.com/anythinganywantanywere/android_kernel_xiaomi_sdm660_419"
KERNEL_DIR="${KERNEL_DIR:-/root/android_kernel_xiaomi_sdm660_419}"
ANYKERNEL_DIR="${ANYKERNEL_DIR:-$KERNEL_DIR/AnyKernel3}"

BRANCH=""                       # empty = whatever is checked out now
DEFCONFIG="vendor/whyred-perf_defconfig"
LTO_MODE="thin"                 # thin | full | none
JOBS="$(nproc --all)"

CLANG_TAG="r547379"
CLANG_URL="https://android.googlesource.com/platform/prebuilts/clang/host/linux-x86"

BUILD_USER="anythinganywantanywere"
BUILD_HOST="sienna"
EXTRA_CFLAGS="-march=armv8-a -mno-outline"

# set this to fake the build timestamp, leave empty to use the real one
BUILD_TIMESTAMP="Sun June 21 16:02:52 WIB 2026"

CCACHE_SIZE="5G"

WORKDIR="${WORKDIR:-$HOME/kernel-build}"
TC_DIR="$WORKDIR/toolchain/aosp-clang"
OUT_DIR="$KERNEL_DIR/out"
RESULT_DIR="$WORKDIR/result"
LOG_FILE="$WORKDIR/build.log"

TG_BOT_TOKEN="${TG_BOT_TOKEN:-}"
TG_CHAT_ID="${TG_CHAT_ID:-}"
SEND_TG=1
CLEAN=0

export TZ="Asia/Jakarta"

# --- helpers -----------------------------------------------------------------
log()  { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m[!] %s\033[0m\n' "$*" >&2; }

tg_ready() { [ "$SEND_TG" -eq 1 ] && [ -n "$TG_BOT_TOKEN" ] && [ -n "$TG_CHAT_ID" ]; }

tg_message() {
  tg_ready || return 0
  curl -s --max-time 30 "https://api.telegram.org/bot${TG_BOT_TOKEN}/sendMessage" \
    -d chat_id="$TG_CHAT_ID" --data-urlencode text="$1" >/dev/null \
    || warn "couldn't send telegram message"
}

# tg_file <file> <caption>
tg_file() {
  tg_ready || return 0
  local size resp
  size=$(stat -c %s "$1")
  # bot api caps uploads at 50 MB
  if [ "$size" -gt 52428800 ]; then
    warn "$(basename "$1") is over 50MB, telegram won't take it. skipping."
    return 0
  fi
  resp=$(curl -s --max-time 600 \
    "https://api.telegram.org/bot${TG_BOT_TOKEN}/sendDocument" \
    -F chat_id="$TG_CHAT_ID" \
    -F document=@"$1" \
    -F caption="$2") || { warn "telegram upload failed"; return 0; }
  if echo "$resp" | grep -q '"ok":true'; then
    echo "sent to telegram: $(basename "$1")"
  else
    warn "telegram rejected the upload: $resp"
  fi
}

die() {
  printf '\033[1;31m[x] %s\033[0m\n' "$*" >&2
  tg_message "Kernel build FAILED: $*"
  [ -f "$LOG_FILE" ] && tg_file "$LOG_FILE" "build.log (failed)"
  trap - ERR
  exit 1
}

usage() {
  # print the comment block at the top of this file
  sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

trap 'die "error on line $LINENO (exit $?)"' ERR

# --- args --------------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    -d) DEFCONFIG="$2"; shift 2 ;;
    -l) LTO_MODE="$2";  shift 2 ;;
    -b) BRANCH="$2";    shift 2 ;;
    -j) JOBS="$2";      shift 2 ;;
    -c) CLEAN=1;        shift ;;
    --no-tg) SEND_TG=0; shift ;;
    -h|--help) usage ;;
    *) echo "unknown option: $1 (try -h)" >&2; exit 1 ;;
  esac
done

case "$LTO_MODE" in
  thin|full|none) ;;
  *) echo "LTO must be thin, full or none" >&2; exit 1 ;;
esac

mkdir -p "$WORKDIR" "$RESULT_DIR"
: > "$LOG_FILE"

if [ "$SEND_TG" -eq 1 ] && ! tg_ready; then
  warn "TG_BOT_TOKEN / TG_CHAT_ID not set, nothing will go to telegram."
fi

# --- 1. dependencies ---------------------------------------------------------
log "Checking dependencies"
MISSING=()
for bin in git curl tar make bc bison flex ccache zip python3 \
           aarch64-linux-gnu-gcc arm-linux-gnueabi-gcc; do
  command -v "$bin" >/dev/null 2>&1 || MISSING+=("$bin")
done
if [ ${#MISSING[@]} -gt 0 ]; then
  warn "missing: ${MISSING[*]}"
  echo "On Debian/Ubuntu:"
  echo "  sudo apt-get install -y bc bison flex libssl-dev libelf-dev build-essential \\"
  echo "    zip curl git python3 python-is-python3 ccache llvm lld \\"
  echo "    gcc-aarch64-linux-gnu gcc-arm-linux-gnueabi"
  exit 1
fi

# --- 2. source ---------------------------------------------------------------
log "Preparing kernel source: $KERNEL_DIR"
if [ -d "$KERNEL_DIR/.git" ]; then
  [ -n "$BRANCH" ] && git -C "$KERNEL_DIR" checkout "$BRANCH"
  echo "using local source on $(git -C "$KERNEL_DIR" rev-parse --abbrev-ref HEAD) (no auto pull)"
else
  git clone --depth=1 ${BRANCH:+-b "$BRANCH"} "$REPO_URL" "$KERNEL_DIR"
fi
[ -f "$KERNEL_DIR/arch/arm64/configs/$DEFCONFIG" ] || die "defconfig not found: $DEFCONFIG"
[ -d "$ANYKERNEL_DIR" ] || die "AnyKernel3 not found at: $ANYKERNEL_DIR"

# --- 3. toolchain ------------------------------------------------------------
log "Getting AOSP clang $CLANG_TAG"
if [ -x "$TC_DIR/bin/clang" ]; then
  echo "already there, skipping download"
else
  mkdir -p "$TC_DIR"
  curl -L --fail \
    "${CLANG_URL}/+archive/refs/heads/main/clang-${CLANG_TAG}.tar.gz" \
    -o /tmp/clang.tar.gz
  tar -xzf /tmp/clang.tar.gz -C "$TC_DIR"
  rm -f /tmp/clang.tar.gz
fi
export PATH="$TC_DIR/bin:$PATH"
which clang
CLANG_VER="$(clang --version | head -n1)"
echo "$CLANG_VER"

# --- 4. ccache ---------------------------------------------------------------
log "Setting up ccache"
export CCACHE_DIR="${CCACHE_DIR:-$HOME/.ccache}"
ccache -M "$CCACHE_SIZE" >/dev/null
ccache -z >/dev/null

# --- 5. build ----------------------------------------------------------------
cd "$KERNEL_DIR"
if [ "$CLEAN" -eq 1 ]; then
  log "Cleaning out/"
  rm -rf "$OUT_DIR"
fi
mkdir -p "$OUT_DIR"

MAKE_ARGS=(
  ARCH=arm64
  CC="ccache clang"
  HOSTCC="ccache clang"
  HOSTCXX="ccache clang++"
  LD=ld.lld
  AR=llvm-ar
  NM=llvm-nm
  OBJCOPY=llvm-objcopy
  OBJDUMP=llvm-objdump
  STRIP=llvm-strip
  READELF=llvm-readelf
  OBJSIZE=llvm-size
  LLVM=1
  LLVM_IAS=1
  KBUILD_BUILD_USER="$BUILD_USER"
  KBUILD_BUILD_HOST="$BUILD_HOST"
  CLANG_TRIPLE=aarch64-linux-gnu-
  CROSS_COMPILE=aarch64-linux-gnu-
  CROSS_COMPILE_ARM32=arm-linux-gnueabi-
  KCFLAGS="$EXTRA_CFLAGS"
)
[ "$LTO_MODE" != "none" ] && MAKE_ARGS+=(LTO="$LTO_MODE")
[ -n "$BUILD_TIMESTAMP" ] && MAKE_ARGS+=(KBUILD_BUILD_TIMESTAMP="$BUILD_TIMESTAMP")

log "Building: defconfig=$DEFCONFIG LTO=$LTO_MODE jobs=$JOBS"
tg_message "Kernel build started
Defconfig: $DEFCONFIG
LTO: $LTO_MODE
Branch: $(git rev-parse --abbrev-ref HEAD)"
START_TIME=$(date +%s)

make O="$OUT_DIR" "${MAKE_ARGS[@]}" "$DEFCONFIG" 2>&1 | tee -a "$LOG_FILE"
make -j"$JOBS" O="$OUT_DIR" "${MAKE_ARGS[@]}" 2>&1 | tee -a "$LOG_FILE"

ccache -s | tee -a "$LOG_FILE"

DURATION=$(( $(date +%s) - START_TIME ))
DUR_TXT="$((DURATION / 60))m $((DURATION % 60))s"
echo "compile time: $DUR_TXT"

# --- 6. check the result -----------------------------------------------------
log "Checking build output"
BOOT_DIR="$OUT_DIR/arch/arm64/boot"
if   [ -f "$BOOT_DIR/Image.gz-dtb" ]; then KIMG="Image.gz-dtb"
elif [ -f "$BOOT_DIR/Image.gz" ];     then KIMG="Image.gz"
else die "build failed, no Image.gz or Image.gz-dtb found"
fi
echo "build ok ($KIMG)"

# --- 7. package with AnyKernel3 ----------------------------------------------
log "Packaging"
BRANCH_NAME="$(git -C "$KERNEL_DIR" rev-parse --abbrev-ref HEAD)"
BRANCH_SAFE="${BRANCH_NAME//\//-}"
COMMIT="$(git -C "$KERNEL_DIR" rev-parse --short HEAD)"
ZIP_NAME="kernel-4.19-${BRANCH_SAFE}-$(date +'%d%m%Y-%H%M').zip"
ZIP_PATH="$RESULT_DIR/$ZIP_NAME"

# work in a temp copy so the AnyKernel3 dir stays clean
PKG_DIR="$(mktemp -d)"
trap 'rm -rf "$PKG_DIR"' EXIT
cp -a "$ANYKERNEL_DIR"/. "$PKG_DIR"/
rm -rf "$PKG_DIR/.git" "$PKG_DIR"/*.zip
cp -v "$BOOT_DIR/$KIMG" "$PKG_DIR/$KIMG"

( cd "$PKG_DIR" && zip -r9 "$ZIP_PATH" . -x ".git/*" "README.md" ".github/*" )
cp "$LOG_FILE" "$RESULT_DIR/${ZIP_NAME%.zip}.log"
echo "zip: $ZIP_PATH ($(du -h "$ZIP_PATH" | cut -f1))"

# --- 8. telegram -------------------------------------------------------------
if tg_ready; then
  log "Sending to telegram"
  CAPTION="Kernel whyred 4.19 done
Branch: $BRANCH_NAME ($COMMIT)
Defconfig: $DEFCONFIG
LTO: $LTO_MODE
Build time: $DUR_TXT
Clang: $CLANG_VER
MD5: $(md5sum "$ZIP_PATH" | cut -d' ' -f1)"
  tg_file "$ZIP_PATH" "$CAPTION"
fi

log "Done: $ZIP_PATH"