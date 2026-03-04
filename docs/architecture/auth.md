# Auth & Security 🔐

## Why?
Security is our top priority! 🔐 We need to ensure that every request to our backend is authenticated and that sensitive interactions with Keycloak are fully verified! 🛡️

## Actual
Our authentication and authorization logic lives in `backend-auth`. It handles:
- **OIDC Discovery**: Automatically fetching and caching OIDC discovery documents and JWKS from the issuer.
- **JWT Verification**: Validating bearer tokens for BFF and Staff APIs.
- **KC Signature Verification**: Ensuring that requests from Keycloak are authentic using HMAC-SHA256 signatures. ✨

### KC Signature Mechanism
We use a canonical payload to verify Keycloak requests:
```text
timestamp + "\n" + method + "\n" + path + "\n" + body
```
The signature is checked against the `x-kc-signature` header, with a timestamp skew check via `x-kc-timestamp`. 🛡️

## Constraints
- **Alphabetical JWK Sorting**: Before serializing JWK keys for signature payloads, we always sort them alphabetically to ensure a deterministic string representation across all platforms! 📏
- **No Padding**: We use Base64URL encoding without padding for our HMAC digests. 🛠️

## Findings
By using OIDC discovery, we've eliminated the need to manually configure `jwks_url`—it's all inferred from the `issuer` now! 🎉 This makes our configuration much cleaner and easier to manage! ✨

## How to?
To configure auth for a new environment:
1. Update the `auth` section in your YAML config.
2. Ensure the `issuer` URL is correct and reachable. 🛠️
3. If using Keycloak signatures, set the secret in your environment variables. 🔐

## Conclusion
Our auth system is designed to be secure, flexible, and automated! Sleep easy knowing your data is safe with us! 😴✨
