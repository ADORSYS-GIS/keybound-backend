import { request } from 'undici';
import { env } from './env';

interface TokenResponse {
  access_token: string;
  expires_in: number;
}

let cached: { token: string; expiresAt: number } | null = null;

export async function getClientToken(): Promise<string> {
  const now = Date.now();
  if (cached && now < cached.expiresAt - 5000) {
    return cached.token;
  }

  const params = new URLSearchParams({
    grant_type: 'client_credentials',
    client_id: env.keycloakClientId,
    client_secret: env.keycloakClientSecret,
  });

  const url = `${env.keycloakUrl}/realms/e2e-testing/protocol/openid-connect/token`;
  const response = await request(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/x-www-form-urlencoded',
    },
    body: params.toString(),
  });

  const text = await response.body.text();
  if (response.statusCode >= 300) {
    throw new Error(`Keycloak token request failed (${response.statusCode}): ${text}`);
  }

  const payload = JSON.parse(text) as TokenResponse;
  const expiresIn = payload.expires_in ?? 60;
  cached = {
    token: payload.access_token,
    expiresAt: now + expiresIn * 1000,
  };

  return payload.access_token;
}
