# xray-tester

Утилита для измерения задержек HTTP/HTTPS запросов через прокси (SOCKS5 и HTTP). Поддерживает HTTPS через HTTP-прокси (CONNECT). Параллельно выполняет запросы, считает успешные/неуспешные, строит статистику задержек (min/max/percentile/avg/mean/stddev) и RPS.

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
  --timeout 5000 \
  # --insecure \ (необязательно) отключить проверку TLS
  # --success-codes 200-399,418 \ (необязательно) переопределить успешные коды (по умолчанию 200-399)
  # --connect-to localhost:9999 \ (необязательно) переопределить адрес назначения для CONNECT-туннеля через HTTP-прокси
```

Параметры:
- `--proxy` — URL прокси: `socks5://host:port` или `http://host:port`.
- `--url` — целевой URL `http` или `https`.
- `--iterations` — количество запросов.
- `--concurrency` — параллелизм.
- `--timeout` — таймаут на один запрос в миллисекундах.
- `--insecure` — отключить проверку TLS.
- `--success-codes <CODES>` — через запятую перечисление HTTP-кодов и/или диапазонов, считающихся успешными. Примеры: `200-399,418`, `200,204,301-302`. По умолчанию: `200-399`.
- `--connect-to <HOST:PORT>` — переопределяет адрес назначения для CONNECT-туннеля через HTTP-прокси, при этом SNI и заголовок Host берутся из исходного URL.

### Пример с пользовательскими успешными кодами:

```bash
xray-tester \
  --proxy http://127.0.0.1:2081 \
  --url https://localhost:9999 \
  --iterations 100 \
  --concurrency 2 \
  --timeout 5000 \
  --success-codes 200-399,418 \
  --insecure
```

### Пример вывода:

```
Proxy: http://127.0.0.1:2081
Target: https://localhost:9999/
Iterations: 100 Concurrency: 2 Timeout: 5000ms Insecure: true Debug: false

Statistics        Avg        Median        Stdev         Max
  Reqs/sec        87.26      50.00      52.33        87.00
  Latency          1.53ms      942µs     2.31ms        14.37ms

  Latency Distribution
     50%       927µs
     75%      1.83ms
     90%      1.91ms
     95%      2.24ms
     99%     14.34ms
  HTTP codes:
    1xx - 0, 2xx - 100, 3xx - 0, 4xx - 0, 5xx - 0

Results
  Total requests: 100
  Success: 100 (100.00%)  Fail: 0

StdDev: 2.31ms
```


## Дополнительно

Генерация автодополнений для shell:

```bash
# bash
xray-tester completions bash > /etc/bash_completion.d/xray-tester

# zsh
xray-tester completions zsh > "$fpath[1]/_xray-tester"

# fish
xray-tester completions fish > ~/.config/fish/completions/xray-tester.fish

# powershell
xray-tester completions powershell > xray-tester.ps1
```

## Лицензия

MIT OR Apache-2.0
