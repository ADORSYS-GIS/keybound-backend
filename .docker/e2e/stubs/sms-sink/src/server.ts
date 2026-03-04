import Fastify, { FastifyInstance } from 'fastify';

interface Message {
  id: string;
  phone: string;
  otp: string;
  timestamp: string;
}

const app: FastifyInstance = Fastify({ logger: true });

const inbox: Message[] = [];

const now = () => new Date().toISOString();

app.post('/otp', async (req, reply) => {
  const { phone, otp } = req.body as { phone?: string; otp?: string };

  if (!phone || !otp) {
    reply.code(400).send({ error: 'phone and otp are required' });
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

app.post('/__admin/reset', async (_, reply) => {
  inbox.length = 0;
  reply.send({ reset: true });
});

const port = Number(process.env.PORT ?? 8081);

app.listen({ port, host: '0.0.0.0' }).catch((err) => {
  app.log.error(err);
  process.exit(1);
});
