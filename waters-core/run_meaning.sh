#!/usr/bin/env bash
# Запуск Смысловой сборки: @lawkeeper (14b) + @director @keeper (7b)
set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -f "$REPO_DIR/.env" ]; then
  set -a; source "$REPO_DIR/.env"; set +a
fi

# Сервер 237 уничтожен — SSH-туннели удалены
echo "   ⚠️ Сервер 237 недоступен. Используются локальные сервисы."

echo "⚖️ Смысловая сборка — @lawkeeper (14b) лидирует"
cp "$REPO_DIR/agents/lawkeeper_AGENTS.md" "$REPO_DIR/AGENTS.md"
opencode -c "$REPO_DIR/opencode-meaning.json"
