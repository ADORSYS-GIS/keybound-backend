import { describe, expect, test } from 'vitest';
import { env } from '../env';
import { getJson, waitForStatus } from '../http';

describe('smoke e2e checks', () => {
  test('user storage health endpoint responds', async () => {
    await waitForStatus(`${env.userStorageUrl}/health`);
  });

  test('keycloak realm metadata loads', async () => {
    await waitForStatus(`${env.keycloakUrl}/realms/e2e-testing`, 200);
  });

  test('cuss stub admin APIs are reachable', async () => {
    await waitForStatus(`${env.cussUrl}/__admin/requests`);
  });

  test('sms sink admin reset works', async () => {
    await waitForStatus(`${env.smsSinkUrl}/__admin/messages`);
    const response = await getJson<{ reset: boolean }>(`${env.smsSinkUrl}/__admin/reset`);
    expect(response.reset).toBe(true);
  });
});
