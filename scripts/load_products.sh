#!/usr/bin/env bash
# Load sample_products.json into vectoria-algolia via the Algolia-compatible batch API.
#
# Usage:
#   ./scripts/load_products.sh [SERVER_URL] [INDEX_NAME]
#
# Defaults:
#   SERVER_URL  http://localhost:8108
#   INDEX_NAME  products
#
# Requires: curl, jq
set -euo pipefail

SERVER="${1:-http://localhost:8108}"
INDEX="${2:-products}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PRODUCTS_FILE="${SCRIPT_DIR}/products.json"

if ! command -v jq &>/dev/null; then
  echo "error: jq is required. Install with: brew install jq" >&2
  exit 1
fi

echo "Loading products from ${PRODUCTS_FILE}"
echo "Target: ${SERVER}/1/indexes/${INDEX}/batch"

# Read products.json and build an Algolia batch payload.
BATCH=$(jq '{requests: [.[] | {"action": "addObject", "body": .}]}' "${PRODUCTS_FILE}")
COUNT=$(echo "${BATCH}" | jq '.requests | length')

echo "Sending ${COUNT} products..."

RESPONSE=$(curl -s -w "\n%{http_code}" \
  -X POST "${SERVER}/1/indexes/${INDEX}/batch" \
  -H "Content-Type: application/json" \
  -d "${BATCH}")

HTTP_CODE=$(echo "${RESPONSE}" | tail -1)
BODY=$(echo "${RESPONSE}" | head -1)

if [ "${HTTP_CODE}" -ge 200 ] && [ "${HTTP_CODE}" -lt 300 ]; then
  INDEXED=$(echo "${BODY}" | jq '.objectIDs | length' 2>/dev/null || echo "?")
  echo "OK: ${INDEXED} products indexed (HTTP ${HTTP_CODE})"
else
  echo "ERROR: HTTP ${HTTP_CODE}" >&2
  echo "${BODY}" >&2
  exit 1
fi

echo ""
echo "Verify with:"
echo "  curl -s -X POST ${SERVER}/1/indexes/${INDEX}/query \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"query\": \"running shoe\", \"hitsPerPage\": 3}' | jq '.hits[].objectID'"
