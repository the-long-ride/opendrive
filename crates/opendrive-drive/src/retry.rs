//! Retry with exponential backoff for rate limits.

use crate::{DriveError, Result};
use std::future::Future;
use std::time::Duration;

pub async fn with_backoff<T, F, Fut>(mut f: F, max_attempts: u32) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0u32;
    loop {
        match f().await {
            Err(DriveError::RateLimited) if attempt + 1 < max_attempts => {
                let ms = (250u64 << attempt).min(30_000);
                tokio::time::sleep(Duration::from_millis(ms)).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn immediate_success() {
        let calls = AtomicU32::new(0);
        let res = with_backoff(
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok::<_, DriveError>(42) }
            },
            3,
        )
        .await;
        assert_eq!(res.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_on_rate_limited_then_succeeds() {
        let calls = AtomicU32::new(0);
        let res = with_backoff(
            || {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n < 2 {
                        Err(DriveError::RateLimited)
                    } else {
                        Ok("ok")
                    }
                }
            },
            4,
        )
        .await;
        assert_eq!(res.unwrap(), "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn exhausts_max_attempts() {
        let calls = AtomicU32::new(0);
        let res: Result<()> = with_backoff(
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err(DriveError::RateLimited) }
            },
            2,
        )
        .await;
        assert!(matches!(res, Err(DriveError::RateLimited)));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn does_not_retry_non_rate_limit_error() {
        let calls = AtomicU32::new(0);
        let res: Result<()> = with_backoff(
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err(DriveError::NotFound) }
            },
            5,
        )
        .await;
        assert!(matches!(res, Err(DriveError::NotFound)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
