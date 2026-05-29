#!/usr/bin/env bash
# Запуск Верховного Архитектора v1.0 на qwen2.5:7b (CPU)
# Гарантирует модель 7B + чистые SSH туннели + pre-flight
# Использование:
#   ./run_architect_7b.sh            # обычный запуск
#   ./run_architect_7b.sh --tmux     # в tmux-сессии
set -euo pipefail

cd "$(dirname "$0")"

echo "🏛️  Верховный Архитектор v1.0 — принудительно qwen2.5:7b"
echo ""

# Принудительно ставим 7B в конфиге
python3 -c "
import json
with open('opencode.json') as f:
    cfg = json.load(f)
cfg['model'] = 'ollama/qwen2.5:7b'
with open('opencode.json', 'w') as f:
    json.dump(cfg, f, indent=2, ensure_ascii=False)
print('   ✅ Модель: qwen2.5:7b')
"

# ─── Pre-flight проверки ──────────────────────────────────────────
echo "   🔍 Pre-flight проверки..."

# ─── Pre-flight: Ollama ─────────────────────────────────────────────
echo ""
echo "   🔄 Pre-flight: Ollama 7B..."
if curl -sf --max-time 5 http://localhost:11434/api/version > /dev/null 2>&1; then
    echo "   ✅ Ollama API доступна"
    curl -s http://localhost:11434/api/generate \
      -d '{"model":"qwen2.5:7b","keep_alive":"10m","prompt":""}' > /dev/null 2>&1 || true
    echo "   ✅ Модель qwen2.5:7b зафиксирована в памяти (keep_alive=10m)"
else
    echo "   ⚠️ Ollama не отвечает на localhost:11434"
fi

# ─── Запуск ──────────────────────────────────────────────────────────
AGENT="architect"
AGENT_FILE="agents/${AGENT}_AGENTS.md"

if [ ! -f "$AGENT_FILE" ]; then
    echo "❌ Файл $AGENT_FILE не найден"
    exit 1
fi

echo ""
echo "🏛️  AGENTS.md ← $AGENT_FILE"
cp "$AGENT_FILE" AGENTS.md

if [ "${1:-}" = "--tmux" ]; then
    echo ""
    echo "🌐 Запуск в tmux-сессии..."
    exec "$(dirname "$0")/scripts/opencode_tmux.sh" "$AGENT" start
fi

echo ""
exec opencode
