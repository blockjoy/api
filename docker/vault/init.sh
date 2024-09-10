#!/usr/bin/env bash

set -euo pipefail

RETRIES=5
until vault status >/dev/null 2>&1 || [ "${RETRIES}" -eq 0 ]; do
    echo "Waiting for vault to start...: $((RETRIES--))"
    sleep 1
done

echo "Setting up vault..."

vault login token=vault-root-token
vault secrets enable -version=2 -path=blockjoy kv

echo "Vault setup complete."
