diesel::table! {
    app_user (user_id) {
        user_id -> Text,
        realm -> Text,
        username -> Text,
        full_name -> Nullable<Text>,
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
        device_record_id -> Text,
        status -> Text,
        label -> Nullable<Text>,
        created_at -> Timestamptz,
        last_seen_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    sm_instance (id) {
        id -> Text,
        kind -> Text,
        user_id -> Nullable<Text>,
        idempotency_key -> Text,
        status -> Text,
        context -> Jsonb,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        completed_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    sm_event (id) {
        id -> Text,
        instance_id -> Text,
        kind -> Text,
        actor_type -> Text,
        actor_id -> Nullable<Text>,
        payload -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    sm_step_attempt (id) {
        id -> Text,
        instance_id -> Text,
        step_name -> Text,
        attempt_no -> Int4,
        status -> Text,
        external_ref -> Nullable<Text>,
        input -> Jsonb,
        output -> Nullable<Jsonb>,
        error -> Nullable<Jsonb>,
        queued_at -> Nullable<Timestamptz>,
        started_at -> Nullable<Timestamptz>,
        finished_at -> Nullable<Timestamptz>,
        next_retry_at -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(device -> app_user (user_id));
diesel::joinable!(sm_instance -> app_user (user_id));
diesel::joinable!(sm_event -> sm_instance (instance_id));
diesel::joinable!(sm_step_attempt -> sm_instance (instance_id));

diesel::allow_tables_to_appear_in_same_query!(
    app_user,
    device,
    sm_instance,
    sm_event,
    sm_step_attempt,
);
