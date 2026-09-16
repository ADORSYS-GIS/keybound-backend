//! Process-local security helpers for the recovery flow.
//!
//! - `RateLimiter`: a simple in-memory sliding-window throttle used to bound
//!   per-phone recovery-session creation and per-case OTP resends so an
//!   authenticated caller cannot spam arbitrary numbers (review item M4).
//! - `OtpLockRegistry`: a per-case async mutex used to serialize OTP
//!   verification so the attempts counter is incremented atomically and a
//!   locked-out case cannot be brute-forced through concurrent submissions
//!   (review item M1).

use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A process-local token-bucket-style limiter. Acceptable because the BFF is a
/// thin facade and a single user-storage instance fronts each tenant; it bounds
/// abuse without requiring an external store.
#[derive(Clone)]
pub struct RateLimiter {
    windows: Arc<Mutex<HashMap<String, (i64, u32)>>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns true if `key` is within `limit` calls during `window_seconds`.
    fn allow(&self, key: &str, limit: u32, window_seconds: i64) -> bool {
        let now = Utc::now().timestamp();
        let mut windows = self.windows.lock().unwrap();
        let entry = windows.entry(key.to_owned()).or_insert((now, 0));
        if now - entry.0 >= window_seconds {
            *entry = (now, 0);
        }
        if entry.1 >= limit {
            return false;
        }
        entry.1 += 1;
        true
    }

    /// Bounds the number of recovery sessions/flows created for one phone.
    pub fn allow_session_creation(&self, phone: &str) -> bool {
        self.allow(&format!("session:{phone}"), 10, 60)
    }

    /// Bounds the number of recovery flows added to one session.
    pub fn allow_flow_creation(&self, session_id: &str) -> bool {
        self.allow(&format!("flow:{session_id}"), 5, 60)
    }

    /// Bounds the number of OTP resends for one case.
    pub fn allow_otp_resend(&self, case_id: &str) -> bool {
        self.allow(&format!("resend:{case_id}"), 3, 300)
    }
}

/// A per-key registry of async mutexes. The map is process-local and never
/// shrinks (bounded in practice by the number of active recovery cases), which
/// is acceptable for a single-instance facade.
#[derive(Default)]
pub struct OtpLockRegistry {
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl OtpLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lock_for(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap();
        locks
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_enforces_window_and_limit() {
        let limiter = RateLimiter::new();
        for _ in 0..3 {
            assert!(limiter.allow_otp_resend("case_1"));
        }
        assert!(!limiter.allow_otp_resend("case_1"), "fourth resend blocked");
        // A different case is unaffected.
        assert!(limiter.allow_otp_resend("case_2"));
    }

    #[test]
    fn rate_limiter_is_per_key() {
        let limiter = RateLimiter::new();
        for _ in 0..10 {
            assert!(limiter.allow_session_creation("+237690000000"));
        }
        assert!(!limiter.allow_session_creation("+237690000000"));
        assert!(limiter.allow_session_creation("+237699999999"));
    }

    #[tokio::test]
    async fn otp_lock_serializes_same_key() {
        let registry = OtpLockRegistry::new();
        let a = registry.lock_for("case_x");
        let b = registry.lock_for("case_x");
        assert!(Arc::ptr_eq(&a, &b), "same key returns the same mutex");

        let _guard = a.lock().await;
        let different = registry.lock_for("case_y");
        let _other_guard = different.lock().await;
    }
}
