# Миссия 1: Поиск метеоритов

## Обзор

Миссия 1 — первая виртуальная миссия платформы WATERS. Цель: отработка автономного поиска, классификации и трекинга метеоритов с использованием Kafka-native Agent Runtime.

| Поле | Значение |
|------|----------|
| **Код** | `mission-1-meteorite` |
| **Тип** | Виртуальная (симуляция на сервере) |
| **Движок** | [mission-control](../mission-control/README.md) — Kafka-native Agent Runtime |
| **Sub-agent'ы** | 5 ролей: explorer, collector, analyzer, matcher, coordinator |
| **LLM** | DeepSeek API + Ollama fallback |
| **Связь с центром** | Kafka (6 топиков), без HTTP bridge |
| **Сервер** | Hetzner (миссионный сервер, отдельный от 238) |
| **Система рангов** | Адаптирована из [h2o-mining.space](https://h2o-mining.space) |
| **Автономия** | L0-L4 (от полной мощности до safe mode) |

## Состав миссии

### Sub-agent'ы (5 ролей, 1 универсальный SubAgent)

| Роль | Кол-во | Назначение | SKILL.md |
|------|--------|------------|----------|
| `explorer` | 4 | Поиск данных: NASA Fireball, спектры | `meteorite-explorer` |
| `collector` | 4 | Сбор деталей: масса, тип, координаты | `meteorite-collector` |
| `analyzer` | 2 | Классификация: тип метеорита, траектория | `meteorite-analyzer` |
| `matcher` | 2 | Поиск аномалий и корреляций | `meteorite-matcher` |
| `coordinator` | 1 | Оркестрация, Kafka bridge, heartbeat | `mission-coordinator` |

Все sub-agent'ы — один универсальный процесс с разными SKILL.md.
Никаких жёстко закодированных типов. Никакого RLM Pro/Flash.

### MCP-серверы

| Сервер | Данные | Транспорт |
|--------|--------|-----------|
| `mcp-nasa-fireball` | NASA Fireball API | MCP stdio |
| `mcp-spectra` | Спектральные базы данных | MCP stdio |
| `mcp-trajectory` | Расчёт траекторий болидов | MCP stdio |
| `mcp-knowledge` (на 238) | ChromaDB + LightRAG | MCP SSE |

### Kafka-протокол (6 топиков)

| Топик | Направление | Формат |
|-------|------------|--------|
| `mission.1.orders.v1` | 238 → Миссия | Приказы (JSON) |
| `mission.1.findings.v1` | Миссия → 238 | Находки (JSON) |
| `mission.1.agents.v1` | Миссия → 238 | Статус агентов |
| `mission.1.ranks.v1` | Миссия → 238 | События рангов |
| `mission.1.heartbeat.v1` | Миссия → 238 | Healthcheck |
| `mission.1.config.v1` | 238 → Миссия | Конфигурация |

JSON-схемы: `schemas/mission_*.v1.json`

### Автономия L0-L4

| Уровень | Kafka | LLM | Режим |
|---------|-------|-----|-------|
| L0 | ✅ Online | DeepSeek API | Полная мощность |
| L1 | ✅ Online | Ollama local | Экономия API |
| L2 | ❌ Offline | Ollama local | Автономный, очередь в Redis |
| L3 | ❌ Offline | Ollama урезанная | Энергосбережение |
| L4 | ❌ Offline | Нет | Safe mode, только логи |

### Система рангов

| Ранг | Findings | Confidence | Привилегия |
|------|----------|-----------|-----------|
| 1 — Bronze | ≥10 | ≥50% | Базовые MCP |
| 2 — Silver | ≥50 | ≥70% | Анализатор |
| 3 — Gold | ≥200 | ≥85% | Управление sub-agent'ами |
| 4 — Platinum | ≥1000 | ≥95% | Mission override |

### Система памяти

| Хранилище | Где | Данные |
|-----------|-----|--------|
| Redis | Миссионный сервер | Состояние миссии, очередь findings |
| ChromaDB | ХАБ 238 | Векторные эмбеддинги всех findings |
| LightRAG | ХАБ 238 | Граф знаний метеоритов |

На миссионном сервере **нет** ChromaDB/LightRAG.
Только Redis (кэш) + Kafka log (история).

---

**Документация движка:** [docs/mission-control/](../mission-control/)
**Сравнение с TUI:** [WATERS_VS_TUI.md](../mission-control/WATERS_VS_TUI.md)
**Доктрина миссии:** [doctrine/mission_system.md](../../doctrine/mission_system.md)
