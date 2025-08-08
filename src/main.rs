use anyhow::{anyhow, Context, Result};
use bytes::BytesMut;
use clap::Parser;
use rustls::client::ClientConfig;
use rustls::pki_types::ServerName;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use url::Url;

const USER_AGENT: &str = "xray-tester/0.1";

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "xray-tester",
    version,
    about = "Send HTTP/HTTPS requests via Xray proxy and measure latency"
)]
struct Args {
    #[arg(short = 'p', long)]
    proxy: Option<String>,

    #[arg(short = 'u', long, value_name = "URL")]
    url: Option<String>,

    #[arg(short = 'n', long, default_value_t = 100)]
    iterations: usize,

    #[arg(short = 'c', long, default_value_t = 20)]
    concurrency: usize,

    #[arg(short = 't', long, default_value_t = 5000)]
    timeout_ms: u64,

    #[arg(short = 'k', long, default_value_t = true)]
    insecure: bool,

    #[command(subcommand)]
    cmd: Option<Commands>,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum Commands {
    Completions { #[arg(value_enum)] shell: Shell },
}

#[derive(Debug, Clone)]
struct Target {
    scheme: String,
    host: String,
    port: u16,
    path: String,
    host_header: String,
}

#[derive(Debug, Default, Clone)]
struct Stats {
    latencies_ms: Vec<u128>,
    success: usize,
    fail: usize,
    conn_errors: usize,
    timeout_errors: usize,
    tls_errors: usize,
    total_duration_ms: u128,
}

impl Stats {
    fn record_success(&mut self, dur: Duration) {
        self.success += 1;
        self.latencies_ms.push(dur.as_millis());
    }
    fn record_fail(&mut self) {
        self.fail += 1;
    }
    fn record_timeout(&mut self) {
        self.fail += 1;
        self.timeout_errors += 1;
    }
    fn record_conn_error(&mut self) {
        self.fail += 1;
        self.conn_errors += 1;
    }
    fn record_tls_error(&mut self) {
        self.fail += 1;
        self.tls_errors += 1;
    }

    fn percentile(&self, p: f64) -> Option<u128> {
        if self.latencies_ms.is_empty() {
            return None;
        }
        let mut v = self.latencies_ms.clone();
        let len = v.len();
        let rank = (((len as f64) * p).ceil() as usize)
            .saturating_sub(1)
            .min(len - 1);
        let (_, nth, _) = v.select_nth_unstable(rank);
        Some(*nth)
    }
    fn mean(&self) -> Option<f64> {
        if self.latencies_ms.is_empty() {
            return None;
        }
        Some(
            self.latencies_ms.iter().copied().sum::<u128>() as f64 / self.latencies_ms.len() as f64,
        )
    }
    fn stddev(&self) -> Option<f64> {
        if self.latencies_ms.len() < 2 {
            return Some(0.0);
        }
        let mean = self.mean().unwrap();
        let var = self
            .latencies_ms
            .iter()
            .map(|&x| {
                let dx = x as f64 - mean;
                dx * dx
            })
            .sum::<f64>()
            / (self.latencies_ms.len() as f64);
        Some(var.sqrt())
    }
    fn min(&self) -> Option<u128> {
        self.latencies_ms.iter().min().copied()
    }
    fn max(&self) -> Option<u128> {
        self.latencies_ms.iter().max().copied()
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(Commands::Completions { shell }) = args.cmd.clone() {
        let mut cmd = Args::command();
        generate(shell, &mut cmd, "xray-tester", &mut io::stdout());
        return Ok(());
    }
    let proxy_str = args
        .proxy
        .as_deref()
        .ok_or_else(|| anyhow!("--proxy is required"))?;
    let proxy = Url::parse(proxy_str).context("invalid proxy URL")?;
    if proxy.scheme() != "socks5" && proxy.scheme() != "http" {
        return Err(anyhow!("unsupported proxy scheme: {}", proxy.scheme()));
    }
    let proxy_host = proxy
        .host_str()
        .ok_or_else(|| anyhow!("proxy host missing"))?
        .to_string();
    let proxy_port = proxy.port().unwrap_or(2080);

    let url_str = args
        .url
        .as_deref()
        .ok_or_else(|| anyhow!("--url is required"))?;
    let target = parse_url_target(url_str)?;

    let client_config = Arc::new(build_client_config(args.insecure)?);

    println!("Proxy: {}://{}:{}", proxy.scheme(), proxy_host, proxy_port);
    println!(
        "Target: {}://{}:{}{}",
        target.scheme, target.host, target.port, target.path
    );
    println!(
        "iterations: {} concurrency: {} timeout: {}ms insecure: {}",
        args.iterations, args.concurrency, args.timeout_ms, args.insecure
    );

    let stats = run_bench(
        &proxy,
        &proxy_host,
        proxy_port,
        &target,
        client_config.clone(),
        args.iterations,
        args.concurrency,
        Duration::from_millis(args.timeout_ms),
    )
    .await?;

    println!("\n=== XRAY-CHECKER Results ===");
    println!(
        "Duration: {:.2}s",
        (stats.total_duration_ms as f64) / 1000.0
    );
    println!("Total requests: {}", args.iterations);
    println!(
        "Successful: {} ({:.2}%)",
        stats.success,
        (stats.success as f64) * 100.0 / (args.iterations as f64)
    );
    println!("Failed: {}", stats.fail);
    println!("Connection errors: {}", stats.conn_errors);
    println!("Timeout errors: {}", stats.timeout_errors);
    println!("TLS errors: {}", stats.tls_errors);
    let rps = if stats.total_duration_ms == 0 {
        0.0
    } else {
        (args.iterations as f64) / (stats.total_duration_ms as f64 / 1000.0)
    };
    println!("Requests per second: {:.2}", rps);

    println!("\n=== Latency Statistics ===");
    println!(
        "Min: {}ms",
        stats.min().map(|v| v as f64 / 1.0).unwrap_or(0.0)
    );
    println!(
        "Max: {}ms",
        stats.max().map(|v| v as f64 / 1.0).unwrap_or(0.0)
    );
    println!("Mean: {:.2}ms", stats.mean().unwrap_or(0.0));
    println!(
        "Median (P50): {:.2}ms",
        stats.percentile(0.50).map(|v| v as f64).unwrap_or(0.0)
    );
    println!(
        "P90: {:.2}ms",
        stats.percentile(0.90).map(|v| v as f64).unwrap_or(0.0)
    );
    println!(
        "P95: {:.2}ms",
        stats.percentile(0.95).map(|v| v as f64).unwrap_or(0.0)
    );
    println!(
        "P99: {:.2}ms",
        stats.percentile(0.99).map(|v| v as f64).unwrap_or(0.0)
    );
    println!(
        "P99.9: {:.2}ms",
        stats.percentile(0.999).map(|v| v as f64).unwrap_or(0.0)
    );
    println!("StdDev: {:.2}ms", stats.stddev().unwrap_or(0.0));
    Ok(())
}

fn parse_url_target(s: &str) -> Result<Target> {
    let url = Url::parse(s).context("invalid URL")?;
    let scheme = url.scheme().to_string();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("unsupported URL scheme: {}", scheme));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("missing host in URL"))?
        .to_string();
    let port = url
        .port()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });
    let mut path = url.path().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    if let Some(q) = url.query() {
        path.push('?');
        path.push_str(q);
    }
    let host_header = if let Some(p) = url.port() {
        format!("{}:{}", host, p)
    } else {
        host.clone()
    };
    Ok(Target {
        scheme,
        host,
        port,
        path,
        host_header,
    })
}

fn build_client_config(insecure: bool) -> Result<ClientConfig> {
    let mut cfg = if insecure {
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth()
    } else {
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth()
    };
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(cfg)
}

#[derive(Debug)]
struct NoVerifier;
impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

async fn run_bench(
    proxy: &Url,
    proxy_host: &str,
    proxy_port: u16,
    target: &Target,
    client_config: Arc<ClientConfig>,
    iterations: usize,
    concurrency: usize,
    per_timeout: Duration,
) -> Result<Stats> {
    let started = Instant::now();
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let proxy = proxy.clone();
        let target = target.clone();
        let client_config = client_config.clone();
        let proxy_host = proxy_host.to_string();
        let proxy_port_captured = proxy_port;
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            timeout(
                per_timeout,
                single_request(
                    &proxy,
                    &proxy_host,
                    proxy_port_captured,
                    &target,
                    client_config,
                ),
            )
            .await
            .ok()
        }));
    }

    let mut stats = Stats::default();
    for h in handles {
        match h.await {
            Ok(Some(Ok(Some(dur)))) => stats.record_success(dur),
            Ok(Some(Ok(None))) => stats.record_fail(),
            Ok(Some(Err(e))) => {
                if e.to_string().contains("timed out") {
                    stats.record_timeout();
                } else if e.to_string().contains("certificate") || e.to_string().contains("TLS") {
                    stats.record_tls_error();
                } else {
                    stats.record_conn_error();
                }
            }
            Ok(None) => stats.record_timeout(),
            Err(_) => stats.record_fail(),
        }
    }
    stats.total_duration_ms = started.elapsed().as_millis();
    Ok(stats)
}

async fn single_request(
    proxy: &Url,
    proxy_host: &str,
    proxy_port: u16,
    target: &Target,
    client_config: Arc<ClientConfig>,
) -> Result<Option<Duration>> {
    let start = Instant::now();
    let mut tcp = TcpStream::connect((proxy_host, proxy_port)).await?;

    match (proxy.scheme(), target.scheme.as_str()) {
        ("socks5", "https") => {
            drop(tcp);
            let socks = tokio_socks::tcp::Socks5Stream::connect(
                (proxy_host, proxy_port),
                (target.host.as_str(), target.port),
            )
            .await
            .context("socks5 connect failed")?;
            let connector = TlsConnector::from(client_config);
            let server_name =
                ServerName::try_from(target.host.clone()).map_err(|_| anyhow!("invalid SNI"))?;
            let mut tls = connector.connect(server_name, socks).await?;
            let req = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
                target.path, target.host_header, USER_AGENT
            );
            tls.write_all(req.as_bytes()).await?;
            let mut buf = BytesMut::with_capacity(1024);
            let mut tmp = [0u8; 1024];
            let success = loop {
                let n = tls.read(&mut tmp).await?;
                if n == 0 {
                    break false;
                }
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break true;
                }
            };
            if !success {
                return Ok(None);
            }
            Ok(Some(start.elapsed()))
        }
        ("http", "https") => {
            let connect_req = format!(
                "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\nProxy-Connection: Keep-Alive\r\n\r\n",
                target.host, target.port, target.host, target.port
            );
            tcp.write_all(connect_req.as_bytes()).await?;
            let mut buf = BytesMut::with_capacity(512);
            let mut tmp = [0u8; 512];
            let status_ok = loop {
                let n = tcp.read(&mut tmp).await?;
                if n == 0 {
                    break false;
                }
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let mut headers = [httparse::EMPTY_HEADER; 32];
                    let mut resp = httparse::Response::new(&mut headers);
                    let _ = resp.parse(&buf)?;
                    if let Some(code) = resp.code {
                        break code == 200;
                    } else {
                        break false;
                    }
                }
            };
            if !status_ok {
                return Ok(None);
            }
            let connector = TlsConnector::from(client_config);
            let server_name =
                ServerName::try_from(target.host.clone()).map_err(|_| anyhow!("invalid SNI"))?;
            let mut tls = connector.connect(server_name, tcp).await?;
            let req = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
                target.path, target.host_header, USER_AGENT
            );
            tls.write_all(req.as_bytes()).await?;
            let mut buf = BytesMut::with_capacity(1024);
            let mut tmp = [0u8; 1024];
            let success = loop {
                let n = tls.read(&mut tmp).await?;
                if n == 0 {
                    break false;
                }
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break true;
                }
            };
            if !success {
                return Ok(None);
            }
            Ok(Some(start.elapsed()))
        }
        ("socks5", "http") => {
            drop(tcp);
            let stream = tokio_socks::tcp::Socks5Stream::connect(
                (proxy_host, proxy_port),
                (target.host.as_str(), target.port),
            )
            .await
            .context("socks5 connect failed")?;
            perform_http_request_plain(stream, &target.host_header, &target.path, start).await
        }
        ("http", "http") => perform_http_via_http_proxy(tcp, target, start).await,
        (other_proxy, other_scheme) => Err(anyhow!(
            "unsupported combo: proxy={} scheme={}",
            other_proxy,
            other_scheme
        )),
    }
}

async fn perform_http_request_plain<S>(
    mut stream: S,
    host_header: &str,
    path: &str,
    start: Instant,
) -> Result<Option<Duration>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        path, host_header, USER_AGENT
    );
    stream.write_all(req.as_bytes()).await?;
    let mut buf = BytesMut::with_capacity(1024);
    let mut tmp = [0u8; 1024];
    let success = loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break false;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break true;
        }
    };
    if !success {
        return Ok(None);
    }
    Ok(Some(start.elapsed()))
}

async fn perform_http_via_http_proxy(
    mut proxy_tcp: TcpStream,
    target: &Target,
    start: Instant,
) -> Result<Option<Duration>> {
    let url_line = if (target.scheme == "http" && target.port == 80)
        || (target.scheme == "https" && target.port == 443)
    {
        format!("{}://{}{}", target.scheme, target.host, target.path)
    } else {
        format!(
            "{}://{}:{}{}",
            target.scheme, target.host, target.port, target.path
        )
    };
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        url_line, target.host_header, USER_AGENT
    );
    proxy_tcp.write_all(req.as_bytes()).await?;
    let mut buf = BytesMut::with_capacity(1024);
    let mut tmp = [0u8; 1024];
    let success = loop {
        let n = proxy_tcp.read(&mut tmp).await?;
        if n == 0 {
            break false;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break true;
        }
    };
    if !success {
        return Ok(None);
    }
    Ok(Some(start.elapsed()))
}
