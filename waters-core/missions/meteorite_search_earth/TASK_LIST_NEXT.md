# Задание на следующий сеанс

> **Архитектор:** `agent.architect.v1`
> **Конструктор:** `agent.constructor.v1`
> **Дата:** 2026-05-16
> **Основание:** Утверждённая архитектура waters-node v0.4, миссия meteorite_search_earth, доктрина virtual_life_modes

---

## Архитектор: документация и онтология

### A1 — Опубликовать платформенные документы в waters-core

- [ ] Создать PR в `waters-core`:
  - `missions/meteorite_search_earth/control_center.md`
  - `doctrine/virtual_life_modes.md`
- [ ] Дополнить `missions/meteorite_search_earth/README.md` — ссылки на новые документы
- [ ] Создать `missions/meteorite_search_earth/reports/` — заглушки для отчётов по миссии

### A2 — Онтология миссии в LightRAG

- [ ] Загрузить онтологию meteorite_search_earth в LightRAG:
  - 4 группы агентов (SEARCH, LAB, COMMAND, OBSERVE)
  - 8 DTN-режимов
  - Схема потоков данных
  - MCP-мосты по площадкам
- [ ] Создать граф: узел `mission.v1.meteorite_search` → связи с агентами, нодами, Kafka-топиками

### A3 — Спецификации для Конструктора (P0)

- [ ] Написать `specs/DTN_MODES.md` — техническое задание на 8 режимов DTN:
  - Автомат переключения по энергии + связи
  - Offline queue с приоритетами
  - DTN-CHUNK: чанкование + resume
  - DTN-SCHEDULE: конфиг расписания
  - DTN-COMPRESS: алгоритмы сжатия под каждый тип данных
- [ ] Написать `specs/LASER_BRIDGE.md` — техническое задание на FSO-модуль:
  - Лазерный протокол между нодами
  - Автомат выбора канала: лазер ↔ Starlink ↔ DTN
  - Дрон как лазерный ретранслятор
- [ ] Написать `specs/GROUP_TASKS.md` — общие задачи группы:
  - Координатор группы распределяет подзадачи
  - Cross-group communication через Kafka
  - 6 нод × 6 лабораторий — одна задача

### A4 — Продуктовая документация

- [ ] Проверить `waters-ai/waters-node/README.md` — все ссылки на docs/ работают
- [ ] Создать `waters-ai/waters-node/CHANGELOG.md` — что в v0.4

---

## Конструктор: код и реализация

### C1 — DTN-модуль (P0, ~3 дня)

Файлы: `waters-node/src/dtn.rs`, `waters-node/src/channel.rs`

- [ ] Реализовать 8 DTN-режимов (документация: `specs/DTN_MODES.md`)
  - `DTN-OFF` — сквозная передача, DTN спит
  - `DTN-EVENT` — дублирование критичных находок
  - `DTN-STORE` — offline_queue.jsonl с лимитом 1000/50MB
  - `DTN-CHUNK` — чанкование по 256KB + CRC32 + resume
  - `DTN-PRIORITY` — приоритет: findings > confirm > heartbeat > telemetry > raw
  - `DTN-SCHEDULE` — расписание из config.toml
  - `DTN-COMPRESS` — gzip/bzip2/lossy JPEG
  - `DTN-BEACON` — только heartbeat 64 байта
- [ ] Автомат переключения: battery_pct + solar_w → DTN-режим
- [ ] DTN-мост между нодами в разных режимах (см. матрицу совместимости)
- [ ] Kafka-обратная связь: `power.v1` топик

Приёмка:
```bash
# Тест DTN-STORE + flush
./waters-node --dtn-mode store
./send drone-operator "найди метеорит"
# → offline_queue.jsonl создан
# → при восстановлении: flush → Kafka
```

### C2 — Лазерный мост FSO (P1, ~2 дня)

Файлы: `waters-node/src/media_bridge.rs`, `waters-node/src/bridge.rs`

- [ ] Новый bridge type: `"type": "fso"` в bridges.json
- [ ] Лазерный протокол: захват цели, компенсация вибраций, LOS check
- [ ] Автомат: `select_channel(weather, los, time) → laser | starlink | dtn`
- [ ] Дрон-ретранслятор: дрон как FSO-мост между площадками

```json
{
  "bridges": [
    {
      "name": "fso-to-mountain",
      "type": "fso",
      "target": "10.0.0.2:43000",
      "wavelength_nm": 1550,
      "bandwidth_gbps": 10,
      "los_check_interval_sec": 60
    }
  ]
}
```

Приёмка:
```bash
/bridges
# → fso-to-mountain: ✅ 10 Gbps, LOS: true
/laser test-connection mountain
# → latency: 0.3 ms, bandwidth: 9.8 Gbps
```

### C3 — Групповые задачи (P1, ~2 дня)

Файлы: `waters-node/src/group.rs`, `waters-node/src/task.rs`

- [ ] `Task` может быть назначена группе, не только агенту
- [ ] Координатор группы распределяет подзадачи между агентами
- [ ] `findings` группы публикуются в общий Stream
- [ ] Cross-group: группа A видит findings группы B через Kafka

```bash
/group create meteorite-search
/task create "найти метеориты" group=meteorite-search
# → группа сама распределяет:
#   navigator → маршрут
#   drone-operator → полёт
#   geologist → анализ
```

### C4 — Чат-аппрув (P1, ~1 день)

Файлы: `waters-node/src/convo.rs`, `waters-node/src/handlers.rs`

- [ ] Входящая нода: запрос в чат → да/нет
- [ ] Входящий агент (Cargo): запрос → да/нет/режим
- [ ] Auto-approve для доверенных IP

```
🔔 Нода 192.168.1.5 запрашивает доступ
   > Принять? (да/нет)
```

### C5 — MCP-мосты под лабораторное оборудование (P2, ~2 дня)

Файлы: `waters-node/src/bridge.rs`, `waters-node/src/mcp.rs`

- [ ] Задокументировать формат bridges.json для:
  - LIBS-спектрометр → `mcp-libs`
  - Масс-спектрометр → `mcp-icp-ms`
  - Микроскоп SEM → `mcp-sem`
  - Рентген XRD → `mcp-xrd`
  - SAR-спутник → `mcp-sar`
  - Метеоритная коллекция → `mcp-meteorite-collection`
- [ ] healthcheck для каждого типа моста
- [ ] personal: true/false — кто видит мост

### C6 — Улучшения по LOG_20260516.md (P2, ~2 дня)

- [ ] SubAgent mailbox — события lifecycle для подписчиков
- [ ] Tamagotchi memory — личность хозяина в Redis
- [ ] Telegram bridge — команды + уведомления
- [ ] VoiceBridge — Whisper STT / TTS как bridge provider
- [ ] ratatui TUI — терминальный интерфейс (~500 строк)
- [ ] Файлообмен между нодами — HTTP file server

---

## Порядок выполнения

```
День 1-3:   C1 (DTN-модуль, 8 режимов)      + A1 (публикация docs)
День 4-5:   C2 (лазерный мост FSO)           + A2 (онтология LightRAG)
День 6-7:   C3 (групповые задачи)            + A3 (спецификации)
День 8:     C4 (чат-аппрув)                  + A4 (CHANGELOG)
День 9-10:  C5 (MCP-мосты для лабораторий)   + A3 (дописать specs)
День 11-12: C6 (улучшения из LOG)            + ревью всего
```

---

## Критерии готовности к следующему сеансу

- [ ] **DTN:** 8 режимов работают, автомат переключается по энергии
- [ ] **Лазер:** FSO-мост соединяет две ноды через лазер
- [ ] **Группы:** задача группе → распределение по агентам
- [ ] **Чат-аппрув:** входящая нода/агент → запрос в чат
- [ ] **MCP-мосты:** LIBS, SEM, ICP-MS как bridges.json
- [ ] **Платформа:** control_center.md + virtual_life_modes.md в git
- [ ] **Онтология:** граф миссии в LightRAG
- [ ] **README:** CHANGELOG.md, ссылки на docs/
