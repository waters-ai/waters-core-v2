#!/usr/bin/env bash
# Запуск агента: Верховный Архитектор v1.0
# Использование:
#   ./run_architect.sh          # обычный запуск
#   ./run_architect.sh --tmux   # запуск в tmux-сессии (24/7)
set -euo pipefail

AGENT="architect"
AGENT_FILE="agents/${AGENT}_AGENTS.md"
MODE="${1:-}"

if [ ! -f "$AGENT_FILE" ]; then
    echo "❌ Файл $AGENT_FILE не найден"
    exit 1
fi

echo "🏛️  Запуск Верховного Архитектора v1.0..."
echo "   AGENTS.md ← $AGENT_FILE"

cp "$AGENT_FILE" AGENTS.md

# Загрузка секретов из .env
if [ -f "$(dirname "$0")/.env" ]; then
  set -a; source "$(dirname "$0")/.env"; set +a
fi

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
if [ "$MODE" = "--tmux" ]; then
    echo ""
    echo "🌐 Запуск в tmux-сессии..."
    exec "$(dirname "$0")/scripts/opencode_tmux.sh" "$AGENT" start
fi

echo ""
exec opencode
