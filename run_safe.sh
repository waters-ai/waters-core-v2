#!/usr/bin/env bash
# Запуск сборки Безопасность: @keeper (14b) + @lawkeeper @architect (7b)
set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -f "$REPO_DIR/.env" ]; then
  set -a; source "$REPO_DIR/.env"; set +a
fi

# Сервер 237 уничтожен — SSH-туннели удалены
echo "   ⚠️ Сервер 237 недоступен. Используются локальные сервисы."

echo "🛡️ Безопасность — @keeper (14b) лидирует"
cp "$REPO_DIR/agents/keeper_AGENTS.md" "$REPO_DIR/AGENTS.md"
opencode -c "$REPO_DIR/opencode-safe.json"
