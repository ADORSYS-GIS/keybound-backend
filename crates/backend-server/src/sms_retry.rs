use crate::state::AppState;
use backend_model::db;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::warn;

pub fn spawn(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&state).await {
                warn!("sms retry tick failed: {e}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn tick(state: &AppState) -> backend_core::Result<()> {
    let rows: Vec<db::SmsMessageRow> = sqlx::query_as(
        r#"
        SELECT
          id::text as id,
          realm,
          client_id,
          user_id,
          phone_number,
          hash,
          otp_sha256,
          ttl_seconds,
          status::text as status,
          attempt_count,
          max_attempts,
          next_retry_at,
          last_error,
          sns_message_id,
          session_id,
          trace_id,
          metadata,
          created_at,
          sent_at,
          confirmed_at
        FROM sms_messages
        WHERE status::text IN ('PENDING', 'FAILED')
          AND (next_retry_at IS NULL OR next_retry_at <= now())
          AND attempt_count < max_attempts
        ORDER BY created_at ASC
        LIMIT 25
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    for row in rows {
        if let Err(e) = try_publish(state, row).await {
            warn!("sms publish failed: {e}");
        }
    }

    Ok(())
}

async fn try_publish(state: &AppState, row: db::SmsMessageRow) -> backend_core::Result<()> {
    let message = row
        .metadata
        .as_ref()
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if message.is_empty() {
        sqlx::query(
            "UPDATE sms_messages SET status = 'GAVE_UP', last_error = $2 WHERE id = $1::uuid",
        )
        .bind(&row.id)
        .bind("missing message body")
        .execute(&state.db)
        .await?;
        return Ok(());
    }

    let attempt = row.attempt_count.max(0) as u32 + 1;

    match state
        .sns
        .publish()
        .phone_number(row.phone_number.clone())
        .message(message.to_owned())
        .send()
        .await
    {
        Ok(out) => {
            let message_id = out.message_id().map(|s| s.to_owned());
            sqlx::query(
                r#"
                UPDATE sms_messages
                SET
                  status = 'SENT',
                  attempt_count = attempt_count + 1,
                  sns_message_id = $2,
                  sent_at = now(),
                  last_error = NULL,
                  next_retry_at = NULL
                WHERE id = $1::uuid
                "#,
            )
            .bind(&row.id)
            .bind(message_id)
            .execute(&state.db)
            .await?;
        }
        Err(e) => {
            let max_attempts = row.max_attempts.max(1) as u32;
            let gave_up = attempt >= max_attempts;

            let backoff = backoff_seconds(
                state.config.aws.sns.initial_backoff_seconds,
                row.attempt_count.max(0) as u32,
            );

            let next_retry_at = if gave_up {
                None
            } else {
                Some(Utc::now() + chrono::Duration::seconds(backoff as i64))
            };

            sqlx::query(
                r#"
                UPDATE sms_messages
                SET
                  status = $2::sms_status,
                  attempt_count = attempt_count + 1,
                  last_error = $3,
                  next_retry_at = $4
                WHERE id = $1::uuid
                "#,
            )
            .bind(&row.id)
            .bind(if gave_up { "GAVE_UP" } else { "FAILED" })
            .bind(e.to_string())
            .bind(next_retry_at)
            .execute(&state.db)
            .await?;
        }
    }

    Ok(())
}

fn backoff_seconds(initial: u64, attempt_count: u32) -> u64 {
    let base = initial.max(1);
    let factor = 2u64.saturating_pow(attempt_count.min(16));
    base.saturating_mul(factor).min(3600)
}
