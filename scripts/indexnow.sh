#!/usr/bin/env bash
# Notify IndexNow-participating search engines that pages have changed.
#
# IndexNow is a push protocol: instead of waiting to be crawled, you tell the
# engines a URL changed. Bing, Yandex, Seznam, Naver and Yep participate.
# Google does NOT — it ignores IndexNow entirely and still has to be reached
# through Search Console and normal crawling.
#
# Usage:
#   scripts/indexnow.sh                       # submit the default URL set
#   scripts/indexnow.sh https://.../ https://.../#faq
#
# The key file must already be live at HOST/KEY.txt or submissions are rejected.

set -euo pipefail

HOST="marita-universe.com"
KEY="12e02ee5650b470f97de8bd0a9c04b28d5c9e558e1324e0db2f59d7b1f0fbd98"
ENDPOINT="https://api.indexnow.org/indexnow"

if [ "$#" -gt 0 ]; then
  URLS=("$@")
else
  URLS=("https://${HOST}/")
fi

# Refuse to submit if the key file is not reachable — the usual cause of a
# silent 403 from the endpoint.
key_status=$(curl -s -o /dev/null -w '%{http_code}' "https://${HOST}/${KEY}.txt")
if [ "$key_status" != "200" ]; then
  echo "error: key file not reachable (HTTP ${key_status})" >&2
  echo "       expected https://${HOST}/${KEY}.txt to return the key as plain text." >&2
  echo "       Deploy the key file before submitting." >&2
  exit 1
fi

payload=$(python3 - "$HOST" "$KEY" "${URLS[@]}" <<'PY'
import json, sys
host, key, *urls = sys.argv[1:]
print(json.dumps({
    "host": host,
    "key": key,
    "keyLocation": f"https://{host}/{key}.txt",
    "urlList": urls,
}))
PY
)

echo "Submitting ${#URLS[@]} url(s) to IndexNow..."
for u in "${URLS[@]}"; do echo "  $u"; done

code=$(curl -s -o /tmp/indexnow_response -w '%{http_code}' \
  -X POST "$ENDPOINT" \
  -H 'Content-Type: application/json; charset=utf-8' \
  -d "$payload")

case "$code" in
  200) echo "OK (200) — accepted." ;;
  202) echo "OK (202) — accepted, key validation pending." ;;
  400) echo "FAILED (400) — malformed request." ; cat /tmp/indexnow_response ; exit 1 ;;
  403) echo "FAILED (403) — key not valid for this host." ; cat /tmp/indexnow_response ; exit 1 ;;
  422) echo "FAILED (422) — URLs do not belong to the host, or key mismatch." ; cat /tmp/indexnow_response ; exit 1 ;;
  429) echo "FAILED (429) — too many requests; try later." ; exit 1 ;;
  *)   echo "Unexpected response ${code}" ; cat /tmp/indexnow_response ; exit 1 ;;
esac
