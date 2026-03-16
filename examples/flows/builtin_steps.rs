//! Builtin Step Implementations for Main Flows
//! 
//! These steps perform internal operations without webhooks:
//! - Database lookups
//! - SMS gateway calls
//! - OTP verification
//! - Validation logic
//! - Metadata persistence

use async_trait::async_trait;
use backend_core::Result as CoreResult;
use backend_flow_sdk::{Step, StepContext, StepOutcome, FlowError};
use serde_json::json;

/// Validates user exists in database
/// - Input: phone_number (from session context)
/// - Output: user info if exists, null if not
/// - Saves: user_id, fullname to session context
pub struct ValidateUserExistsStep;

#[async_trait]
impl Step for ValidateUserExistsStep {
    fn step_type(&self) -> &'static str { "validate_user_exists" }
    fn actor(&self) -> backend_flow_sdk::Actor { backend_flow_sdk::Actor::System }
    fn human_id(&self) -> &'static str { "validate_user_exists" }
    
    async fn execute(&self, ctx: &StepContext) -> CoreResult<StepOutcome, FlowError> {
        let phone = ctx.session_context.get("phone_number")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FlowError::InvalidInput("Missing phone_number".to_string()))?;
        
        println!("[INTERNAL] Looking up user with phone: {}", phone);
        
        // In production: Query database
        // In test: Mock response
        let mock_user = if phone == "+1234567890" {
            Some(json!({
                "userId": "usr_123",
                "fullname": "John Doe",
                "phoneNumber": phone
            }))
        } else {
            None
        };
        
        if let Some(user) = mock_user {
            let updates = backend_flow_sdk::step::ContextUpdates {
                session_context_patch: Some(json!({
                    "user_id": user["userId"],
                    "fullname": user["fullname"]
                })),
                flow_context_patch: Some(json!({
                    "user_exists": true
                })),
                user_metadata_patch: None,
            };
            
            Ok(StepOutcome::Done {
                output: Some(user),
                updates: Some(updates),
            })
        } else {
            let updates = backend_flow_sdk::step::ContextUpdates {
                session_context_patch: Some(json!({
                    "user_id": serde_json::Value::Null,
                    "fullname": serde_json::Value::Null
                })),
                flow_context_patch: Some(json!({
                    "user_exists": false
                })),
                user_metadata_patch: None,
            };
            
            Ok(StepOutcome::Done {
                output: Some(json!({
                    "userId": serde_json::Value::Null,
                    "fullname": serde_json::Value::Null,
                    "phoneNumber": phone
                })),
                updates: Some(updates),
            })
        }
    }
}

/// Sends SMS OTP via gateway
/// - Input: phone_number, user_id from session context
/// - Output: OTP reference, send status
pub struct SendSmsOtpStep;

#[async_trait]
impl Step for SendSmsOtpStep {
    fn step_type(&self) -> &'static str { "send_sms_otp" }
    fn actor(&self) -> backend_flow_sdk::Actor { backend_flow_sdk::Actor::System }
    fn human_id(&self) -> &'static str { "send_sms_otp" }
    
    async fn execute(&self, ctx: &StepContext) -> CoreResult<StepOutcome, FlowError> {
        let phone = ctx.session_context.get("phone_number")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FlowError::InvalidInput("Missing phone_number".to_string()))?;
        
        let user_id = ctx.session_context.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("new_user");
        
        println!("[INTERNAL] Sending OTP to phone: {} for user: {}", phone, user_id);
        
        // In production: Call SMS gateway API
        // For now: Mock
        let otp_code = generate_otp();
        let otp_ref = format!("otp_{}_{}", phone, Utc::now().timestamp());
        
        println!("[MOCK SMS] OTP {} sent to {}, ref: {}", otp_code, phone, otp_ref);
        
        Ok(StepOutcome::Done {
            output: Some(json!({
                "otpRef": otp_ref,
                "otpSent": true,
                "userId": user_id,
                "expiresAt": Utc::now() + chrono::Duration::minutes(5)
            })),
            updates: None,
        })
    }
}

/// Verifies OTP code
/// - Input: otp from step_input, otp_ref from flow context
/// - Output: verification status, auth token
pub struct VerifyOtpStep;

#[async_trait]
impl Step for VerifyOtpStep {
    fn step_type(&self) -> &'static str { "verify_otp" }
    fn actor(&self) -> backend_flow_sdk::Actor { backend_flow_sdk::Actor::System }
    fn human_id(&self) -> &'static str { "verify_otp" }
    
    async fn execute(&self, ctx: &StepContext) -> CoreResult<StepOutcome, FlowError> {
        let user_otp = ctx.input.get("otp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FlowError::InvalidInput("Missing OTP".to_string()))?;
        
        println!("[INTERNAL] Verifying OTP: {}", user_otp);
        
        // In production: Verify against stored OTP
        let is_valid = user_otp == "123456";
        
        if is_valid {
            let updates = backend_flow_sdk::step::ContextUpdates {
                session_context_patch: Some(json!({
                    "auth_token": "jwt_".to_string() + user_otp
                })),
                flow_context_patch: Some(json!({
                    "otp_verified": true
                })),
                user_metadata_patch: None,
            };
            
            Ok(StepOutcome::Done {
                output: Some(json!({
                    "verified": true,
                    "token": "jwt_".to_string() + user_otp
                })),
                updates: Some(updates),
            })
        } else {
            Ok(StepOutcome::Failed {
                error: "Invalid OTP".to_string(),
                retryable: false,
            })
        }
    }
}

/// Checks if user exists for deposit
/// Same as validate_user_exists but for deposit context
pub struct CheckUserExistsStep;

#[async_trait]
impl Step for CheckUserExistsStep {
    fn step_type(&self) -> &'static str { "check_user_exists" }
    fn actor(&self) -> backend_flow_sdk::Actor { backend_flow_sdk::Actor::System }
    fn human_id(&self) -> &'static str { "check_user_exists" }
    
    async fn execute(&self, ctx: &StepContext) -> CoreResult<StepOutcome, FlowError> {
        let phone = ctx.input.get("phone_number")  // From user input
            .or_else(|| ctx.session_context.get("phone_number"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| FlowError::InvalidInput("Missing phone_number".to_string()))?;
        
        println!("[INTERNAL] Checking user for deposit, phone: {}", phone);
        
        // Mock database lookup
        let (user_exists, user_info) = if phone == "+19998887777" {
            (true, json!({
                "userId": "usr_456",
                "fullname": "Jane Smith",
                "phoneNumber": phone
            }))
        } else {
            (false, json!({
                "userId": serde_json::Value::Null,
                "fullname": serde_json::Value::Null,
                "phoneNumber": phone
            }))
        };
        
        let updates = backend_flow_sdk::step::ContextUpdates {
            session_context_patch: if user_exists {
                Some(json!({
                    "recipient_name": user_info["fullname"],
                    "recipient_phone": user_info["phoneNumber"],
                    "deposit_amount": ctx.input.get("amount"),
                    "deposit_currency": ctx.input.get("currency"),
                    "provider": ctx.input.get("provider")
                }))
            } else {
                Some(json!({
                    "recipient_name": serde_json::Value::Null,
                    "recipient_phone": user_info["phoneNumber"],
                    "deposit_amount": ctx.input.get("amount"),
                    "deposit_currency": ctx.input.get("currency"),
                    "provider": ctx.input.get("provider")
                }))
            },
            flow_context_patch: Some(json!({
                "user_exists": user_exists
            })),
            user_metadata_patch: None,
        };
        
        Ok(StepOutcome::Done {
            output: Some(json!({
                "userExists": user_exists,
                "userInfo": user_info
            })),
            updates: Some(updates),
        })
    }
}

/// Validates deposit amount
/// Checks limits, currency support, etc.
pub struct ValidateDepositStep;

#[async_trait]
impl Step for ValidateDepositStep {
    fn step_type(&self) -> &'static str { "validate_deposit" }
    fn actor(&self) -> backend_flow_sdk::Actor { backend_flow_sdk::Actor::System }
    fn human_id(&self) -> &'static str { "validate_deposit" }
    
    async fn execute(&self, ctx: &StepContext) -> CoreResult<StepOutcome, FlowError> {
        let amount = ctx.input.get("amount")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FlowError::InvalidInput("Missing amount".to_string()))?;
        
        let currency = ctx.input.get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FlowError::InvalidInput("Missing currency".to_string()))?;
        
        println!("[INTERNAL] Validating deposit: {} {}", amount, currency);
        
        // Mock validation rules
        let amount_num: f64 = amount.parse().map_err(|_| {
            FlowError::InvalidInput("Invalid amount format".to_string())
        })?;
        
        let is_valid = amount_num > 0.0 && amount_num <= 10000.0 
            && ["USD", "EUR"].contains(&currency);
        
        if is_valid {
            Ok(StepOutcome::Done {
                output: Some(json!({
                    "valid": true,
                    "amount": amount,
                    "currency": currency
                })),
                updates: None,
            })
        } else {
            Ok(StepOutcome::Failed {
                error: "Deposit validation failed".to_string(),
                retryable: false,
            })
        }
    }
}

/// Persists deposit result to metadata
/// Called after webhook success
pub struct PersistDepositResultStep;

#[async_trait]
impl Step for PersistDepositResultStep {
    fn step_type(&self) -> &'static str { "persist_deposit_result" }
    fn actor(&self) -> backend_flow_sdk::Actor { backend_flow_sdk::Actor::System }
    fn human_id(&self) -> &'static str { "persist_deposit_result" }
    
    async fn execute(&self, ctx: &StepContext) -> CoreResult<StepOutcome, FlowError> {
        // Get deposit result from previous webhook step
        let webhook_response = &ctx.flow_context;
        
        let tx_id = webhook_response.get("transactionId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        
        let status = webhook_response.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");
        
        println!("[INTERNAL] Saving deposit result to metadata: tx={}, status={}", tx_id, status);
        
        let updates = backend_flow_sdk::step::ContextUpdates {
            session_context_patch: None,
            flow_context_patch: None,
            user_metadata_patch: Some(json!({
                "deposit_transaction_id": tx_id,
                "deposit_status": status,
                "deposit_processed_at": Utc::now().to_rfc3339()
            })),
        };
        
        Ok(StepOutcome::Done {
            output: Some(json!({
                "saved": true,
                "transaction_id": tx_id,
                "status": status
            })),
            updates: Some(updates),
        })
    }
}

fn generate_otp() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

use chrono::Utc;
