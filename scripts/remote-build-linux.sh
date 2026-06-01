#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DORY_ROOT="$(dirname "$DIR")"
REMOTE_HOST="${DORY_REMOTE_BUILD_HOST:?Set DORY_REMOTE_BUILD_HOST to the SSH host for the Linux build machine.}"
REMOTE_DIR="${DORY_REMOTE_BUILD_DIR:?Set DORY_REMOTE_BUILD_DIR to the remote Dory build directory.}"
REMOTE_RIPHT_DIR="${DORY_REMOTE_RIPHT_DIR:?Set DORY_REMOTE_RIPHT_DIR to the remote ripht-php-sapi checkout directory.}"

echo "=== 0. Building embedded runtime locally ==="
# Ensure framework changes (Phalanx, dory-runtime) are bundled into the embedded tarball
php "$DORY_ROOT/scripts/build-embed.php"

echo "=== 1. Syncing local source to $REMOTE_HOST ==="
# Sync the dependency crate that Cargo.toml references via a local path
rsync -avz --exclude 'target' --exclude '.git' "$DORY_ROOT/../../../../Rust/ripht-php-sapi/" "$REMOTE_HOST:$REMOTE_RIPHT_DIR/"

# We exclude build artifacts and cached directories so we don't overwrite the server's cache
rsync -avz \
  --exclude 'target' \
  --exclude '.ripht' \
  --exclude '.spc' \
  --exclude '.spc-work' \
  --exclude '.git' \
  "$DORY_ROOT/" "$REMOTE_HOST:$REMOTE_DIR/"

echo "=== 2. Running remote build ==="
ssh "$REMOTE_HOST" "
  set -euo pipefail
  cd $REMOTE_DIR

  # Ensure Cargo is in PATH
  source ~/.cargo/env

  # PATCH: Update local path dependency to reflect remote server layout
  sed -i 's|path = \"../../../../Rust/ripht-php-sapi\"|path = \"$REMOTE_RIPHT_DIR\"|g' Cargo.toml

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
rsync -avz "$REMOTE_HOST:$REMOTE_DIR/target/release/dory" "$DORY_ROOT/dory-linux-x86_64"

echo "=== Success ==="
echo "Linux binary available at: $DORY_ROOT/dory-linux-x86_64"
