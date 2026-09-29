#!/usr/bin/env bash

set -euo pipefail

readonly REPO_URL="https://github.com/munificent/craftinginterpreters.git"
readonly FIXTURES_DIR="$(git rev-parse --show-toplevel)/tests/fixtures"
readonly TMP_DIR="$(mktemp -d)"

trap 'rm -rf "$TMP_DIR"' EXIT

git clone --depth 1 --filter=blob:none --sparse "$REPO_URL" "$TMP_DIR/craftinginterpreters"

git -C "$TMP_DIR/craftinginterpreters" sparse-checkout set test

readonly REVISION="$(git -C "$TMP_DIR/craftinginterpreters" rev-parse HEAD)"

rm -rf "$FIXTURES_DIR"
mkdir -p "$FIXTURES_DIR"

cp -a "$TMP_DIR/craftinginterpreters/test/." "$FIXTURES_DIR/"
printf '%s\n' "$REVISION" >"$FIXTURES_DIR/.upstream-revision"

echo "Updated Crafting Interpreters fixtures to $REVISION"
