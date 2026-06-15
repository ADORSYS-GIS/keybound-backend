const { Client, LocalAuth } = require('whatsapp-web.js');
const qrcode = require('qrcode-terminal');
const express = require('express');
const path = require('path');

const app = express();
const port = process.env.PORT || 3000;

app.use(express.json());

// Initialize WhatsApp Client
const client = new Client({
    authStrategy: new LocalAuth({
        dataPath: process.env.DATA_PATH || path.join(__dirname, '../.wwebjs_auth')
    }),
    puppeteer: {
        args: ['--no-sandbox', '--disable-setuid-sandbox'],
        executablePath: process.env.CHROME_PATH || null,
        headless: true
    }
});

let isReady = false;

client.on('qr', (qr) => {
    console.log('QR RECEIVED', qr);
    qrcode.generate(qr, { small: true });
});

client.on('ready', () => {
    console.log('Client is ready!');
    isReady = true;
});

client.on('authenticated', () => {
    console.log('AUTHENTICATED');
});

client.on('auth_failure', msg => {
    console.error('AUTHENTICATION FAILURE', msg);
});

client.on('disconnected', (reason) => {
    console.log('Client was logged out', reason);
    isReady = false;
    client.initialize();
});

client.initialize();

// Routes
app.get('/health', (req, res) => {
    res.json({
        ready: isReady,
        authenticated: client.info ? true : false
    });
});

app.post('/send', async (req, res) => {
    const { phone, message } = req.body;

    if (!isReady) {
        return res.status(503).json({ error: 'WhatsApp client is not ready' });
    }

    if (!phone || !message) {
        return res.status(400).json({ error: 'Phone and message are required' });
    }

    try {
        // WhatsApp numbers should be in format 237xxxxxxxxx@c.us
        const formattedPhone = phone.includes('@c.us') ? phone : `${phone.replace('+', '')}@c.us`;
        const response = await client.sendMessage(formattedPhone, message);
        res.json({ success: true, messageId: response.id.id });
    } catch (error) {
        console.error('Error sending message:', error);
        res.status(500).json({ error: 'Failed to send message', details: error.message });
    }
});

app.listen(port, () => {
    console.log(`WhatsApp provider listening at http://localhost:${port}`);
});
