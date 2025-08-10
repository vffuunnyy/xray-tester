# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic Versioning.

## [0.1.1] - 2025-08-10

### Added
- Поддержка HTTPS через туннель HTTP CONNECT (через HTTP-прокси).
- Конфигурируемые успешные коды ответа HTTP через `--success-codes` (список/диапазоны), по умолчанию `200-399`.
- Улучшенный вывод статистики: распределение задержек по перцентилям и распределение HTTP-кодов.
- Флаг `--connect-to` для переопределения адреса назначения для CONNECT-туннеля через HTTP-прокси, при этом SNI и заголовок Host берутся из исходного URL.

### Changed
- Код разбит на модули: `cli.rs`, `request.rs`, `stats.rs`, `pretty.rs`, `main.rs`.
- Конкурентное выполнение переписано на `FuturesUnordered` + семафор Tokio: ниже пиковое потребление памяти и стабильнее сбор результатов.
- Снижен объём клонов: в задачи передаются `Arc<Url>` и `Arc<Target>`.
- Удалён внешний таймаут-обёртка в `run_bench`; остаются точечные таймауты на фазы (CONNECT/TLS/handshake/request).
- SNI теперь берётся напрямую из `target.host` (без парсинга из Host-заголовка).
- Сборка HTTP-запросов больше не использует `unwrap()`, ошибки аккуратно прокидываются.
- Для сортировок вещественных значений применяется `total_cmp`.
- Уточнена точность измерения задержек: внутри статистики задержки хранятся в микросекундах (µs), что исключает схлопывание суб‑миллисекундных значений в 0.

### Fixed
- Исправлены ошибки HTTPS-запросов через HTTP-прокси (ранее не проходили без `--insecure`).
- Корректное распознавание HTTP статусов в HTTPS-сценариях через прокси.

### Docs
- README обновлён: добавлен флаг `--connect-to`, исправлен заголовок столбца на `Median`.
- Форматирование задержек: значения < 1ms в выводе автоматически отображаются в микросекундах (µs) для лучшей читаемости малых величин.

### Internal
- Добавлена зависимость `futures = 0.3`.

## [0.1.0] - 2025-08-08

### Added
- Первая публичная версия утилиты `xray-tester`.
- Измерение задержек HTTP/HTTPS запросов через прокси (SOCKS5 и HTTP).
- Параллельное выполнение запросов с настраиваемой конкуренцией.
- Подсчет успешных/неуспешных запросов и построение статистики задержек:
  - min / max / mean / stddev
  - произвольные перцентили
- Подсчет RPS.
- Настраиваемые параметры:
  - `--proxy` (`socks5://` или `http://`)
  - `--url` (HTTP/HTTPS)
  - `--iterations`, `--concurrency`, `--timeout-ms`
  - `--insecure` для отключения проверки TLS (для тестирования)
- Генерация shell completion для `bash`, `zsh`, `fish`, `powershell` через `--completion`.
- Сборка релизных бинарников для Linux, macOS и Windows через GitHub Actions с автоматической загрузкой в релиз.

### Requirements
- Требуется Rust ≥ 1.74 (edition 2021).

[0.1.0]: https://github.com/vffuunnyy/xray-tester/releases/tag/v0.1.0
[0.1.1]: https://github.com/vffuunnyy/xray-tester/releases/tag/v0.1.1
[0.1.2]: https://github.com/vffuunnyy/xray-tester/releases/tag/v0.1.2
