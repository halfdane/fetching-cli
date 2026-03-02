#!/usr/bin/env bash
set -e

cd "$(git rev-parse --show-toplevel)"

# ── Pre-flight checks ───────────────────────────────────────────────────
echo "Running tests..."
cargo test

echo "Running clippy..."
cargo clippy -- -D warnings

echo "Checking formatting..."
cargo fmt -- --check

# If any tracked files were changed, abort so the user can review.
if ! git diff --quiet HEAD; then
  echo "⚠️  Working tree has uncommitted changes." >&2
  echo "   Please review, commit, and re-run." >&2
  exit 1
fi

# ── Bump version ─────────────────────────────────────────────────────────
# Read current version from Cargo.toml
CURRENT=$(grep '^version = "' Cargo.toml | head -1 | sed 's/.*version = "\([^"]*\)".*/\1/')
if [[ -z "$CURRENT" ]]; then
  echo "Could not determine current version from Cargo.toml" >&2
  exit 1
fi

# Auto-bump patch, or accept an explicit version as first argument
if [[ -n "$1" ]]; then
  NEXT="$1"
else
  IFS='.' read -r major minor patch <<< "$CURRENT"
  NEXT="$major.$minor.$((patch + 1))"
fi

echo "Bumping $CURRENT -> $NEXT"

# Patch Cargo.toml in place
sed -i "s/^version = \"$CURRENT\"/version = \"$NEXT\"/" Cargo.toml

# Update Cargo.lock
cargo check --quiet 2>/dev/null || true

git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to v$NEXT"

TAG="v$NEXT"
git tag "$TAG"
git push origin main
git push origin "$TAG"

echo "Tagged $TAG"
