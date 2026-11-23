#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
export CARGO_HOME="$ROOT/.tooling/cargo"
export RUSTUP_HOME="$ROOT/.tooling/rustup"
export PATH="$CARGO_HOME/bin:$PATH"

COMPOSE="docker compose -f tests/openssh/compose.yml -f tests/fault-injection/compose.yml"
CONTAINER=sparrowshell-openssh-integration

reset_faults() {
    docker exec "$CONTAINER" sh -c '
        tc qdisc del dev eth0 root 2>/dev/null || true
        umount /home/sparrow/full 2>/dev/null || true
        rm -rf /home/sparrow/full
    ' >/dev/null 2>&1 || true
}

cleanup() {
    reset_faults
    $COMPOSE down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

$COMPOSE up -d --build --wait

for scenario in latency packet-loss large-output disk-full; do
    reset_faults
    case "$scenario" in
        latency)
            docker exec "$CONTAINER" tc qdisc replace dev eth0 root netem delay 1200ms 200ms
            ;;
        packet-loss)
            docker exec "$CONTAINER" tc qdisc replace dev eth0 root netem loss 20%
            ;;
        disk-full)
            docker exec "$CONTAINER" sh -c '
                mkdir -p /home/sparrow/full
                mount -t tmpfs -o size=64k tmpfs /home/sparrow/full
                chown sparrow:sparrow /home/sparrow/full
                dd if=/dev/zero of=/home/sparrow/full/fill bs=1024 count=64 conv=fsync 2>/dev/null || true
            '
            ;;
    esac
    printf '\n==> fault scenario: %s\n' "$scenario"
    SPARROWSHELL_FAULT_SCENARIO="$scenario" cargo test \
        -p sparrowshell-ssh-core \
        --test openssh_fault_injection \
        -- --ignored --nocapture
    reset_faults
done
