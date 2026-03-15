//! Notifications types for cross-crate message passing.
//!
//! This module defines the notification job types that can be enqueued by backend-server
//! and consumed by sms-sink or other notification backends.

use serde::{Deserialize, Serialize};

/// Notification job types that can be enqueued for async processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationJob {
    /// SMS OTP notification
    Otp {
        step_id: String,
        msisdn: String,
        otp: String,
    },
    /// Magic email link notification
    MagicEmail {
        step_id: String,
        email: String,
        token: String,
    },
}
