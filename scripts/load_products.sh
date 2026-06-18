#!/usr/bin/env bash
# Load products into vectoria-algolia via the Algolia-compatible batch API.
#
# Usage:
#   ./scripts/load_products.sh [SERVER_URL] [INDEX_NAME]
#
# Defaults:
#   SERVER_URL  http://localhost:8108
#   INDEX_NAME  products
set -euo pipefail

SERVER="${1:-http://localhost:8108}"
INDEX="${2:-products}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BATCH_FILE="${SCRIPT_DIR}/batch.json"

echo "Loading products from ${BATCH_FILE}"
echo "Target: ${SERVER}/1/indexes/${INDEX}/batch"

RESPONSE=$(curl -sf -w "\n%{http_code}" \
  -X POST "${SERVER}/1/indexes/${INDEX}/batch" \
  -H "Content-Type: application/json" \
  -d @"${BATCH_FILE}")

HTTP_CODE=$(echo "${RESPONSE}" | tail -1)
BODY=$(echo "${RESPONSE}" | head -1)

if [ "${HTTP_CODE}" -ge 200 ] && [ "${HTTP_CODE}" -lt 300 ]; then
  echo "OK: products indexed (HTTP ${HTTP_CODE})"
  echo "${BODY}"
else
  echo "ERROR: HTTP ${HTTP_CODE}" >&2
  echo "${BODY}" >&2
  exit 1
fi
