# GOWA (WhatsApp) Sidecar Integration & Interactive Testing Guide

This guide describes how to configure, pair, test, and troubleshoot the **GOWA (go-whatsapp-web-multidevice)** WhatsApp sidecar integration within the Keybound Backend's SMS Gateway.

---

## Architecture Overview

The system architecture wraps GOWA as a sidecar provider for sending OTP codes over WhatsApp:

```mermaid
flowchart LR
    App[Keybound Server] -->|REST/Redis Queue| Gateway[SMS Gateway]
    Gateway -->|POST /send/message| GOWA[GOWA Sidecar]
    GOWA -->|WhatsApp Web Protocol| WA[WhatsApp Servers]
    WA -->|OTP Message| Phone[User Device]
```

- **SMS Gateway**: Built in Rust (`app/bins/sms-gateway`). When configured with `provider: whatsapp`, it delegates all SMS sending requests to GOWA.
- **GOWA Sidecar**: A Golang-based WhatsApp multi-device REST API. Ran inside Docker using the `aldinokemal2104/go-whatsapp-web-multidevice` image.

---

## 1. Local Configuration

Ensure the WhatsApp provider settings are configured in your configuration files (e.g., [config/dev.yaml](file:///home/marco/Projects/skyengpro/keybound-backend/config/dev.yaml) or [config/local.yaml](file:///home/marco/Projects/skyengpro/keybound-backend/config/local.yaml)):

```yaml
# SMS Provider Configuration
sms:
  # Primary SMS provider (choose: "console", "sns", "avlytext", "whatsapp")
  provider: whatsapp

  # WhatsApp sidecar configuration (used when provider=whatsapp)
  whatsapp:
    # Base URL for the WhatsApp provider service (within the Docker network)
    base_url: "http://whatsapp-provider:3000"
    # Optional: Scoped device identifier. If omitted, GOWA defaults to the only registered device.
    device_id: "default"

  # Fallback provider to use if primary provider fails
  fallback:
    - provider: console
```

---

## 2. Launching GOWA Sidecar

To spin up GOWA and start the SMS Gateway:

```bash
# Start GOWA and its dependencies (Postgres, Redis, MinIO)
docker compose up -d whatsapp-provider

# Verify the container is running and check its logs
docker compose logs -f whatsapp-provider
```

> [!WARNING]
> **Port Conflict Troubleshooting:**
> If GOWA fails to start with a port binding error on port `3030`:
> 1. Check if a host process is already occupying port `3030`:
>    ```bash
>    ss -tlnp | grep 3030
>    ```
> 2. If a process like `whatsapp-native` is running on the host, terminate it:
>    ```bash
>    killall whatsapp-native
>    ```
> 3. Restart the container:
>    ```bash
>    docker compose up -d whatsapp-provider
>    ```

---

## 3. Interactive Device Pairing

GOWA must be paired with an active WhatsApp account before it can send messages. You can pair it interactively using either a **QR Code** or a **Pairing Code**.

### Step A: Register a Device Slot
First, allocate a device slot in GOWA:

```bash
curl -i -X POST \
  -H "Content-Type: application/json" \
  -d '{"device_id": "default"}' \
  http://localhost:3030/devices
```

*Expected Response (`200 OK`):*
```json
{
  "code": "SUCCESS",
  "message": "Device added",
  "results": {
    "id": "default",
    "state": "disconnected",
    ...
  }
}
```

---

### Step B: Pair WhatsApp Account

Choose one of the two pairing methods below:

#### Option 1: Pairing via QR Code (Recommended)
1. Open your browser and navigate to the GOWA status page:
   [http://localhost:3030](http://localhost:3030)
2. You can also retrieve the QR code directly via API:
   ```bash
   curl -i http://localhost:3030/devices/default/login
   ```
3. Open **WhatsApp** on your phone.
4. Go to **Settings** > **Linked Devices** > **Link a Device**.
5. Scan the QR code displayed on the screen.

#### Option 2: Pairing via Phone Number & Code
If scanning a QR code is not convenient, request a pairing code instead:

```bash
curl -i -X POST \
  "http://localhost:3030/devices/default/login/code?phone=YOUR_PHONE_NUMBER"
```
*(Ensure `YOUR_PHONE_NUMBER` includes the country code, e.g., `15551234567` or `491701234567`)*

*Expected Response (`200 OK`):*
```json
{
  "code": "SUCCESS",
  "message": "Pairing code generated",
  "results": {
    "code": "ABCD-1234"
  }
}
```

1. Open **WhatsApp** on your phone.
2. Go to **Settings** > **Linked Devices** > **Link a Device** > **Link with phone number instead**.
3. Enter the 8-character code displayed in the API response.

---

### Step C: Verify Connection Status
Verify that the device status shows `"connected"` and `"is_logged_in": true`:

```bash
curl -i http://localhost:3030/devices/default/status
```

Or query GOWA's app connection status:
```bash
curl -i -H "X-Device-Id: default" http://localhost:3030/app/status
```

---

## 4. Sending OTP via GOWA

Once your device is logged in, you can test OTP delivery.

### Option A: Testing directly via GOWA API
Test that GOWA is capable of sending messages directly:

```bash
curl -i -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "phone": "RECIPIENT_PHONE_NUMBER",
    "message": "Your verification code is: 998877"
  }' \
  "http://localhost:3030/send/message?device_id=default"
```
*(Replace `RECIPIENT_PHONE_NUMBER` with the target phone number including country code, e.g., `15559876543`)*

---

### Option B: Testing via SMS Gateway API
Test the end-to-end integration by running the local SMS Gateway and firing a request at it:

1. Start the SMS Gateway locally in development mode:
   ```bash
   just dev-sms
   ```
2. Send an OTP request to the SMS Gateway:
   ```bash
   curl -i -X POST \
     -H "Content-Type: application/json" \
     -d '{
       "phone": "RECIPIENT_PHONE_NUMBER",
       "otp": "654321"
     }' \
     http://localhost:3000/otp
   ```
3. Check the SMS Gateway terminal logs. You should see it delegate the message to the WhatsApp provider and log a successful delivery status.

---

## 5. API Troubleshooting & Common Errors

| Error Code | HTTP Status | Message | Cause / Solution |
| :--- | :--- | :--- | :--- |
| `DEVICE_ID_REQUIRED` | `400 Bad Request` | `device_id is required via X-Device-Id header...` | GOWA contains multiple device slots but no `X-Device-Id` header or `device_id` query param was specified in the request. Configure `whatsapp.device_id` in `config/dev.yaml`. |
| `INVALID_WA_CLI` | `500 Internal Error` | `your WhatsApp CLI is invalid or empty` | The device slot exists but it is not connected or logged in to WhatsApp. Follow the **Interactive Device Pairing** steps to link it. |
| `404 Not Found` | `404 Not Found` | *Empty* | A conflicting process (e.g., `whatsapp-native` running outside docker) is listening on port `3030`. Check port assignments and terminate the conflicting process. |
