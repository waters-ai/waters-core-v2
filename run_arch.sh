#!/usr/bin/env bash
# Запуск Архитектурной сборки: @architect (14b) + @constructor @integrator (7b)
set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

# Загрузка секретов
if [ -f "$REPO_DIR/.env" ]; then
  set -a; source "$REPO_DIR/.env"; set +a
fi

# SSH туннели (если не запущены)
# Сервер 237 уничтожен — SSH-туннели удалены
echo "   ⚠️ Сервер 237 недоступен. Используются локальные сервисы."

echo "🏛️ Архитектурная сборка — @architect (14b) лидирует"
cp "$REPO_DIR/agents/architect_AGENTS.md" "$REPO_DIR/AGENTS.md"
opencode -c "$REPO_DIR/opencode-arch.json"
