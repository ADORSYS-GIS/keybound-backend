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

## 6. Testing Guide

### Prerequisites

```bash
# Ensure both services are running with the WhatsApp-first config
docker compose -f compose.yml up -d sms-gateway whatsapp-provider
```

### QR Code Authentication (First-time only)

When the `whatsapp-provider` starts for the first time, it generates a **QR code**. You can retrieve it via API or view it in the logs.

#### Option 1: API Endpoint (Recommended for Frontend)

```bash
# Get QR code for frontend display
curl http://whatsapp-provider:3000/qr

# Response:
# {"authenticated":false,"qrCode":"2@...","message":"Scan this QR code with WhatsApp on your phone"}
```

The frontend can use a QR code library (e.g., `qrcode` npm package) to render the `qrCode` string.

#### Option 2: View in Logs

```bash
docker compose -f compose.yml logs --tail 30 whatsapp-provider
```

The QR code will appear as ASCII art in the logs.

#### Option 3: Pair Code (Phone Number Authentication)

Alternative to QR code — request an 8-digit code to enter on your phone:

```bash
# Request pairing code for phone number
curl -X POST http://whatsapp-provider:3000/pair-code \
  -H "Content-Type: application/json" \
  -d '{"phone": "+237XXXXXXXXX"}'

# Response:
# {"success":true,"phone":"237XXXXXXXXX","pairingCode":"ABCD-EFGH","message":"Enter this code..."}
```

On your phone: WhatsApp → Settings → Linked Devices → Link a Device → Enter the code.

#### Verify Authentication

```bash
# Check authentication status
curl http://whatsapp-provider:3000/auth/status

# Response when authenticated:
# {"authenticated":true,"ready":true,"hasQrCode":false,"pairCodeRequested":false,"phoneNumber":"237XXXXXXXXX"}
```

The session is persisted in a Docker volume (`whatsapp-data`) and survives container restarts. Re-authentication is only needed if explicitly logged out or session expires.

### Current Config (dev)

```yaml
sms:
  provider: whatsapp           # Primary — sends via WhatsApp Web
  whatsapp:
    base_url: "http://whatsapp-provider:3000"
  fallback:
    - provider: console        # Fallback — logs to stdout if WhatsApp fails
```

### Test 1: Direct WhatsApp provider

Send a real WhatsApp message to verify the provider can reach the WhatsApp Web API:

```bash
docker run --rm --network keybound-backend_default alpine:3.20 sh -c '
apk add -q curl
curl -s -m 10 -X POST http://whatsapp-provider:3000/send \
  -H "Content-Type: application/json" \
  -d "{\"phone\": \"237XXXXXXXXX\", \"message\": \"Test from WhatsApp provider\"}"
'
```

Use the international format (e.g. `237XXXXXXXXX` for Cameroon) — the provider strips the `+` prefix and appends `@c.us` automatically.

### Test 2: SMS gateway with WhatsApp primary → Console fallback

```bash
docker run --rm --network keybound-backend_default alpine:3.20 sh -c '
apk add -q curl
curl -s -m 10 -X POST http://sms-gateway:3000/otp \
  -H "Content-Type: application/json" \
  -d "{\"msisdn\": \"237XXXXXXXXX\", \"otp\": \"123456\"}"
'
```

Check the logs to see which provider handled the request:
```bash
docker compose -f compose.yml logs sms-gateway
# Expect: "Using WhatsApp provider: http://whatsapp-provider:3000"
```

### Test 3: Fallback activation (transient error)

Stop the WhatsApp provider, then send another OTP — Console fallback should activate:

```bash
docker compose -f compose.yml stop whatsapp-provider

# Send OTP — should fall back to Console
docker run --rm --network keybound-backend_default alpine:3.20 sh -c '
apk add -q curl
curl -s -m 10 -X POST http://sms-gateway:3000/otp \
  -H "Content-Type: application/json" \
  -d "{\"msisdn\": \"237XXXXXXXXX\", \"otp\": \"654321\"}"
'

# Check logs — should show console output
docker compose -f compose.yml logs --tail 10 sms-gateway
# Expect something like: "[ERROR] WhatsApp provider unavailable, falling back to Console"
```

### Test 4: Full integration via app

Once the `app` service is running, the KYC flow sends OTPs via `SMS_SINK_URL=http://sms-gateway:3000`. Any OTP step (e.g., phone OTP verification) will traverse the fallback chain automatically.

```bash
docker compose -f compose.yml up -d app
# Trigger an OTP flow via the BFF or Staff API
# The sms-gateway logs will show which provider served the request
```

### Test 5: Restore WhatsApp after failure

```bash
docker compose -f compose.yml start whatsapp-provider
# Wait for "ready:true" on /health
# Subsequent OTP requests use WhatsApp again (primary)
```

### Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `{"ready":false}` on /health | WhatsApp Web not initialized yet | Wait 15–30s for Chromium/Puppeteer |
| `{"authenticated":false}` | QR not scanned | Check logs for QR code, scan with phone |
| `Connection reset by peer` in CI | Transient network error | Rerun the CI job |
| `provider: console` in logs | Config not picked up | Verify `config/dev.yaml` has `provider: whatsapp` |
| `ProtocolError: Runtime.callFunctionOn timed out` | Puppeteer timeout | Increase `protocolTimeout` in puppeteer config |

## 7. WhatsApp Provider API Reference

The `whatsapp-provider` service exposes the following REST endpoints:

### Authentication Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check — returns `{ready, authenticated, hasQrCode}` |
| `/qr` | GET | Get QR code string for frontend display |
| `/pair-code` | POST | Request pairing code for phone number auth |
| `/auth/status` | GET | Full authentication status |
| `/logout` | POST | Logout and clear session |

### Messaging Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/send` | POST | Send WhatsApp message |
| `/info` | GET | Get client info (when authenticated) |

### Request/Response Examples

#### GET /qr
```json
// Response when not authenticated
{"authenticated": false, "qrCode": "2@...", "message": "Scan this QR code with WhatsApp on your phone"}

// Response when authenticated
{"authenticated": true, "message": "Client is already authenticated"}
```

#### POST /pair-code
```json
// Request
{"phone": "+237XXXXXXXXX"}

// Response
{"success": true, "phone": "237XXXXXXXXX", "pairingCode": "ABCD-EFGH", "message": "Enter this code on your phone..."}
```

#### POST /send
```json
// Request
{"phone": "+237XXXXXXXXX", "message": "Your OTP is 123456"}

// Response
{"success": true, "messageId": "3EB0...", "to": "237XXXXXXXXX@c.us"}
```

#### GET /info
```json
// Response (when authenticated)
{"phoneNumber": "237XXXXXXXXX", "platform": "safari", "pushname": "Your Name"}
```
