#!/bin/bash

# Build script for Fieldnotes DNA (forked from ProofPoll v1.3).
#
# Builds ONLY the polls zome (now Item / Response / Finding) and reuses the
# agent_linking_*.wasm already committed in workdir/ — so no
# flowsta-agent-linking crate is needed. Identity/auth is unchanged.
#
# Prerequisites:
#   - hc CLI 0.6.0
#
# Run from this directory:  cd dna/v1.3 && bash build.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "Building Fieldnotes DNA"

if ! command -v hc &> /dev/null; then
    echo "Error: Holochain CLI (hc) not found"
    echo "Install with: cargo install holochain_cli --version 0.6.0"
    exit 1
fi

mkdir -p workdir

echo "Building polls zomes (Item / Response / Finding)..."
RUSTFLAGS='--cfg getrandom_backend="custom"' CARGO_TARGET_DIR=target \
    cargo build --release --target wasm32-unknown-unknown

echo "Copying polls WASM (reusing the committed agent_linking WASM)..."
cp target/wasm32-unknown-unknown/release/polls_integrity.wasm workdir/
cp target/wasm32-unknown-unknown/release/polls_coordinator.wasm workdir/

echo "Packing DNA..."
hc dna pack workdir

echo "Packing hApp..."
hc app pack workdir

RESOURCES_DIR="$SCRIPT_DIR/../../src-tauri/resources"
if [ -d "$RESOURCES_DIR" ] || [ -d "$SCRIPT_DIR/../../src-tauri" ]; then
    mkdir -p "$RESOURCES_DIR"
    cp workdir/proofpoll_v1_3_happ.happ "$RESOURCES_DIR/"
    echo "Copied hApp to src-tauri/resources/"
fi

echo ""
echo "Build complete!"
echo "  - DNA:  workdir/proofpoll_v1_3.dna"
echo "  - hApp: workdir/proofpoll_v1_3_happ.happ"
