//! HTTP Basic auth plus a small per-IP failure throttle.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;

const MAX_FAILURES: u32 = 10;
const WINDOW: Duration = Duration::from_secs(10 * 60);
const BLOCK: Duration = Duration::from_secs(5 * 60);

pub struct Auth {
    password: Option<String>,
}

impl Auth {
    pub fn new(password: Option<String>) -> Self {
        Self {
            password: password.filter(|value| !value.is_empty()),
        }
    }

    pub fn required(&self) -> bool {
        self.password.is_some()
    }

    pub fn check(&self, header: Option<&str>) -> bool {
        let Some(expected) = &self.password else {
            return true;
        };
        let Some(encoded) = header.and_then(|value| value.strip_prefix("Basic ")) else {
            return false;
        };
        let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
            return false;
        };
        let Some((user, supplied)) = split_credentials(&raw) else {
            return false;
        };
        constant_time_eq(user, b"share") & constant_time_eq(supplied, expected.as_bytes())
    }
}

fn split_credentials(raw: &[u8]) -> Option<(&[u8], &[u8])> {
    let index = raw.iter().position(|byte| *byte == b':')?;
    Some((&raw[..index], &raw[index + 1..]))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Default)]
struct Failure {
    count: u32,
    window_start: Option<Instant>,
    blocked_until: Option<Instant>,
}

#[derive(Default)]
pub struct AuthLimiter {
    entries: Mutex<HashMap<IpAddr, Failure>>,
}

impl AuthLimiter {
    pub fn blocked_for(&self, ip: IpAddr) -> Option<u64> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let entry = entries.get_mut(&ip)?;
        let until = entry.blocked_until?;
        let now = Instant::now();
        if until > now {
            Some((until - now).as_secs().max(1))
        } else {
            entry.blocked_until = None;
            entry.count = 0;
            entry.window_start = None;
            None
        }
    }

    pub fn note_failure(&self, ip: IpAddr) {
        let now = Instant::now();
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if entries.len() > 1024 {
            entries.retain(|_, entry| {
                entry.blocked_until.is_some_and(|until| until > now)
                    || entry.window_start.is_some_and(|start| now - start < WINDOW)
            });
        }
        let entry = entries.entry(ip).or_default();
        let expired = entry.window_start.is_none_or(|start| now - start > WINDOW);
        if expired {
            entry.count = 0;
            entry.window_start = Some(now);
        }
        entry.count += 1;
        if entry.count >= MAX_FAILURES {
            entry.blocked_until = Some(now + BLOCK);
        }
    }

    pub fn note_success(&self, ip: IpAddr) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        entries.remove(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic(user: &str, password: &str) -> String {
        let raw = format!("{user}:{password}");
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        )
    }

    #[test]
    fn checks_basic_header() {
        let auth = Auth::new(Some("s3cret".into()));
        assert!(auth.required());
        assert!(auth.check(Some(&basic("share", "s3cret"))));
        assert!(!auth.check(Some(&basic("share", "wrong"))));
        assert!(!auth.check(Some(&basic("admin", "s3cret"))));
        assert!(!auth.check(None));
        assert!(!auth.check(Some("Basic !!!")));
        assert!(Auth::new(None).check(None));
    }

    #[test]
    fn throttles_failures() {
        let limiter = AuthLimiter::default();
        let ip: IpAddr = "10.0.0.2".parse().unwrap();
        for _ in 0..MAX_FAILURES {
            assert!(limiter.blocked_for(ip).is_none());
            limiter.note_failure(ip);
        }
        assert!(limiter.blocked_for(ip).is_some());
        limiter.note_success(ip);
        assert!(limiter.blocked_for(ip).is_none());
    }
}
