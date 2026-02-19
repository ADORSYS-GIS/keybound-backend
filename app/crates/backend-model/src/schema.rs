diesel::table! {
    app_user (user_id) {
        user_id -> Text,
        realm -> Text,
        username -> Text,
        first_name -> Nullable<Text>,
        last_name -> Nullable<Text>,
        email -> Nullable<Text>,
        email_verified -> Bool,
        phone_number -> Nullable<Text>,
        fineract_customer_id -> Nullable<Text>,
        disabled -> Bool,
        attributes -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    device (device_id, public_jwk) {
        device_id -> Text,
        user_id -> Text,
        jkt -> Text,
        public_jwk -> Text,
        status -> Text,
        label -> Nullable<Text>,
        created_at -> Timestamptz,
        last_seen_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    kyc_session (id) {
        id -> Text,
        user_id -> Text,
        status -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    kyc_step (id) {
        id -> Text,
        session_id -> Text,
        user_id -> Text,
        step_type -> Text,
        status -> Text,
        data -> Jsonb,
        policy -> Jsonb,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        submitted_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    kyc_otp_challenge (otp_ref) {
        otp_ref -> Text,
        step_id -> Text,
        msisdn -> Text,
        channel -> Text,
        otp_hash -> Text,
        expires_at -> Timestamptz,
        tries_left -> Int4,
        created_at -> Timestamptz,
        verified_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    kyc_magic_email_challenge (token_ref) {
        token_ref -> Text,
        step_id -> Text,
        email -> Text,
        token_hash -> Text,
        expires_at -> Timestamptz,
        created_at -> Timestamptz,
        verified_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    kyc_upload (upload_id) {
        upload_id -> Text,
        step_id -> Text,
        user_id -> Text,
        purpose -> Text,
        asset_type -> Text,
        mime -> Text,
        size_bytes -> Int8,
        bucket -> Text,
        object_key -> Text,
        method -> Text,
        url -> Text,
        headers -> Jsonb,
        multipart -> Nullable<Jsonb>,
        expires_at -> Timestamptz,
        created_at -> Timestamptz,
        completed_at -> Nullable<Timestamptz>,
        etag -> Nullable<Text>,
        computed_sha256 -> Nullable<Text>,
    }
}

diesel::table! {
    kyc_evidence (evidence_id) {
        evidence_id -> Text,
        step_id -> Text,
        asset_type -> Text,
        bucket -> Text,
        object_key -> Text,
        sha256 -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    kyc_review_queue (id) {
        id -> Int8,
        session_id -> Text,
        step_id -> Text,
        status -> Text,
        assigned_to -> Nullable<Text>,
        claimed_at -> Nullable<Timestamptz>,
        lock_expires_at -> Nullable<Timestamptz>,
        priority -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    kyc_review_decision (id) {
        id -> Int8,
        session_id -> Text,
        step_id -> Text,
        outcome -> Text,
        reason_code -> Text,
        comment -> Nullable<Text>,
        decided_at -> Timestamptz,
        reviewer_id -> Nullable<Text>,
    }
}

diesel::joinable!(device -> app_user (user_id));
diesel::joinable!(kyc_session -> app_user (user_id));
diesel::joinable!(kyc_step -> kyc_session (session_id));
diesel::joinable!(kyc_step -> app_user (user_id));
diesel::joinable!(kyc_otp_challenge -> kyc_step (step_id));
diesel::joinable!(kyc_magic_email_challenge -> kyc_step (step_id));
diesel::joinable!(kyc_upload -> kyc_step (step_id));
diesel::joinable!(kyc_upload -> app_user (user_id));
diesel::joinable!(kyc_evidence -> kyc_step (step_id));
diesel::joinable!(kyc_review_queue -> kyc_session (session_id));
diesel::joinable!(kyc_review_queue -> kyc_step (step_id));
diesel::joinable!(kyc_review_decision -> kyc_session (session_id));
diesel::joinable!(kyc_review_decision -> kyc_step (step_id));

diesel::allow_tables_to_appear_in_same_query!(
    app_user,
    device,
    kyc_session,
    kyc_step,
    kyc_otp_challenge,
    kyc_magic_email_challenge,
    kyc_upload,
    kyc_evidence,
    kyc_review_queue,
    kyc_review_decision,
);
