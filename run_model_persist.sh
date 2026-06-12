#!/usr/bin/env bash
# run_model_persist.sh — Запуск OpenCode с сохранением профиля (НЕ перезаписывает AGENTS.md и opencode.json)
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK_BASE="$HOME/.local/share/waters/profiles"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'

usage() {
    echo -e "${CYAN}run_model_persist.sh${NC} — запуск OpenCode с сохранением профиля"
    echo "Синтаксис:"
    echo "  $0 -m <3|7|14|4> -s <arch|build|meaning|safe>"
    echo "  $0 -m <3|7|14|4> -a <agent>"
    echo "  $0 -m <3|7|14|4> -a <agent> -r"
    exit 1
}

MODE=""; SET=""; AGENT=""; RESUME=false
while getopts "m:s:a:rh" opt; do
    case $opt in
        m) MODE="$OPTARG" ;;
        s) SET="$OPTARG" ;;
        a) AGENT="$OPTARG" ;;
        r) RESUME=true ;;
        h|*) usage ;;
    esac
done

if [ -z "$MODE" ] || { [ -z "$SET" ] && [ -z "$AGENT" ]; }; then usage; fi
[ -n "$SET" ] && [ -n "$AGENT" ] && { echo -e "${RED}Ошибка: -s и -a вместе${NC}"; usage; }

case "$MODE" in 3|7|14|4) ;; *) echo -e "${RED}Модель $MODE не поддерживается${NC}"; usage ;; esac

case "$MODE" in
    3) MODEL_STR="ollama-3b/qwen2.5:3b" ;;
    7) MODEL_STR="ollama-7b/qwen2.5:7b" ;;
    14) MODEL_STR="ollama-14b/qwen2.5:14b" ;;
    4) MODEL_STR="deepseek/deepseek-v4-flash" ;;
esac

if [ -n "$SET" ]; then
    PROFILE="$SET"
    SRC_CONFIG="$REPO_DIR/opencode-${SET}.json"
else
    PROFILE="$AGENT"
    SRC_CONFIG="$REPO_DIR/opencode-test-agent.json"
fi

WORK_DIR="$WORK_BASE/$PROFILE"

# Загрузка .env
[ -f "$REPO_DIR/.env" ] && set -a && source "$REPO_DIR/.env" && set +a

# Проверка Ollama (опционально)
if [[ "$MODE" =~ ^(3|7|14)$ ]]; then
    PORT=$([ "$MODE" = "14" ] && echo "11435" || echo "11434")
    if curl -s --max-time 2 "http://localhost:$PORT/api/tags" >/dev/null 2>&1; then
        echo -e "${GREEN}✅ Ollama :$PORT доступен${NC}"
    else
        echo -e "${YELLOW}⚠️ Ollama :$PORT не отвечает${NC}"
    fi
fi

# Восстановление сессии (-r)
if [ "$RESUME" = true ] && [ -f "$WORK_DIR/opencode.json" ]; then
    echo -e "${GREEN}✅ Сессия восстановлена: ${WORK_DIR}${NC}"
    cd "$WORK_DIR" && exec opencode
fi

# ─── Создание профиля (только если не существует) ─────────────────────────────
mkdir -p "$WORK_DIR"
ln -sfn "$REPO_DIR/agents" "$WORK_DIR/agents"

if [ ! -f "$WORK_DIR/opencode.json" ]; then
    echo -e "${CYAN}Создание нового профиля...${NC}"
    cp "$SRC_CONFIG" "$WORK_DIR/opencode.json"

    # Настройка модели в конфиге
    python3 << EOF
import json, os
cfg_path = os.path.join("${WORK_DIR}", "opencode.json")
with open(cfg_path) as f:
    cfg = json.load(f)
cfg['model'] = "${MODEL_STR}"
if "${MODE}" == "4":
    cfg.setdefault('provider', {})['deepseek'] = {
        'name': 'DeepSeek (WATERS Primary)',
        'apiKey': '{env:DEEPSEEK_API_KEY}',
        'models': {'deepseek-v4-flash': {'name': 'DeepSeek V4 Flash'}}
    }
with open(cfg_path, 'w') as f:
    json.dump(cfg, f, indent=2, ensure_ascii=False)
EOF

# AGENTS.md для одиночного агента
    if [ -n "$AGENT" ]; then
        AGENT_FILE="$REPO_DIR/agents/${AGENT}_AGENTS.md"
        if [ -f "$AGENT_FILE" ]; then
            cp "$AGENT_FILE" "$WORK_DIR/AGENTS.md"
            echo -e "   AGENTS.md скопирован из шаблона"
        else
            echo -e "${YELLOW}⚠ Шаблон AGENTS.md для $AGENT не найден, создан пустой${NC}"
            touch "$WORK_DIR/AGENTS.md"
        fi
    fi
    echo -e "${GREEN}✅ Профиль создан: ${WORK_DIR}${NC}"
else
    echo -e "${GREEN}✅ Используется существующий профиль: ${WORK_DIR}${NC}"
fi
# Запуск OpenCode
echo ""
echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${CYAN}  Запуск OpenCode в ${WORK_DIR}${NC}"
echo -e "${CYAN}  Модель: ${MODEL_STR}${NC}"
[ -n "$SET" ] && echo -e "${CYAN}  Набор: ${SET}${NC}" || echo -e "${CYAN}  Агент: ${AGENT}${NC}"
echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
echo ""

cd "$WORK_DIR" && exec opencode
EOF

chmod +x ~/WATERS/repos/waters-core/run_model_persist.sh
