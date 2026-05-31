#!/usr/bin/env bash
set -euo pipefail

# Dory Static Runtime Builder
# Orchestrates StaticPHP v3 to compile libphp.a from craft.yml.

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$DIR")"

DORY_STATIC_PHP_PREFIX="${DORY_STATIC_PHP_PREFIX:-$ROOT_DIR/.ripht/php}"
SPC_WORK_DIR="${SPC_WORK_DIR:-$ROOT_DIR/.spc-work}"
SPC_BIN="${DORY_SPC_BIN:-}"
SPC_VERSION="${SPC_VERSION:-nightly}"
SPC_BASE_URL="${SPC_BASE_URL:-https://dl.static-php.dev/v3/spc-bin/$SPC_VERSION}"

echo "=== Dory Static Runtime Builder ==="
echo "Reading configuration from $ROOT_DIR/craft.yml"

case "$(uname -s)" in
    Darwin) SPC_OS="macos" ;;
    Linux) SPC_OS="linux" ;;
    *) echo "Unsupported OS for SPC v3 binary: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    arm64|aarch64) SPC_ARCH="aarch64" ;;
    x86_64|amd64) SPC_ARCH="x86_64" ;;
    *) echo "Unsupported architecture for SPC v3 binary: $(uname -m)" >&2; exit 1 ;;
esac

if [ -z "$SPC_BIN" ]; then
    mkdir -p "$ROOT_DIR/.spc"
    SPC_BIN="$ROOT_DIR/.spc/spc-$SPC_OS-$SPC_ARCH"

    if [ ! -x "$SPC_BIN" ]; then
        echo "Downloading StaticPHP v3 SPC: $SPC_OS-$SPC_ARCH"
        curl -#fSL "$SPC_BASE_URL/spc-$SPC_OS-$SPC_ARCH" -o "$SPC_BIN"
        chmod +x "$SPC_BIN"
    fi
fi

mkdir -p "$SPC_WORK_DIR"
cd "$SPC_WORK_DIR"

echo "=== Phase 1: StaticPHP v3 toolchain ==="
"$SPC_BIN" --version
"$SPC_BIN" install-pkg --no-interaction --dl-parallel=8 --dl-retry=5 --dl-prefer-binary=true -- pkg-config

echo "=== Phase 2: StaticPHP v3 craft ==="
"$SPC_BIN" craft --no-interaction "$ROOT_DIR/craft.yml"

echo "=== Phase 3: Install static PHP runtime ==="
mkdir -p "$DORY_STATIC_PHP_PREFIX/lib" "$DORY_STATIC_PHP_PREFIX/include"

cp "buildroot/lib/libphp.a" "$DORY_STATIC_PHP_PREFIX/lib/libphp.a"

rm -rf "$DORY_STATIC_PHP_PREFIX/include/php"
cp -r "buildroot/include/php" "$DORY_STATIC_PHP_PREFIX/include/php"

for src in buildroot/lib/*.a; do
    if [ -f "$src" ]; then
        cp "$src" "$DORY_STATIC_PHP_PREFIX/lib/"
    fi
done

echo "=== Success ==="
echo "libphp.a installed to: $DORY_STATIC_PHP_PREFIX/lib/libphp.a"
ls -lh "$DORY_STATIC_PHP_PREFIX/lib/libphp.a"
