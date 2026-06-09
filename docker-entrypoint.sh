#!/bin/sh
set -e

trap 'kill $API_PID 2>/dev/null; exit' TERM INT

export CADDY_PORT=${PORT:-8080}
echo "Starting API server..."
/usr/local/bin/api &
API_PID=$!

echo "Waiting for API server..."
for i in $(seq 1 30); do
	if curl -sf http://localhost:3001/api/health > /dev/null 2>&1; then
		echo "API server ready"
		break
	fi
	sleep 1
done

echo "Starting Caddy..."
exec /usr/local/bin/caddy run --config /etc/caddy/Caddyfile --adapter caddyfile
