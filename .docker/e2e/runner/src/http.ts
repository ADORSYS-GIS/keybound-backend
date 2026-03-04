import { request } from 'undici';

export function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export interface JsonRequestOptions {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: unknown;
}

export interface JsonResponse<T> {
  statusCode: number;
  body: T | null;
  text: string;
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

const JSON_HEADERS = { 'content-type': 'application/json' };

export async function sendJson<T>(
  opts: JsonRequestOptions,
): Promise<JsonResponse<T>> {
  const method = (opts.method ?? 'GET').toUpperCase();
  const headers: Record<string, string> = {
    ...JSON_HEADERS,
    ...(opts.headers ?? {}),
  };

  const response = await request(opts.url, {
    method,
    headers,
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  });

  const text = await response.body.text();
  let parsed: T | null = null;
  if (text) {
    try {
      parsed = JSON.parse(text) as T;
    } catch {
      parsed = null;
    }
  }

  return {
    statusCode: response.statusCode,
    body: parsed,
    text,
  };
}

export async function getJson<T>(url: string): Promise<T> {
  const res = await sendJson<T>({ url, method: 'GET' });
  if (res.statusCode < 200 || res.statusCode >= 300) {
    throw new Error(`request to ${url} returned ${res.statusCode}: ${res.text}`);
  }
  if (res.body === null) {
    throw new Error(`request to ${url} returned empty body`);
  }
  return res.body;
}
