import Fastify, { FastifyInstance } from 'fastify';

interface Message {
  id: string;
  phone: string;
  otp: string;
  timestamp: string;
}

interface Fault {
  status: number;
  body?: unknown;
  remaining: number;
}

const app: FastifyInstance = Fastify({ logger: true });

const inbox: Message[] = [];
const faults: Fault[] = [];

const now = () => new Date().toISOString();

app.post('/otp', async (req, reply) => {
  const { phone, otp } = req.body as { phone?: string; otp?: string };

  if (!phone || !otp) {
    reply.code(400).send({ error: 'phone and otp are required' });
    return;
  }

  const activeFault = faults[0];
  if (activeFault) {
    activeFault.remaining -= 1;
    if (activeFault.remaining <= 0) {
      faults.shift();
    }
    reply.code(activeFault.status).send(
      activeFault.body ?? {
        error: `forced sms sink fault ${activeFault.status}`,
      },
    );
    return;
  }

  inbox.push({
    id: `${phone}-${Date.now()}`,
    phone,
    otp,
    timestamp: now(),
  });

  reply.code(200).send({ delivered: true });
});

app.get('/__admin/messages', async (_, reply) => {
  reply.send(inbox);
});

app.post('/__admin/faults', async (req, reply) => {
  const { status, body, count } = req.body as {
    status?: number;
    body?: unknown;
    count?: number;
  };

  if (!status || status < 100 || status > 599) {
    reply.code(400).send({ error: 'status must be a valid HTTP status code' });
    return;
  }

  const remaining = Number(count ?? 1);
  if (!Number.isFinite(remaining) || remaining <= 0) {
    reply.code(400).send({ error: 'count must be a positive number' });
    return;
  }

  faults.push({
    status,
    body,
    remaining,
  });

  reply.send({ queued: true, status, remaining });
});

app.post('/__admin/reset', async (_, reply) => {
  inbox.length = 0;
  faults.length = 0;
  reply.send({ reset: true });
});

const port = Number(process.env.PORT ?? 8081);

app.listen({ port, host: '0.0.0.0' }).catch((err) => {
  app.log.error(err);
  process.exit(1);
});
