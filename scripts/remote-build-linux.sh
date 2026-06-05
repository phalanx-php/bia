#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIA_ROOT="$(dirname "$DIR")"
REMOTE_HOST="${BIA_REMOTE_BUILD_HOST:?Set BIA_REMOTE_BUILD_HOST to the SSH host for the Linux build machine.}"
REMOTE_DIR="${BIA_REMOTE_BUILD_DIR:?Set BIA_REMOTE_BUILD_DIR to the remote Bia build directory.}"

echo "=== 0. Building embedded runtime locally ==="
# Ensure framework changes (Phalanx, bia-runtime) are bundled into the embedded tarball
php "$BIA_ROOT/scripts/build-embed.php"

echo "=== 1. Syncing local source to $REMOTE_HOST ==="
# We exclude build artifacts and cached directories so we don't overwrite the server's cache
rsync -avz \
  --exclude 'target' \
  --exclude '.ripht' \
  --exclude '.spc' \
  --exclude '.spc-work' \
  --exclude '.git' \
  "$BIA_ROOT/" "$REMOTE_HOST:$REMOTE_DIR/"

echo "=== 2. Running remote build ==="
ssh "$REMOTE_HOST" "
  set -euo pipefail
  cd $REMOTE_DIR

  # Ensure Cargo is in PATH
  source ~/.cargo/env

  # Calculate a hash of the files that dictate the C/PHP compilation environment
  CURRENT_HASH=\$(md5sum craft.yml scripts/build-static-engine.sh | md5sum | awk '{print \$1}')

  if [ -f .spc-hash ] && [ \"\$(cat .spc-hash)\" = \"\$CURRENT_HASH\" ]; then
    echo '=> PHP Configuration unchanged. Skipping 15-minute StaticPHP compilation!'
  else
    echo '=> PHP Configuration (craft.yml or build-static-engine.sh) changed.'
    echo '=> Running full StaticPHP build...'

    # We leave clean-build whatever it is set to in craft.yml to ensure correctness on changes
    ./scripts/build-static-engine.sh

    # Save the hash so we can skip this next time
    echo \"\$CURRENT_HASH\" > .spc-hash
  fi

  # Build the Rust binary (Cargo inherently knows what Rust files changed)
  echo '=> Running cargo build...'
  export RIPHT_PHP_SAPI_PREFIX=\"\$PWD/.ripht/php\"
  cargo build --release
"

echo "=== 3. Pulling compiled binary back to local ==="
rsync -avz "$REMOTE_HOST:$REMOTE_DIR/target/release/bia" "$BIA_ROOT/bia-linux-x86_64"

echo "=== Success ==="
echo "Linux binary available at: $BIA_ROOT/bia-linux-x86_64"
