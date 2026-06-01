#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DORY_ROOT="$(dirname "$DIR")"

echo "=== 0. Building embedded runtime locally ==="
php "$DORY_ROOT/scripts/build-embed.php"

cd "$DORY_ROOT"

# Determine MD5 command for macOS vs Linux
if command -v md5 >/dev/null 2>&1; then
    # macOS
    MD5_CMD="md5 -q"
elif command -v md5sum >/dev/null 2>&1; then
    # Linux
    MD5_CMD="md5sum"
else
    echo "No md5sum or md5 command found."
    exit 1
fi

# Calculate a hash of the files that dictate the C/PHP compilation environment
CURRENT_HASH=$(cat craft.yml scripts/build-static-engine.sh | eval "$MD5_CMD" | awk '{print $1}')

if [ -f .spc-hash ] && [ "$(cat .spc-hash)" = "$CURRENT_HASH" ]; then
    echo "=> PHP Configuration unchanged. Skipping StaticPHP compilation!"
else
    echo "=> PHP Configuration (craft.yml or build-static-engine.sh) changed or missing cache."
    echo "=> Running StaticPHP build..."

    craft_backup="$(mktemp)"
    cp craft.yml "$craft_backup"
    trap 'cp "$craft_backup" craft.yml; rm -f "$craft_backup"' EXIT

    # Temporarily disable clean-build to utilize SPC cache
    if [ "$(uname -s)" = "Darwin" ]; then
        sed -i '' 's/clean-build: true/clean-build: false/g' craft.yml
    else
        sed -i 's/clean-build: true/clean-build: false/g' craft.yml
    fi

    # Build StaticPHP
    ./scripts/build-static-engine.sh

    # Save the hash so we can skip this next time
    echo "$CURRENT_HASH" > .spc-hash

    # Restore craft.yml so the working tree isn't left dirty.
    cp "$craft_backup" craft.yml
    rm -f "$craft_backup"
    trap - EXIT
fi

echo "=> Running cargo build..."
cargo build

echo "=== Success ==="
echo "Local binary available at: $DORY_ROOT/target/debug/dory"
