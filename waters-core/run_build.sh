#!/usr/bin/env bash
# Запуск Строительной сборки: @constructor (14b) + @architect @integrator (7b)
set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -f "$REPO_DIR/.env" ]; then
  set -a; source "$REPO_DIR/.env"; set +a
fi

# Сервер 237 уничтожен — SSH-туннели удалены
echo "   ⚠️ Сервер 237 недоступен. Используются локальные сервисы."

echo "🌐 Строительная сборка — @constructor (14b) лидирует"
cp "$REPO_DIR/agents/constructor_AGENTS.md" "$REPO_DIR/AGENTS.md"
opencode -c "$REPO_DIR/opencode-build.json"
