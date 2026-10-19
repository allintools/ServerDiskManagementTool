#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TEST_DIR="$ROOT/tests/ssh"
STATE_DIR="$TEST_DIR/state"
KEYS_DIR="$TEST_DIR/keys"
FIXTURES="$TEST_DIR/fixtures/data"
CONTAINER=rack-ssh-test
PORT="${RACK_TEST_PORT:-2222}"
BIN="${RACK_SELFTEST_BIN:-$ROOT/src-tauri/target/debug/ServerDiskManagementTool}"

mkdir -p "$KEYS_DIR" "$STATE_DIR" "$FIXTURES/logs" "$FIXTURES/space dir"
if [ ! -f "$KEYS_DIR/id_ed25519" ]; then
  ssh-keygen -q -t ed25519 -N '' -f "$KEYS_DIR/id_ed25519"
fi
if [ ! -f "$KEYS_DIR/id_ed25519_enc" ]; then
  ssh-keygen -q -t ed25519 -N 'rackpass' -f "$KEYS_DIR/id_ed25519_enc"
fi
dd if=/dev/zero of="$FIXTURES/delete-me.bin" bs=1048576 count=2 2>/dev/null
if [ ! -f "$FIXTURES/large.bin" ] || [ "$(stat -f%z "$FIXTURES/large.bin" 2>/dev/null || echo 0)" -lt 100000000 ]; then
  dd if=/dev/zero of="$FIXTURES/large.bin" bs=1048576 count=120 2>/dev/null
fi
printf 'demo log line\n' > "$FIXTURES/logs/app.log"
printf 'unicode fixture\n' > "$FIXTURES/space dir/中文 文件.txt"

if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER}$"; then
  docker rm -f "$CONTAINER" >/dev/null
fi
docker build -q -t "$CONTAINER" "$TEST_DIR" >/dev/null
docker run -d --name "$CONTAINER" -p "127.0.0.1:${PORT}:22" -v "$FIXTURES:/data" "$CONTAINER" >/dev/null
cleanup() { if [ "${KEEP:-0}" != "1" ]; then docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; fi }
trap cleanup EXIT

for i in $(seq 1 30); do
  if ssh-keyscan -T 2 -p "$PORT" 127.0.0.1 >/dev/null 2>&1; then break; fi
  sleep 1
done
ssh-keyscan -T 3 -p "$PORT" 127.0.0.1 >/dev/null 2>&1 || { echo "sshd did not start"; docker logs "$CONTAINER" | tail -30; exit 1; }

if [ ! -x "$BIN" ]; then
  echo "selftest binary missing: $BIN"
  echo "run: (cd src-tauri && cargo build)"
  exit 1
fi

echo "Running Nova Rack SSH integration selftest against 127.0.0.1:${PORT}"
RACK_TEST_HOST=127.0.0.1 RACK_TEST_PORT="$PORT" RACK_TEST_USER=rack \
RACK_TEST_KEY="$KEYS_DIR/id_ed25519" RACK_TEST_ENC_KEY="$KEYS_DIR/id_ed25519_enc" \
RACK_TEST_PASSWORD=racktest RACK_TEST_ENC_PASSWORD=rackpass RACK_SELFTEST_DATA="$STATE_DIR/selftest" \
"$BIN" --selftest-ssh
