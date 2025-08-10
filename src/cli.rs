use clap::Parser;
use clap_complete::Shell;

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Commands {
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "xray-tester",
    version,
    about = "Send HTTP/HTTPS requests via Xray proxy and measure latency"
)]
pub struct Args {
    #[arg(short = 'p', long)]
    pub proxy: Option<String>,

    #[arg(short = 'u', long, value_name = "URL")]
    pub url: Option<String>,

    #[arg(short = 'n', long, default_value_t = 100)]
    pub iterations: usize,

    #[arg(short = 'c', long, default_value_t = 20)]
    pub concurrency: usize,

    #[arg(short = 't', long = "timeout", default_value_t = 5000)]
    pub timeout_ms: u64,

    #[arg(short = 'k', long, action = clap::ArgAction::SetTrue)]
    pub insecure: bool,

    #[arg(
        long = "success-codes",
        value_name = "CODES",
        help = "Comma-separated list of HTTP codes and/or ranges considered success, e.g. '200-399,418'. Default: 200-400"
    )]
    pub success_codes: Option<String>,

    #[arg(
        long = "connect-to",
        value_name = "HOST:PORT",
        help = "Override proxy CONNECT destination while keeping original URL host for SNI/Host"
    )]
    pub connect_to: Option<String>,

    #[arg(long = "debug", action = clap::ArgAction::SetTrue)]
    pub debug: bool,

    #[command(subcommand)]
    pub cmd: Option<Commands>,
}

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct SuccessMatcher {
    pub ranges: Vec<(u16, u16)>,
}

impl SuccessMatcher {
    pub fn default() -> Self {
        Self {
            ranges: vec![(200, 400)],
        }
    }
    pub fn parse(spec: &str) -> Result<Self> {
        let mut ranges = Vec::new();
        for part in spec.split(',') {
            let t = part.trim();
            if t.is_empty() {
                continue;
            }
            if let Some((a, b)) = t.split_once('-') {
                let start: u16 = a.trim().parse().context("invalid start code in range")?;
                let end: u16 = b.trim().parse().context("invalid end code in range")?;
                if start > end {
                    return Err(anyhow!("invalid range: {}", t));
                }
                if end > 599 {
                    return Err(anyhow!("status code out of range: {}", end));
                }
                ranges.push((start, end));
            } else {
                let code: u16 = t.parse().context("invalid status code")?;
                if code > 599 {
                    return Err(anyhow!("status code out of range: {}", code));
                }
                ranges.push((code, code));
            }
        }
        if ranges.is_empty() {
            return Err(anyhow!("empty success-codes specification"));
        }
        Ok(Self { ranges })
    }
    pub fn contains(&self, code: u16) -> bool {
        self.ranges.iter().any(|&(s, e)| code >= s && code <= e)
    }
}
