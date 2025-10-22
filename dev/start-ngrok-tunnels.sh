#!/bin/bash

set -e

RANDOM_SUFFIX="$(date +%s)-${RANDOM}"

# Start first tunnel for port 5556 with HTTP/2 for gRPC
ngrok http 5556 --app-protocol=http2 --url="node-${RANDOM_SUFFIX}.ngrok-free.app" --log=stdout > ngrok-5556.log 2>&1 &
PID_5556=$!
echo $PID_5556 > ngrok-5556.pid

# Start second tunnel for port 5558 with HTTP/2 for gRPC
ngrok http 5558 --url="history-${RANDOM_SUFFIX}.ngrok-free.app" --log=stdout > ngrok-5558.log 2>&1 &
PID_5558=$!
echo $PID_5558 > ngrok-5558.pid

# Wait for both tunnel URLs
for i in {1..30}; do
  NODE_URL=$(grep -oP 'url=https://[a-z0-9-]+\.ngrok-free\.(app|dev|io)' ngrok-5556.log 2>/dev/null | head -1 | sed 's/url=//' || true)
  HISTORY_URL=$(grep -oP 'url=https://[a-z0-9-]+\.ngrok-free\.(app|dev|io)' ngrok-5558.log 2>/dev/null | head -1 | sed 's/url=//' || true)

  if [ -n "$NODE_URL" ] && [ -n "$HISTORY_URL" ]; then
    mkdir -p tunnel-info
    echo "${NODE_URL}:443" > tunnel-info/node-url.txt
    echo "${HISTORY_URL}:443" > tunnel-info/history-url.txt
    echo "Tunnels started successfully"
    echo "Node (5556): $NODE_URL"
    echo "History (5558): $HISTORY_URL"
    exit 0
  fi
  sleep 1
done

echo "Error: Failed to get tunnel URLs" >&2
echo "=== 5556 Log ===" >&2
cat ngrok-5556.log >&2
echo "=== 5558 Log ===" >&2
cat ngrok-5558.log >&2
exit 1
