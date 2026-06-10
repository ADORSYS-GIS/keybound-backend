# Plan: WhatsApp Fallback for SMS Gateway

This plan outlines the integration of `wwebjs.dev` (WhatsApp Web JS) as a fallback mechanism for the existing Rust-based `sms-gateway`.

## 1. New Service: `whatsapp-provider` (Node.js)

Since `whatsapp-web.js` is a Node.js library that requires a browser environment, we will implement it as a separate microservice.

### Features:
- **API:** REST API using Express.
  - `POST /send`: Sends a message to a phone number.
  - `GET /status`: Returns the current connection status and QR code (if not authenticated).
- **Library:** `whatsapp-web.js` with `puppeteer`.
- **Session Management:** Local file-based session storage (using `LocalAuth`) to persist logins across restarts.
- **Dockerized:** Running in a container with Chromium dependencies.

## 2. Updates to `sms-gateway` (Rust)

We will modify the existing `sms-gateway` to support calling the new WhatsApp service as a fallback.

### Changes:
- **`app/crates/backend-core/src/config.rs`**:
  - Add `Whatsapp` and `Fallback` variants to `SmsProviderType`.
  - Define `WhatsappConfig` (base_url, optional auth).
  - Define `FallbackConfig` (primary provider, secondary provider).
- **`app/bins/sms-gateway/src/sms_provider.rs`**:
  - Implement `WhatsappSmsProvider`: Translates `send_otp` calls to HTTP requests for the Node.js service.
  - Implement `FallbackSmsProvider`: Implements the fallback logic (try primary, then secondary).
- **`app/bins/sms-gateway/src/main.rs`**:
  - Update `create_sms_provider` to handle the new provider types.

## 3. Configuration Example

```yaml
sms:
  provider: fallback
  fallback:
    primary:
      provider: orange
      orange:
        client_id: "..."
        client_secret: "..."
        # ... other orange config
    secondary:
      provider: whatsapp
      whatsapp:
        base_url: "http://whatsapp-provider:3000"
```

## 4. Infrastructure (Docker)

- **New Service:** `whatsapp-provider`
- **Volume:** `/data` for session persistence.
- **Environment:** `PUPPETEER_SKIP_CHROMIUM_DOWNLOAD=true`, `CHROME_PATH=/usr/bin/google-chrome-stable`.

## 5. Implementation Steps

1. **Step 1:** Scaffold the `whatsapp-provider` Node.js application.
2. **Step 2:** Implement the `WhatsAppSmsProvider` in Rust.
3. **Step 3:** Implement the `FallbackSmsProvider` logic in Rust.
4. **Step 4:** Update configuration and main initialization.
5. **Step 5:** Add Dockerfile and update `compose.yml`.
6. **Step 6:** Test the fallback flow by simulating a primary provider failure.
