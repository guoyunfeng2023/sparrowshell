#!/bin/bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
report="$root/.artifacts/native-ai-ui-e2e-report.json"
log="$root/.artifacts/native-ai-ui-e2e.log"
app_pid=""
mkdir -p "$root/.artifacts"
cleanup() {
  if [[ -n "$app_pid" ]]; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  (cd "$root" && npm run tauri -- build --debug --no-bundle >/dev/null) || true
}
trap cleanup EXIT
if [[ "${VITE_SPARROWSHELL_E2E_SKIP_LIVE_ANALYSIS:-0}" != "1" ]]; then
  curl -fsS --max-time 3 http://127.0.0.1:8070/v1/ai/health | python -c 'import json,sys; health=json.load(sys.stdin); assert health.get("providerConfigured") is True, health'
fi
(cd "$root" && VITE_SPARROWSHELL_E2E=1 npm run tauri -- build --debug --no-bundle)
rm -f "$report"
SPARROWSHELL_E2E_REPORT="$report" "$root/target/debug/sparrowshell-desktop" >"$log" 2>&1 &
app_pid=$!
for attempt in {1..240}; do
  if [[ -f "$report" ]]; then break; fi
  if ! kill -0 "$app_pid" 2>/dev/null; then
    python -c 'from pathlib import Path; print(Path("'"$log"'").read_text(errors="replace"))'
    exit 1
  fi
  sleep 0.5
done
if [[ ! -f "$report" ]]; then
  echo "原生 UI 验收超时，未生成报告" >&2
  exit 1
fi
python -c 'import json,sys; report=json.load(open(sys.argv[1])); print(json.dumps(report,ensure_ascii=False,indent=2)); assert report.get("status")=="passed",report' "$report"
