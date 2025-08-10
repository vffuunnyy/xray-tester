use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use clap::CommandFactory;
use clap_complete::generate;
use std::io;
mod cli;
mod pretty;
mod request;
mod stats;
use crate::cli::{Args, Commands, SuccessMatcher};
use crate::request::{parse_url_target, run_bench};

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
    if !matches!(proxy.scheme(), "socks5" | "http") {
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

    println!("Proxy: {}://{}:{}", proxy.scheme(), proxy_host, proxy_port);
    println!(
        "Target: {}://{}:{}{}",
        target.scheme, target.host, target.port, target.path
    );
    println!(
        "Iterations: {} Concurrency: {} Timeout: {}ms Insecure: {} Debug: {}",
        args.iterations, args.concurrency, args.timeout_ms, args.insecure, args.debug
    );

    let success_matcher = if let Some(spec) = args.success_codes.as_deref() {
        SuccessMatcher::parse(spec)?
    } else {
        SuccessMatcher::default()
    };

    let stats = run_bench(
        Arc::new(proxy),
        &proxy_host,
        proxy_port,
        Arc::new(target),
        Arc::new(success_matcher),
        args.iterations,
        args.concurrency,
        Duration::from_millis(args.timeout_ms),
        args.insecure,
        args.debug,
        args.connect_to,
    )
    .await?;

    pretty::print_results(&stats, args.iterations);
    Ok(())
}
