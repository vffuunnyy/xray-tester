use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub latencies_us: Vec<u128>,
    pub success: usize,
    pub fail: usize,
    pub conn_errors: usize,
    pub timeout_errors: usize,
    pub tls_errors: usize,
    pub total_duration_ms: u128,
    pub status_counts: BTreeMap<u16, usize>,
    pub rps_secs: BTreeMap<u64, u32>,
}

impl Stats {
    pub fn record_success(&mut self, dur: Duration) {
        self.success += 1;
        self.latencies_us.push(dur.as_micros());
    }

    pub fn record_fail(&mut self) {
        self.fail += 1;
    }

    pub fn record_status(&mut self, code: u16) {
        *self.status_counts.entry(code).or_insert(0) += 1;
    }

    pub fn record_timeout(&mut self) {
        self.fail += 1;
        self.timeout_errors += 1;
    }

    pub fn record_conn_error(&mut self) {
        self.fail += 1;
        self.conn_errors += 1;
    }

    pub fn record_tls_error(&mut self) {
        self.fail += 1;
        self.tls_errors += 1;
    }

    pub fn record_success_bucket(&mut self, sec: u64) {
        *self.rps_secs.entry(sec).or_insert(0) += 1;
    }

    // === Latency ===

    pub fn latency_percentile(&self, p: f64) -> Option<f64> {
        if self.latencies_us.is_empty() {
            return None;
        }
        let mut v = self.latencies_us.clone();
        v.sort_unstable();
        let idx = ((v.len() as f64) * p).ceil() as usize;
        let idx = idx.saturating_sub(1).min(v.len() - 1);
        Some(v[idx] as f64 / 1000.0)
    }

    pub fn latency_avg(&self) -> Option<f64> {
        if self.latencies_us.is_empty() {
            return None;
        }
        let sum_us: u128 = self.latencies_us.iter().copied().sum();
        Some((sum_us as f64) / 1000.0 / (self.latencies_us.len() as f64))
    }

    pub fn latency_median(&self) -> Option<f64> {
        let mut samples: Vec<f64> = self
            .latencies_us
            .iter()
            .map(|&x| (x as f64) / 1000.0)
            .collect();
        if samples.is_empty() {
            return None;
        }
        samples.retain(|x| !x.is_nan());
        if samples.is_empty() {
            return None;
        }
        samples.sort_by(|a, b| a.total_cmp(b));
        let n = samples.len();
        if n % 2 == 1 {
            Some(samples[n / 2])
        } else {
            Some((samples[n / 2 - 1] + samples[n / 2]) / 2.0)
        }
    }

    pub fn latency_stddev(&self) -> Option<f64> {
        if self.latencies_us.len() < 2 {
            return None;
        }
        let mean = self.latency_avg()?;
        let var = self
            .latencies_us
            .iter()
            .map(|&x| {
                let d = (x as f64) / 1000.0 - mean;
                d * d
            })
            .sum::<f64>()
            / (self.latencies_us.len() as f64 - 1.0);
        Some(var.sqrt())
    }

    // pub fn latency_min(&self) -> Option<u128> {
    //     self.latencies_ms.iter().copied().reduce(u128::min)
    // }

    pub fn latency_max(&self) -> Option<f64> {
        self.latencies_us
            .iter()
            .copied()
            .reduce(u128::max)
            .map(|us| us as f64 / 1000.0)
    }

    // === RPS ===

    fn rps_series(&self) -> Option<Vec<f64>> {
        if self.rps_secs.is_empty() {
            return None;
        }
        let &last_sec = self.rps_secs.keys().last().unwrap();
        let mut series = vec![0.0f64; (last_sec as usize) + 1];
        for (sec, cnt) in &self.rps_secs {
            if let Some(slot) = series.get_mut(*sec as usize) {
                *slot = *cnt as f64;
            }
        }
        Some(series)
    }

    pub fn rps_avg(&self) -> Option<f64> {
        if self.total_duration_ms == 0 || self.success == 0 {
            return None;
        }
        Some((self.success as f64) / (self.total_duration_ms as f64 / 1000.0))
    }

    pub fn rps_median(&self) -> Option<f64> {
        let mut s = self.rps_series()?;
        if s.is_empty() { return None; }
        s.sort_by(|a,b| a.total_cmp(b));
        let n = s.len();
        if n % 2 == 1 { Some(s[n/2]) } else { Some((s[n/2 - 1] + s[n/2]) / 2.0) }
    }

    pub fn rps_stddev(&self) -> Option<f64> {
        let s = self.rps_series()?;
        if s.len() < 2 { return None; }
        let mean = s.iter().sum::<f64>() / (s.len() as f64);
        let var = s.iter().map(|&x| { let d = x - mean; d*d }).sum::<f64>() / (s.len() as f64 - 1.0);
        Some(var.sqrt())
    }

    // pub fn rps_min(&self) -> Option<f64> {
    //     let s = self.rps_series()?;
    //     s.into_iter().reduce(f64::min)
    // }

    pub fn rps_max(&self) -> Option<f64> {
        let s = self.rps_series()?;
        s.into_iter().reduce(f64::max)
    }
}
