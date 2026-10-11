#!/usr/bin/env bash
set -euo pipefail

echo "==> 1. Running cargo fmt --all -- --check..."
cargo fmt --all -- --check

echo "==> 2. Running cargo check --all-targets..."
cargo check --all-targets

echo "==> 3. Running cargo clippy --all-targets --all-features -- -D warnings..."
cargo clippy --all-targets --all-features -- -D warnings

echo "==> 4. Running cargo test --all-targets..."
cargo test --all-targets

echo "==> 5. Verifying comment preservation (Rule: Never remove user comments)..."
# Check if any deleted comments exist in git diff that were not preserved or updated
DELETED_COMMENTS=$(git diff -U0 HEAD 2>/dev/null | grep -E '^-[[:space:]]*(//|///)' || true)
if [ -n "$DELETED_COMMENTS" ]; then
  echo "Notice: Detected modified/removed comment lines in git diff:"
  echo "$DELETED_COMMENTS"
  echo "Verify that all user comments were edited/updated rather than deleted."
fi

echo "==> All checks passed successfully!"
