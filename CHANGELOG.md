# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic Versioning.

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

### Security
- TLS реализован на базе `rustls` 0.23 (crypto-провайдер `aws-lc-rs`) с возможностью отключения проверки сертификата для тестов.

### Requirements
- Требуется Rust ≥ 1.74 (edition 2021).

[0.1.0]: https://github.com/vffuunnyy/xray-tester/releases/tag/v0.1.0
