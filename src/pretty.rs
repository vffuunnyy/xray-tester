use crate::stats::Stats;

fn fmt_ms_w(ms: f64, width: usize) -> String {
    if ms.is_finite() && ms < 1.0 {
        let mut us = (ms * 1000.0).round();
        if us == 0.0 && ms > 0.0 {
            us = 1.0;
        }
        format!("{:>width$}", format!("{:.0}µs", us), width = width)
    } else if ms.is_finite() {
        format!("{:>width$}", format!("{:.2}ms", ms), width = width)
    } else {
        format!("{:>width$}", "-", width = width)
    }
}

pub fn print_results(stats: &Stats, iterations: usize) {
    println!("\nStatistics        Avg        Median        Stdev         Max");
    let rps = stats.rps_avg().unwrap_or(0.0);
    println!(
        "  Reqs/sec   {:>10.2}   {:>8.2}   {:>8.2}   {:>10.2}",
        rps,
        stats.rps_median().unwrap_or(0.0),
        stats.rps_stddev().unwrap_or(0.0),
        stats.rps_max().unwrap_or(0.0)
    );
    println!(
        "  Latency    {} {} {}   {}",
        fmt_ms_w(stats.latency_avg().unwrap_or(0.0), 12),
        fmt_ms_w(stats.latency_median().unwrap_or(0.0), 10),
        fmt_ms_w(stats.latency_stddev().unwrap_or(0.0), 10),
        fmt_ms_w(stats.latency_max().map(|v| v as f64).unwrap_or(0.0), 12)
    );

    println!("\n  Latency Distribution");
    println!(
        "     50%  {}",
        fmt_ms_w(stats.latency_percentile(0.50).unwrap_or(0.0), 10)
    );
    println!(
        "     75%  {}",
        fmt_ms_w(stats.latency_percentile(0.75).unwrap_or(0.0), 10)
    );
    println!(
        "     90%  {}",
        fmt_ms_w(stats.latency_percentile(0.90).unwrap_or(0.0), 10)
    );
    println!(
        "     95%  {}",
        fmt_ms_w(stats.latency_percentile(0.95).unwrap_or(0.0), 10)
    );
    println!(
        "     99%  {}",
        fmt_ms_w(stats.latency_percentile(0.99).unwrap_or(0.0), 10)
    );

    let (mut c1, mut c2, mut c3, mut c4, mut c5, mut other) = (0, 0, 0, 0, 0, 0);
    for (&code, &count) in &stats.status_counts {
        match code / 100 {
            1 => c1 += count,
            2 => c2 += count,
            3 => c3 += count,
            4 => c4 += count,
            5 => c5 += count,
            _ => other += count,
        }
    }
    println!("  HTTP codes:");
    println!(
        "    1xx - {}, 2xx - {}, 3xx - {}, 4xx - {}, 5xx - {}",
        c1, c2, c3, c4, c5
    );
    if other > 0 {
        println!("    others - {}", other);
    }

    println!("\nResults");
    println!("  Total requests: {}", iterations);
    println!(
        "  Success: {} ({:.2}%)  Fail: {}",
        stats.success,
        (stats.success as f64) * 100.0 / (iterations as f64),
        stats.fail
    );
    println!(
        "\nStdDev: {}",
        fmt_ms_w(stats.latency_stddev().unwrap_or(0.0), 0)
    );
}
