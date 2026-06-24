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

# DNA modifier injection (progenitor key + network seed).
#
# Committed dna.yaml ships SAFE DEFAULTS: progenitor_pubkey null (bootstrap
# mode) and network_seed "fieldnotes-network-v1". Forkers override either or
# both via env vars to stand up their OWN isolated network with their OWN admin:
#
#   FIELDNOTES_PROGENITOR_PUBKEY  durable Flowsta agent key (uhCAk..., from
#                                 Vault /status) -- enables admin enforcement
#   FIELDNOTES_NETWORK_SEED       a unique string -- isolates this network from
#                                 every other Fieldnotes deployment
#
# Both values are DNA modifiers, so changing either yields a different DNA
# hash = a separate DHT. We substitute just for the pack, then a trap restores
# dna.yaml on ANY exit, so no per-deployer value is ever committed or left in
# the working tree.
if [ -n "$FIELDNOTES_PROGENITOR_PUBKEY" ] || [ -n "$FIELDNOTES_NETWORK_SEED" ]; then
    cp workdir/dna.yaml workdir/dna.yaml.bak
    trap 'mv -f workdir/dna.yaml.bak workdir/dna.yaml 2>/dev/null || true' EXIT

    if [ -n "$FIELDNOTES_PROGENITOR_PUBKEY" ]; then
        echo "Injecting progenitor pubkey from FIELDNOTES_PROGENITOR_PUBKEY..."
        if ! grep -q "progenitor_pubkey: null" workdir/dna.yaml; then
            echo "ERROR: expected progenitor_pubkey: null in dna.yaml" >&2; exit 1
        fi
        sed -i.sedbak "s|progenitor_pubkey: null|progenitor_pubkey: \"$FIELDNOTES_PROGENITOR_PUBKEY\"|" workdir/dna.yaml
        rm -f workdir/dna.yaml.sedbak
        echo "  progenitor_pubkey set for this build"
    fi

    if [ -n "$FIELDNOTES_NETWORK_SEED" ]; then
        echo "Injecting network seed from FIELDNOTES_NETWORK_SEED..."
        sed -i.sedbak "s|network_seed: \".*\"|network_seed: \"$FIELDNOTES_NETWORK_SEED\"|" workdir/dna.yaml
        rm -f workdir/dna.yaml.sedbak
        echo "  network_seed set for this build"
    fi
else
    echo "No FIELDNOTES_PROGENITOR_PUBKEY or FIELDNOTES_NETWORK_SEED set -- using committed defaults (progenitor_pubkey: null, network_seed: fieldnotes-network-v1)."
fi

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
