#!/usr/bin/env bash
# Build + deploy brain.geiger.ink on air13. Mirrors ~/server/redeploy-48hfp.sh.
# Run as user mgeiger; it sudo's internally for the root-only steps.

set -euo pipefail

REPO="$(cd "$(dirname "$0")" && pwd)"
PLIST_SRC="$REPO/deploy/ink.geiger.brain.plist"
PLIST_DST=/Library/LaunchDaemons/ink.geiger.brain.plist
LABEL=system/ink.geiger.brain
PORT=4006
HOSTNAME=brain.geiger.ink
CADDYFILE=/opt/homebrew/etc/Caddyfile

echo "==> copy MRI into client/ (gitignored)"
"$REPO/scripts/setup-brain-data.sh"

echo "==> trunk build --release (wasm client)"
( cd "$REPO/client" && trunk build --release )

echo "==> cargo build --release -p server"
( cd "$REPO" && cargo build --release -p server )

echo "==> validate plist"
plutil -lint "$PLIST_SRC" >/dev/null

echo "==> ensure log dir exists"
mkdir -p ~/Library/Logs/geiger

echo "==> install plist + respawn daemon (sudo)"
sudo cp "$PLIST_SRC" "$PLIST_DST"
sudo chown root:wheel "$PLIST_DST"
sudo chmod 644 "$PLIST_DST"
sudo launchctl bootout "$LABEL" 2>/dev/null || true
sudo launchctl bootstrap system "$PLIST_DST"

echo "==> wait for local port=${PORT}"
for i in {1..15}; do
    code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 2 "http://localhost:${PORT}/" || echo 000)
    if [ "$code" = "200" ]; then
        echo "    local service up (HTTP ${code}) after ${i}s"
        break
    fi
    sleep 1
done

if ! grep -q "${HOSTNAME}" "$CADDYFILE"; then
    echo "==> adding Caddyfile block for ${HOSTNAME} (sudo)"
    sudo tee -a "$CADDYFILE" >/dev/null <<EOF

${HOSTNAME} {
    reverse_proxy localhost:${PORT}
}
EOF
    echo "==> caddy validate"
    caddy validate --config "$CADDYFILE"
    echo "==> hot-reload caddy"
    curl -s localhost:2019/load -H "Content-Type: text/caddyfile" \
        --data-binary "@${CADDYFILE}" -X POST
    echo
else
    echo "==> Caddyfile already contains ${HOSTNAME} — skipping edit/reload"
fi

echo
echo "✓ done. Visit https://${HOSTNAME}/ (first hit triggers Let's Encrypt issuance — may take ~30s)."
