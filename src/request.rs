use anyhow::{anyhow, Context, Result};
use hyper::client::conn;
use hyper::Request;
use hyper::http::Uri;
use bytes::Bytes;
use http_body_util::Empty;
use hyper_util::rt::TokioIo;
use native_tls::TlsConnector as NativeTlsConnector;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio_native_tls::TlsConnector as TokioTlsConnector;
use url::Url;
use futures::stream::{FuturesUnordered, StreamExt};

use crate::cli::SuccessMatcher;
use crate::stats::Stats;

pub const USER_AGENT: &str = "xray-tester/0.1";

pub fn parse_url_target(url_str: &str) -> Result<Target> {
    let url = Url::parse(url_str).context("invalid target URL")?;
    let scheme = url.scheme().to_string();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("unsupported URL scheme: {}", scheme));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("target host missing"))?
        .to_string();
    let port = match (url.port(), scheme.as_str()) {
        (Some(p), _) => p,
        (None, "http") => 80,
        (None, "https") => 443,
        _ => 0,
    };
    let path = if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    };
    let host_header = if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
        host.clone()
    } else {
        format!("{}:{}", host, port)
    };
    Ok(Target {
        scheme,
        host,
        port,
        path,
        host_header,
    })
}

#[derive(Debug, Clone)]
pub struct Target {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub host_header: String,
}

#[derive(Debug, Clone)]
pub struct RespMeta {
    pub success: bool,
    pub dur: Option<Duration>,
    pub status: Option<u16>,
    pub finished: Instant,
}

pub async fn run_bench(
    proxy: Arc<Url>,
    _proxy_host: &str,
    _proxy_port: u16,
    target: Arc<Target>,
    success_matcher: Arc<SuccessMatcher>,
    iterations: usize,
    concurrency: usize,
    per_timeout: Duration,
    insecure: bool,
    debug: bool,
    connect_to: Option<String>,
) -> Result<Stats> {
    let started = Instant::now();
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut futs = FuturesUnordered::new();
    for _ in 0..iterations {
        let sem = sem.clone();
        let proxy = proxy.clone();
        let target = target.clone();
        let success_matcher = success_matcher.clone();
        let connect_to_inner = connect_to.clone();
        let insecure_local = insecure;
        futs.push(tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            single_request(
                &proxy,
                &target,
                success_matcher,
                insecure_local,
                &connect_to_inner,
                per_timeout,
            )
            .await
        }));
    }

    let mut stats = Stats::default();
    while let Some(join_res) = futs.next().await {
        match join_res {
            Ok(Ok(meta)) => {
                let sec = meta.finished.duration_since(started).as_secs();
                stats.record_success_bucket(sec);
                if let Some(code) = meta.status {
                    stats.record_status(code);
                }
                if meta.success {
                    if let Some(dur) = meta.dur {
                        stats.record_success(dur);
                    } else {
                        stats.record_success(Duration::from_millis(0));
                    }
                } else {
                    if let Some(code) = meta.status {
                        if debug {
                            eprintln!("[xray-tester] Response status {} not in success set; counted as fail. Consider --success-codes", code);
                        }
                    } else {
                        if debug {
                            eprintln!("[xray-tester] Request completed without parsable status; counted as fail");
                        }
                    }
                    stats.record_fail();
                }
            }
            Ok(Err(e)) => {
                let sec = started.elapsed().as_secs();
                stats.record_success_bucket(sec);
                if e.to_string().contains("timed out") {
                    stats.record_timeout();
                } else if e.to_string().contains("certificate") || e.to_string().contains("TLS") {
                    stats.record_tls_error();
                } else {
                    stats.record_conn_error();
                }
                if debug {
                    eprintln!("[xray-tester] Request error: {}", e);
                }
            }
            Err(_) => {
                let sec = started.elapsed().as_secs();
                stats.record_success_bucket(sec);
                stats.record_fail();
                if debug {
                    eprintln!("[xray-tester] Internal join error");
                }
            }
        }
    }
    stats.total_duration_ms = started.elapsed().as_millis();
    Ok(stats)
}

async fn single_request(
    proxy: &Url,
    target: &Target,
    success_matcher: Arc<SuccessMatcher>,
    insecure: bool,
    connect_to: &Option<String>,
    timeout_dur: Duration,
) -> Result<RespMeta> {
    let connect_target = if let Some(ct) = connect_to {
        ct.clone()
    } else {
        format!("{}:{}", target.host, target.port)
    };

    let proxy_addr = format!(
        "{}:{}",
        proxy.host_str().unwrap_or("127.0.0.1"),
        proxy.port_or_known_default().unwrap_or(80)
    );
    let mut stream = tokio::time::timeout(timeout_dur, TcpStream::connect(&proxy_addr))
        .await
        .map_err(|_| anyhow!("connect to proxy {} timed out", proxy_addr))?
        .with_context(|| format!("connect to proxy {} failed", proxy_addr))?;

    let connect_req = format!(
        "CONNECT {} HTTP/1.1\r\nHost: {}\r\nProxy-Connection: Keep-Alive\r\n\r\n",
        connect_target, connect_target
    );
    let write_res = tokio::time::timeout(timeout_dur, stream.write_all(connect_req.as_bytes()))
        .await
        .map_err(|_| anyhow!("proxy CONNECT write timed out"))?;
    write_res?;

    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 512];
    loop {
        let read_res = tokio::time::timeout(timeout_dur, stream.read(&mut tmp))
            .await
            .map_err(|_| anyhow!("proxy CONNECT read timed out"))?;
        let n = read_res?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() >= 4 {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = &buf[..pos];
                let ok = head.windows(12).any(|w| w == b" 200 Connection")
                    || head.starts_with(b"HTTP/1.1 200");
                if !ok {
                    return Err(anyhow!(
                        "proxy CONNECT failed: {}",
                        String::from_utf8_lossy(head)
                    ));
                }
                break;
            }
        }
        if buf.len() > 8192 {
            return Err(anyhow!("proxy CONNECT response too large"));
        }
    }

    let host_for_sni = &target.host;
    if target.scheme == "https" {
        let mut tls_builder = NativeTlsConnector::builder();
        if insecure {
            tls_builder.danger_accept_invalid_certs(true);
            tls_builder.danger_accept_invalid_hostnames(true);
        }
        let tls = tls_builder.build().context("building TLS connector")?;
        let tls = TokioTlsConnector::from(tls);
        let dns_name = host_for_sni;
        let tls_stream = tokio::time::timeout(timeout_dur, tls.connect(dns_name, stream))
            .await
            .map_err(|_| anyhow!("TLS connect timed out"))??;
        let io = TokioIo::new(tls_stream);
        let (mut sender, connection) =
            tokio::time::timeout(timeout_dur, conn::http1::handshake(io))
                .await
                .map_err(|_| anyhow!("handshake timed out"))??;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let path = if target.path.is_empty() {
            "/"
        } else {
            &target.path
        };
        let uri: Uri = path.parse().context("invalid request path")?;
        let req = Request::get(uri)
            .header("Host", &target.host_header)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "*/*")
            .header("Connection", "close")
            .body(Empty::<Bytes>::new())
            .map_err(|e| anyhow!("build request failed: {e}"))?;

        let start = Instant::now();
        let resp = tokio::time::timeout(timeout_dur, sender.send_request(req))
            .await
            .map_err(|_| anyhow!("request timed out"))?
            .map_err(|e| anyhow!("request failed: {e:?}"))?;
        let status = resp.status().as_u16();
        let success = success_matcher.contains(status);
        let dur = Some(start.elapsed());
        return Ok(RespMeta {
            success,
            dur,
            status: Some(status),
            finished: Instant::now(),
        });
    } else {
        let io = TokioIo::new(stream);
        let (mut sender, connection) =
            tokio::time::timeout(timeout_dur, conn::http1::handshake(io))
                .await
                .map_err(|_| anyhow!("handshake timed out"))??;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let path = if target.path.is_empty() {
            "/"
        } else {
            &target.path
        };
        let uri: Uri = path.parse().context("invalid request path")?;
        let req = Request::get(uri)
            .header("Host", &target.host_header)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "*/*")
            .header("Connection", "close")
            .body(Empty::<Bytes>::new())
            .map_err(|e| anyhow!("build request failed: {e}"))?;
        let start = Instant::now();
        let resp = tokio::time::timeout(timeout_dur, sender.send_request(req))
            .await
            .map_err(|_| anyhow!("request timed out"))?
            .map_err(|e| anyhow!("request failed: {e:?}"))?;
        let status = resp.status().as_u16();
        let success = success_matcher.contains(status);
        let dur = Some(start.elapsed());
        return Ok(RespMeta {
            success,
            dur,
            status: Some(status),
            finished: Instant::now(),
        });
    }
}
