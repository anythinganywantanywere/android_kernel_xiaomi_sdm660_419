#!/usr/bin/env bash
# =============================================================================
# buildstart.sh - Build kernel whyred (SDM660, 4.19) secara lokal
# Turunan dari workflow GitHub Actions "Kernel Builder" (build.yml)
#
# Source    : /root/android_kernel_xiaomi_sdm660_419 (clone dari
#             https://github.com/user-why-red/android_kernel_xiaomi_sdm660_419
#             jika folder belum ada)
# Toolchain : AOSP Clang r547379
# Packaging : AnyKernel3 dari dalam source -> zip -> kirim ke Telegram
#
# Telegram (isi lewat environment variable atau file .env di samping script):
#   TG_BOT_TOKEN="123456:ABC..."
#   TG_CHAT_ID="-100123456789"
#
# Pemakaian:
#   ./buildstart.sh                      # default: thin LTO, vendor/whyred-perf_defconfig
#   ./buildstart.sh -l full              # LTO full
#   ./buildstart.sh -l none -j 8         # tanpa LTO, 8 job
#   ./buildstart.sh -b <branch>          # pilih branch source
#   ./buildstart.sh -c                   # clean out/ dulu (build bersih)
#   ./buildstart.sh --no-tg              # jangan kirim ke Telegram
#   ./buildstart.sh -h                   # bantuan
# =============================================================================

set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Muat token/chat id dari .env jika ada (jangan di-commit ke git!)
[ -f "$SCRIPT_DIR/.env" ] && . "$SCRIPT_DIR/.env"

# ----------------------------- KONFIGURASI -----------------------------------
REPO_URL="https://github.com/user-why-red/android_kernel_xiaomi_sdm660_419"
KERNEL_DIR="${KERNEL_DIR:-/root/android_kernel_xiaomi_sdm660_419}"
ANYKERNEL_DIR="${ANYKERNEL_DIR:-$KERNEL_DIR/AnyKernel3}"

BRANCH=""                                   # kosong = branch yang sedang aktif
DEFCONFIG="vendor/whyred-perf_defconfig"
LTO_MODE="thin"                             # thin | full | none
JOBS="$(nproc --all)"

AOSP_CLANG_TAG="r547379"
AOSP_CLANG_URL="https://android.googlesource.com/platform/prebuilts/clang/host/linux-x86"

BUILD_USER="anythinganywantanywere"
BUILD_HOST="sienna"
CUSTOM_CFLAGS="-march=armv8-a -mno-outline"

# Isi manual jika ingin timestamp build custom, contoh:
# BUILD_TIMESTAMP="Sun June 21 16:02:52 WIB 2026"
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

# ----------------------------- HELPER ----------------------------------------
log()  { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m[!] %s\033[0m\n' "$*" >&2; }

tg_ready() { [ "$SEND_TG" -eq 1 ] && [ -n "$TG_BOT_TOKEN" ] && [ -n "$TG_CHAT_ID" ]; }

tg_message() {
  tg_ready || return 0
  curl -s --max-time 30 "https://api.telegram.org/bot${TG_BOT_TOKEN}/sendMessage" \
    -d chat_id="$TG_CHAT_ID" --data-urlencode text="$1" >/dev/null || warn "Gagal kirim pesan Telegram"
}

tg_file() { # $1=file  $2=caption
  tg_ready || return 0
  local size; size=$(stat -c %s "$1")
  if [ "$size" -gt 52428800 ]; then
    warn "File $(basename "$1") > 50MB, melebihi batas Bot API Telegram. Tidak dikirim."
    return 0
  fi
  local resp
  resp=$(curl -s --max-time 600 \
    "https://api.telegram.org/bot${TG_BOT_TOKEN}/sendDocument" \
    -F chat_id="$TG_CHAT_ID" \
    -F document=@"$1" \
    -F caption="$2") || { warn "Gagal upload ke Telegram"; return 0; }
  if echo "$resp" | grep -q '"ok":true'; then
    echo "Terkirim ke Telegram: $(basename "$1")"
  else
    warn "Telegram menolak upload: $resp"
  fi
}

die() {
  printf '\033[1;31m[x] %s\033[0m\n' "$*" >&2
  tg_message "❌ Build kernel GAGAL: $*"
  [ -f "$LOG_FILE" ] && tg_file "$LOG_FILE" "build.log (gagal)"
  trap - ERR
  exit 1
}

usage() {
  sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

trap 'die "Error di baris $LINENO (exit $?)"' ERR

# ----------------------------- ARGUMEN ---------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    -d) DEFCONFIG="$2"; shift 2 ;;
    -l) LTO_MODE="$2"; shift 2 ;;
    -b) BRANCH="$2"; shift 2 ;;
    -j) JOBS="$2"; shift 2 ;;
    -c) CLEAN=1; shift ;;
    --no-tg) SEND_TG=0; shift ;;
    -h|--help) usage ;;
    *) echo "Opsi tidak dikenal: $1 (pakai -h)" >&2; exit 1 ;;
  esac
done

case "$LTO_MODE" in thin|full|none) ;; *) echo "LTO harus thin/full/none" >&2; exit 1 ;; esac

mkdir -p "$WORKDIR" "$RESULT_DIR"
: > "$LOG_FILE"

if [ "$SEND_TG" -eq 1 ] && ! tg_ready; then
  warn "TG_BOT_TOKEN / TG_CHAT_ID belum di-set -> hasil tidak akan dikirim ke Telegram."
fi

# ----------------------------- 1. DEPENDENSI ---------------------------------
log "Cek dependensi"
MISSING=()
for bin in git curl tar make bc bison flex ccache zip python3 \
           aarch64-linux-gnu-gcc arm-linux-gnueabi-gcc; do
  command -v "$bin" >/dev/null 2>&1 || MISSING+=("$bin")
done
if [ ${#MISSING[@]} -gt 0 ]; then
  warn "Tools belum ada: ${MISSING[*]}"
  echo "Install (Debian/Ubuntu):"
  echo "  sudo apt-get install -y bc bison flex libssl-dev libelf-dev build-essential \\"
  echo "    zip curl git python3 python-is-python3 ccache llvm lld \\"
  echo "    gcc-aarch64-linux-gnu gcc-arm-linux-gnueabi"
  exit 1
fi

# ----------------------------- 2. SOURCE -------------------------------------
log "Siapkan source kernel: $KERNEL_DIR"
if [ -d "$KERNEL_DIR/.git" ]; then
  if [ -n "$BRANCH" ]; then
    git -C "$KERNEL_DIR" checkout "$BRANCH"
  fi
  echo "Pakai source lokal (branch: $(git -C "$KERNEL_DIR" rev-parse --abbrev-ref HEAD)), tidak di-pull otomatis."
else
  git clone --depth=1 ${BRANCH:+-b "$BRANCH"} "$REPO_URL" "$KERNEL_DIR"
fi
[ -f "$KERNEL_DIR/arch/arm64/configs/$DEFCONFIG" ] || die "Defconfig tidak ditemukan: $DEFCONFIG"
[ -d "$ANYKERNEL_DIR" ] || die "AnyKernel3 tidak ditemukan di: $ANYKERNEL_DIR"

# ----------------------------- 3. TOOLCHAIN ----------------------------------
log "Siapkan AOSP Clang ${AOSP_CLANG_TAG}"
if [ -x "$TC_DIR/bin/clang" ]; then
  echo "Toolchain sudah ada, skip download."
else
  mkdir -p "$TC_DIR"
  curl -L --fail \
    "${AOSP_CLANG_URL}/+archive/refs/heads/main/clang-${AOSP_CLANG_TAG}.tar.gz" \
    -o /tmp/clang.tar.gz
  tar -xzf /tmp/clang.tar.gz -C "$TC_DIR"
  rm -f /tmp/clang.tar.gz
fi
export PATH="$TC_DIR/bin:$PATH"
which clang
CLANG_VER="$(clang --version | head -n1)"
echo "$CLANG_VER"

# ----------------------------- 4. CCACHE -------------------------------------
log "Setup ccache"
export CCACHE_DIR="${CCACHE_DIR:-$HOME/.ccache}"
ccache -M "$CCACHE_SIZE" >/dev/null
ccache -z >/dev/null

# ----------------------------- 5. BUILD --------------------------------------
cd "$KERNEL_DIR"
[ "$CLEAN" -eq 1 ] && { log "Clean out/"; rm -rf "$OUT_DIR"; }
mkdir -p "$OUT_DIR"

TOOLCHAIN_ARGS=(
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
  KCFLAGS="$CUSTOM_CFLAGS"
)
[ "$LTO_MODE" != "none" ] && TOOLCHAIN_ARGS+=(LTO="$LTO_MODE")
[ -n "$BUILD_TIMESTAMP" ] && TOOLCHAIN_ARGS+=(KBUILD_BUILD_TIMESTAMP="$BUILD_TIMESTAMP")

log "Build: defconfig=$DEFCONFIG | LTO=$LTO_MODE | jobs=$JOBS"
tg_message "🔨 Build kernel dimulai
Defconfig: $DEFCONFIG
LTO: $LTO_MODE
Branch: $(git rev-parse --abbrev-ref HEAD)"
START_TIME=$(date +%s)

make O="$OUT_DIR" "${TOOLCHAIN_ARGS[@]}" "$DEFCONFIG" 2>&1 | tee -a "$LOG_FILE"
make -j"$JOBS" O="$OUT_DIR" "${TOOLCHAIN_ARGS[@]}" 2>&1 | tee -a "$LOG_FILE"

ccache -s | tee -a "$LOG_FILE"

DURATION=$(( $(date +%s) - START_TIME ))
DUR_TXT="$((DURATION / 60)) menit $((DURATION % 60)) detik"
echo "-----------------------------------------------------"
echo "Waktu total kompilasi: $DUR_TXT"
echo "-----------------------------------------------------"

# ----------------------------- 6. VERIFIKASI ---------------------------------
log "Verifikasi hasil build"
BOOT_DIR="$OUT_DIR/arch/arm64/boot"
if   [ -f "$BOOT_DIR/Image.gz-dtb" ]; then KIMG="Image.gz-dtb"
elif [ -f "$BOOT_DIR/Image.gz" ];     then KIMG="Image.gz"
else die "BUILD GAGAL - Image.gz / Image.gz-dtb tidak ditemukan"
fi
echo "BUILD BERHASIL ($KIMG)"

# ----------------------------- 7. PACKAGING (AnyKernel3) ---------------------
log "Packaging dengan AnyKernel3"
BRANCH_NAME="$(git -C "$KERNEL_DIR" rev-parse --abbrev-ref HEAD)"
BRANCH_SAFE="${BRANCH_NAME//\//-}"
COMMIT="$(git -C "$KERNEL_DIR" rev-parse --short HEAD)"
ZIP_NAME="kernel-4.19-${BRANCH_SAFE}-$(date +'%d%m%Y-%H%M').zip"
ZIP_PATH="$RESULT_DIR/$ZIP_NAME"

# Salin AnyKernel3 ke folder sementara agar source tidak kotor
PKG_DIR="$(mktemp -d)"
trap 'rm -rf "$PKG_DIR"' EXIT
cp -a "$ANYKERNEL_DIR"/. "$PKG_DIR"/
rm -rf "$PKG_DIR/.git" "$PKG_DIR"/*.zip
cp -v "$BOOT_DIR/$KIMG" "$PKG_DIR/$KIMG"

( cd "$PKG_DIR" && zip -r9 "$ZIP_PATH" . -x ".git/*" "README.md" ".github/*" )
cp "$LOG_FILE" "$RESULT_DIR/${ZIP_NAME%.zip}.log"
echo "Zip: $ZIP_PATH ($(du -h "$ZIP_PATH" | cut -f1))"

# ----------------------------- 8. KIRIM KE TELEGRAM --------------------------
if tg_ready; then
  log "Kirim ke Telegram"
  CAPTION="✅ Kernel whyred 4.19 selesai
Branch: $BRANCH_NAME ($COMMIT)
Defconfig: $DEFCONFIG
LTO: $LTO_MODE
Waktu build: $DUR_TXT
Clang: $CLANG_VER
MD5: $(md5sum "$ZIP_PATH" | cut -d' ' -f1)"
  tg_file "$ZIP_PATH" "$CAPTION"
fi

log "Selesai. Hasil: $ZIP_PATH"
