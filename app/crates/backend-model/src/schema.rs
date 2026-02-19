pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "kyc_case_status"))]
    pub struct KycCaseStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "kyc_submission_status"))]
    pub struct KycSubmissionStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "kyc_provisioning_status"))]
    pub struct KycProvisioningStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "kyc_document_status"))]
    pub struct KycDocumentStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "provisioning_status"))]
    pub struct ProvisioningStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "kyc_review_queue_status"))]
    pub struct KycReviewQueueStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "kyc_review_decision_outcome"))]
    pub struct KycReviewDecisionOutcome;
}

diesel::table! {
    app_user (user_id) {
        #[max_length = 40]
        user_id -> Varchar,
        #[max_length = 255]
        realm -> Varchar,
        #[max_length = 255]
        username -> Varchar,
        #[max_length = 255]
        first_name -> Nullable<Varchar>,
        #[max_length = 255]
        last_name -> Nullable<Varchar>,
        #[max_length = 320]
        email -> Nullable<Varchar>,
        email_verified -> Bool,
        #[max_length = 64]
        phone_number -> Nullable<Varchar>,
        #[max_length = 128]
        fineract_customer_id -> Nullable<Varchar>,
        disabled -> Bool,
        attributes -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::KycCaseStatus;

    kyc_case (id) {
        id -> Text,
        user_id -> Text,
        case_status -> Varchar,
        active_submission_id -> Nullable<Text>,
        queue_rank -> Nullable<Int8>,
        last_activity_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::KycSubmissionStatus;
    use super::sql_types::KycProvisioningStatus;

    kyc_submission (id) {
        id -> Text,
        kyc_case_id -> Text,
        version -> Int4,
        status -> Varchar,
        requested_tier -> Nullable<Int4>,
        decided_tier -> Nullable<Int4>,
        approved_tier -> Nullable<Int4>,
        submitted_at -> Nullable<Timestamptz>,
        decided_at -> Nullable<Timestamptz>,
        decided_by -> Nullable<Text>,
        reviewer_id -> Nullable<Text>,
        next_action -> Nullable<Text>,
        provisioning_status -> Varchar,
        profile_json -> Jsonb,
        profile_etag -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        first_name -> Nullable<Text>,
        last_name -> Nullable<Text>,
        email -> Nullable<Text>,
        phone_number -> Nullable<Text>,
        date_of_birth -> Nullable<Text>,
        nationality -> Nullable<Text>,
        rejection_reason -> Nullable<Text>,
        review_notes -> Nullable<Text>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::KycDocumentStatus;

    kyc_document (id) {
        id -> Text,
        submission_id -> Text,
        doc_type -> Text,
        s3_bucket -> Text,
        s3_key -> Text,
        file_name -> Text,
        mime_type -> Text,
        size_bytes -> Int8,
        sha256 -> Text,
        status -> Varchar,
        uploaded_at -> Timestamptz,
        expires_at -> Nullable<Timestamptz>,
        object_url -> Nullable<Text>,
        is_verified -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    kyc_submission_profile_history (id) {
        id -> Int8,
        submission_id -> Text,
        version -> Int4,
        profile_json -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::KycReviewQueueStatus;

    kyc_review_queue (id) {
        id -> Int8,
        case_id -> Text,
        status -> Varchar,
        assigned_to -> Nullable<Text>,
        claimed_at -> Nullable<Timestamptz>,
        lock_expires_at -> Nullable<Timestamptz>,
        priority -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::KycReviewDecisionOutcome;

    kyc_review_decision (id) {
        id -> Int8,
        submission_id -> Text,
        decision -> Varchar,
        reason_code -> Text,
        comment -> Nullable<Text>,
        decided_at -> Timestamptz,
        reviewer_id -> Nullable<Text>,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    fineract_provisioning (id) {
        id -> Text,
        kyc_case_id -> Text,
        submission_id -> Text,
        status -> Varchar,
        fineract_customer_id -> Nullable<Text>,
        error_code -> Nullable<Text>,
        error_message -> Nullable<Text>,
        attempt_no -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    device (device_id, public_jwk) {
        #[max_length = 40]
        device_id -> Varchar,
        #[max_length = 40]
        user_id -> Varchar,
        #[max_length = 255]
        jkt -> Varchar,
        public_jwk -> Text,
        #[max_length = 255]
        status -> Varchar,
        #[max_length = 255]
        label -> Nullable<Varchar>,
        created_at -> Timestamptz,
        last_seen_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    approval (request_id) {
        #[max_length = 40]
        request_id -> Varchar,
        #[max_length = 40]
        user_id -> Varchar,
        #[max_length = 40]
        new_device_id -> Varchar,
        #[max_length = 255]
        new_device_jkt -> Varchar,
        new_device_public_jwk -> Text,
        #[max_length = 64]
        new_device_platform -> Nullable<Varchar>,
        #[max_length = 128]
        new_device_model -> Nullable<Varchar>,
        #[max_length = 64]
        new_device_app_version -> Nullable<Varchar>,
        #[max_length = 255]
        status -> Varchar,
        created_at -> Timestamptz,
        expires_at -> Timestamptz,
        decided_at -> Nullable<Timestamptz>,
        #[max_length = 40]
        decided_by_device_id -> Nullable<Varchar>,
        #[max_length = 512]
        message -> Nullable<Varchar>,
    }
}

diesel::table! {
    sms_messages (id) {
        #[max_length = 40]
        id -> Varchar,
        #[max_length = 255]
        realm -> Varchar,
        #[max_length = 255]
        client_id -> Varchar,
        #[max_length = 40]
        user_id -> Nullable<Varchar>,
        #[max_length = 64]
        phone_number -> Varchar,
        #[max_length = 64]
        hash -> Varchar,
        otp_sha256 -> Bytea,
        ttl_seconds -> Int4,
        #[max_length = 32]
        status -> Varchar,
        attempt_count -> Int4,
        max_attempts -> Int4,
        next_retry_at -> Nullable<Timestamptz>,
        last_error -> Nullable<Text>,
        #[max_length = 255]
        sns_message_id -> Nullable<Varchar>,
        #[max_length = 255]
        session_id -> Nullable<Varchar>,
        #[max_length = 255]
        trace_id -> Nullable<Varchar>,
        metadata -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        sent_at -> Nullable<Timestamptz>,
        confirmed_at -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(kyc_case -> app_user (user_id));
diesel::joinable!(kyc_submission -> kyc_case (kyc_case_id));
diesel::joinable!(kyc_submission_profile_history -> kyc_submission (submission_id));
diesel::joinable!(kyc_document -> kyc_submission (submission_id));
diesel::joinable!(kyc_review_queue -> kyc_case (case_id));
diesel::joinable!(kyc_review_decision -> kyc_submission (submission_id));
diesel::joinable!(device -> app_user (user_id));
diesel::joinable!(approval -> app_user (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    app_user,
    kyc_case,
    kyc_submission,
    kyc_submission_profile_history,
    kyc_document,
    kyc_review_queue,
    kyc_review_decision,
    fineract_provisioning,
    device,
    approval,
    sms_messages,
);
