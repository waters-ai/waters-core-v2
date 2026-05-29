#!/usr/bin/env bash
# Запуск агента: Конструктор Сети v1.0
# Использование:
#   ./run_constructor.sh          # обычный запуск (7B CPU)
#   ./run_constructor.sh --tmux   # запуск в tmux-сессии (24/7)
set -euo pipefail

AGENT="constructor"
AGENT_FILE="agents/${AGENT}_AGENTS.md"
MODE="${1:-}"

if [ ! -f "$AGENT_FILE" ]; then
    echo "❌ Файл $AGENT_FILE не найден"
    exit 1
fi

echo "🌐 Запуск Конструктора Сети v1.0..."
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
    # Keep model in memory (10 мин)
    curl -s http://localhost:11434/api/generate \
      -d '{"model":"qwen2.5:7b","keep_alive":"10m","prompt":""}' > /dev/null 2>&1 || true
    echo "   ✅ Модель qwen2.5:7b зафиксирована в памяти (keep_alive=10m)"
else
    echo "   ⚠️ Ollama не отвечает на localhost:11434"
fi

# ─── Startup Sequence ────────────────────────────────────────────────
echo ""
echo "   🔄 Startup Sequence (8 команд)..."
export REPO_DIR="$(dirname "$0")"
STARTUP_OUTPUT=$(python3 "$(dirname "$0")/scripts/constructor_startup.py" 2>/dev/null) || {
    echo "   ⚠️ Startup Sequence: некоторые сервисы недоступны"
    STARTUP_OUTPUT=""
}
if [ -n "$STARTUP_OUTPUT" ]; then
    echo "$STARTUP_OUTPUT" >> AGENTS.md
    echo "   ✅ Startup контекст добавлен в AGENTS.md"
fi

# ─── Запуск ──────────────────────────────────────────────────────────
if [ "$MODE" = "--tmux" ]; then
    echo ""
    echo "🌐 Запуск в tmux-сессии..."
    exec "$(dirname "$0")/scripts/opencode_tmux.sh" "$AGENT" start
fi

echo ""
exec opencode
