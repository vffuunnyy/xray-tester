# xray-tester

Утилита для измерения задержек HTTP/HTTPS запросов через прокси (SOCKS5 и HTTP). Параллельно выполняет запросы, считает успешные/неуспешные, строит простую статистику задержек (min/max/percentile/mean/stddev) и RPS.

## Установка

Требуется Rust >= 1.74 (edition 2021).

```bash
cargo build --release
```

## Использование

```bash
xray-tester \
  --proxy socks5://127.0.0.1:2080 \
  --url https://example.com/ \
  --iterations 100 \
  --concurrency 20 \
  --timeout-ms 5000 \
  --insecure
```

Дополнительные параметры:
- `--completion` — генерация shell completion для `bash`, `zsh`, `fish`, `powershell` (см. ).

Параметры:
- `--proxy` — URL прокси: `socks5://host:port` или `http://host:port`.
- `--url` — целевой URL `http` или `https`.
- `--iterations` — количество запросов.
- `--concurrency` — параллелизм.
- `--timeout-ms` — таймаут на один запрос в миллисекундах.
- `--insecure` — отключить проверку TLS.

## Лицензия

MIT OR Apache-2.0
