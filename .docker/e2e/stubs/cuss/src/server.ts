import Fastify, { FastifyInstance, type FastifyReply } from 'fastify';

type Endpoint = 'register' | 'approve' | 'webhook-auth-test';

interface FaultConfig {
  status: number;
  body?: Record<string, unknown>;
  count: number;
}

interface RequestRecord {
  id: string;
  endpoint: Endpoint;
  path: string;
  method: string;
  payload: Record<string, unknown>;
  timestamp: string;
}

const app: FastifyInstance = Fastify({ logger: true });

const state = {
  requests: [] as RequestRecord[],
  faults: {
    register: null as FaultConfig | null,
    approve: null as FaultConfig | null,
    'webhook-auth-test': null as FaultConfig | null,
  } as Record<Endpoint, FaultConfig | null>,
  counter: 1,
};

const now = () => new Date().toISOString();

function record(endpoint: Endpoint, path: string, method: string, payload: Record<string, unknown>) {
  const record: RequestRecord = {
    id: `${endpoint}-${state.counter}-${Date.now()}`,
    endpoint,
    path,
    method,
    payload,
    timestamp: now(),
  };
  state.requests.push(record);
}

function tryFault(endpoint: Endpoint, reply: FastifyReply): boolean {
  const fault = state.faults[endpoint];
  if (!fault || fault.count <= 0) {
    return false;
  }

  fault.count -= 1;
  if (fault.count == 0) {
    state.faults[endpoint] = null;
  }

  reply.code(fault.status).send(fault.body ?? { message: 'Injected fault' });
  return true;
}

app.post('/api/registration/register', async (req, reply) => {
  if (tryFault('register', reply)) {
    return;
  }

  const payload = req.body as Record<string, unknown>;
  const authHeader = req.headers.authorization;
  record('register', req.url, req.method, { ...payload, authHeader });

  const clientId = state.counter;
  state.counter += 1;

  reply.code(201).send({
    success: true,
    status: 'success',
    fineractClientId: clientId,
    savingsAccountId: clientId * 2,
  });
});

app.post('/api/registration/approve-and-deposit', async (req, reply) => {
  if (tryFault('approve', reply)) {
    return;
  }

  const payload = req.body as Record<string, unknown>;
  const authHeader = req.headers.authorization;
  record('approve', req.url, req.method, { ...payload, authHeader });

  reply.code(200).send({
    success: true,
    status: 'success',
    savingsAccountId: payload.savingsAccountId ?? null,
    transactionId: state.counter * 10,
  });
});

app.post('/api/webhook-auth-test', async (req, reply) => {
  const authHeader = req.headers.authorization;
  record('webhook-auth-test', req.url, req.method, { authHeader });

  reply.code(200).send({
    success: true,
    status: 'success',
  });
});

app.get('/__admin/requests', async (_, reply) => {
  reply.send(state.requests);
});

app.post('/__admin/reset', async (_, reply) => {
  state.requests = [];
  state.faults = {
    register: null,
    approve: null,
    'webhook-auth-test': null,
  };
  state.counter = 1;
  reply.send({ reset: true });
});

app.post('/__admin/faults', async (req, reply) => {
  const { endpoint, status = 500, body, count = 1 } = req.body as {
    endpoint: Endpoint;
    status?: number;
    body?: Record<string, unknown>;
    count?: number;
  };

  if (!endpoint || !['register', 'approve', 'webhook-auth-test'].includes(endpoint)) {
    reply.code(400).send({ error: 'endpoint must be register, approve, or webhook-auth-test' });
    return;
  }

  state.faults[endpoint] = { status, body, count };
  reply.code(200).send({ updated: true, endpoint });
});

const port = Number(process.env.PORT ?? 8080);

app.listen({ port, host: '0.0.0.0' }).catch((err) => {
  app.log.error(err);
  process.exit(1);
});
