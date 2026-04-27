#!/usr/bin/env bash
set -euo pipefail

NETWORK="testnet"
RPC_URL="https://soroban-testnet.stellar.org"
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
WASM="target/wasm32-unknown-unknown/release/stellarremit_contract.wasm"

# 1. Build
cargo build --release --target wasm32-unknown-unknown

# 2. Optimize (requires soroban-cli)
soroban contract optimize --wasm "$WASM"

OPTIMIZED="${WASM%.wasm}.optimized.wasm"

# 3. Deploy
CONTRACT_ID=$(soroban contract deploy \
  --wasm "$OPTIMIZED" \
  --source "$STELLAR_SECRET_KEY" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE")

echo "Deployed contract ID: $CONTRACT_ID"

# 4. Init
soroban contract invoke \
  --id "$CONTRACT_ID" \
  --source "$STELLAR_SECRET_KEY" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  -- init \
  --admin "$STELLAR_PUBLIC_KEY"

echo "Contract initialized."
