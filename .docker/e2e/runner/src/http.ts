import { request } from 'undici';

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitForStatus(url: string, expectedStatus = 200, attempts = 30) {
  let lastError: unknown;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      const res = await request(url);
      if (res.statusCode === expectedStatus) {
        return;
      }
      lastError = new Error(`unexpected status ${res.statusCode}`);
    } catch (err) {
      lastError = err;
    }
    await sleep(1000);
  }

  throw new Error(`service at ${url} did not return ${expectedStatus} within timeout: ${lastError}`);
}

export async function getJson<T>(url: string): Promise<T> {
  const res = await request(url);
  const body = await res.body.text();
  if (res.statusCode < 200 || res.statusCode >= 300) {
    throw new Error(`request to ${url} returned ${res.statusCode}: ${body}`);
  }

  try {
    return JSON.parse(body) as T;
  } catch (err) {
    throw new Error(`failed to parse JSON from ${url}: ${err}`);
  }
}
