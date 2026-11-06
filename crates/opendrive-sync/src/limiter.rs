//! Simple token-bucket bandwidth limiter (bytes/sec).

use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub struct BandwidthLimiter {
    inner: Mutex<Inner>,
}

struct Inner {
    bytes_per_sec: Option<u64>,
    tokens: f64,
    last: Instant,
}

impl BandwidthLimiter {
    pub fn new(bytes_per_sec: Option<u64>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                bytes_per_sec,
                tokens: bytes_per_sec.unwrap_or(0) as f64,
                last: Instant::now(),
            }),
        }
    }

    pub fn set_limit(&self, bytes_per_sec: Option<u64>) {
        let mut g = self.inner.lock();
        g.bytes_per_sec = bytes_per_sec;
        g.tokens = bytes_per_sec.unwrap_or(0) as f64;
        g.last = Instant::now();
    }

    /// Block until `n` bytes may be transferred (async-friendly via sleep).
    pub async fn take(&self, n: u64) {
        loop {
            let wait = {
                let mut g = self.inner.lock();
                let Some(rate) = g.bytes_per_sec else {
                    return;
                };
                if rate == 0 {
                    return;
                }
                let elapsed = g.last.elapsed().as_secs_f64();
                g.last = Instant::now();
                g.tokens = (g.tokens + elapsed * rate as f64).min(rate as f64 * 2.0);
                if g.tokens >= n as f64 {
                    g.tokens -= n as f64;
                    return;
                }
                let need = n as f64 - g.tokens;
                Duration::from_secs_f64(need / rate as f64)
            };
            tokio::time::sleep(wait).await;
        }
    }
}
