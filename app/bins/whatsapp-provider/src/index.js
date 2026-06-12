const { Client, LocalAuth } = require('whatsapp-web.js');
const express = require('express');
const path = require('path');

const app = express();
const port = process.env.PORT || 3000;

app.use(express.json());

// State management
let isReady = false;
let currentQrCode = null;
let pairingCodeRequested = false;

// Initialize WhatsApp Client
const client = new Client({
    authStrategy: new LocalAuth({
        dataPath: process.env.DATA_PATH || path.join(__dirname, '../.wwebjs_auth')
    }),
    puppeteer: {
        args: [
            '--no-sandbox',
            '--disable-setuid-sandbox',
            '--disable-dev-shm-usage',
            '--disable-gpu',
            '--disable-software-rasterizer',
            '--disable-extensions',
            '--disable-background-networking',
            '--disable-sync',
            '--metrics-recording-only',
            '--no-first-run'
        ],
        executablePath: process.env.CHROME_PATH || null,
        headless: true,
        protocolTimeout: 300000 // 5 minutes for slow container startup
    }
});

// Event handlers
client.on('qr', (qr) => {
    console.log('QR RECEIVED:', qr);
    currentQrCode = qr;
    pairingCodeRequested = false; // Reset pair code flag when QR is shown
});

client.on('ready', () => {
    console.log('Client is ready!');
    isReady = true;
    currentQrCode = null; // Clear QR code after successful auth
});

client.on('authenticated', () => {
    console.log('AUTHENTICATED');
    currentQrCode = null;
});

client.on('auth_failure', (msg) => {
    console.error('AUTHENTICATION FAILURE:', msg);
    currentQrCode = null;
});

client.on('disconnected', (reason) => {
    console.log('Client was logged out:', reason);
    isReady = false;
    currentQrCode = null;
    // Re-initialize on disconnect
    client.initialize();
});

// Initialize client
client.initialize();

// ==================== API Routes ====================

/**
 * Health check endpoint
 * Returns ready state and authentication status
 */
app.get('/health', (req, res) => {
    res.json({
        ready: isReady,
        authenticated: client.info ? true : false,
        hasQrCode: currentQrCode !== null
    });
});

/**
 * Get current QR code
 * Returns the QR code string for frontend display
 * Frontend can use a QR code library to render it
 */
app.get('/qr', (req, res) => {
    if (isReady) {
        return res.json({
            authenticated: true,
            message: 'Client is already authenticated'
        });
    }

    if (!currentQrCode) {
        return res.status(503).json({
            error: 'QR code not yet available',
            message: 'Wait for WhatsApp client to initialize and generate QR code'
        });
    }

    res.json({
        authenticated: false,
        qrCode: currentQrCode,
        message: 'Scan this QR code with WhatsApp on your phone'
    });
});

/**
 * Request pairing code for phone number authentication
 * Alternative to QR code scanning
 * 
 * Body: { "phone": "+237XXXXXXXXX" }
 */
app.post('/pair-code', async (req, res) => {
    const { phone } = req.body;

    if (!phone) {
        return res.status(400).json({ error: 'Phone number is required' });
    }

    if (isReady) {
        return res.json({
            authenticated: true,
            message: 'Client is already authenticated'
        });
    }

    try {
        // Format phone number (remove + and spaces)
        const formattedPhone = phone.replace(/[\+\s]/g, '');
        
        // Request pairing code
        // Note: whatsapp-web.js supports pair code via client.requestPairingCode()
        const code = await client.requestPairingCode(formattedPhone);
        
        pairingCodeRequested = true;
        currentQrCode = null; // Clear QR since we're using pair code
        
        console.log(`Pairing code requested for ${formattedPhone}: ${code}`);
        
        res.json({
            success: true,
            phone: formattedPhone,
            pairingCode: code,
            message: 'Enter this code on your phone: Settings > Linked Devices > Link a Device'
        });
    } catch (error) {
        console.error('Error requesting pairing code:', error);
        res.status(500).json({
            error: 'Failed to request pairing code',
            details: error.message
        });
    }
});

/**
 * Get authentication status and available methods
 */
app.get('/auth/status', (req, res) => {
    res.json({
        authenticated: isReady,
        ready: isReady,
        hasQrCode: currentQrCode !== null,
        pairCodeRequested: pairingCodeRequested,
        phoneNumber: client.info ? client.info.wid.user : null
    });
});

/**
 * Logout and clear session
 */
app.post('/logout', async (req, res) => {
    try {
        await client.logout();
        isReady = false;
        currentQrCode = null;
        res.json({ success: true, message: 'Logged out successfully' });
    } catch (error) {
        console.error('Error during logout:', error);
        res.status(500).json({ error: 'Failed to logout', details: error.message });
    }
});

/**
 * Send WhatsApp message
 * Body: { "phone": "+237XXXXXXXXX", "message": "Your message" }
 */
app.post('/send', async (req, res) => {
    const { phone, message } = req.body;

    if (!isReady) {
        return res.status(503).json({ 
            error: 'WhatsApp client is not ready',
            authenticated: false,
            hint: 'Check /health or /qr endpoints for authentication status'
        });
    }

    if (!phone || !message) {
        return res.status(400).json({ error: 'Phone and message are required' });
    }

    try {
        // WhatsApp numbers should be in format 237xxxxxxxxx@c.us
        const formattedPhone = phone.includes('@c.us') ? phone : `${phone.replace('+', '')}@c.us`;
        const response = await client.sendMessage(formattedPhone, message);
        res.json({ 
            success: true, 
            messageId: response.id.id,
            to: formattedPhone
        });
    } catch (error) {
        console.error('Error sending message:', error);
        res.status(500).json({ 
            error: 'Failed to send message', 
            details: error.message 
        });
    }
});

/**
 * Get client info (only when authenticated)
 */
app.get('/info', (req, res) => {
    if (!isReady) {
        return res.status(503).json({ 
            error: 'Client not authenticated',
            hint: 'Use /qr or /pair-code to authenticate first'
        });
    }

    res.json({
        phoneNumber: client.info.wid.user,
        platform: client.info.platform,
        pushname: client.info.pushname
    });
});

// Start server
app.listen(port, () => {
    console.log(`WhatsApp provider listening at http://localhost:${port}`);
    console.log('Endpoints:');
    console.log('  GET  /health      - Health check');
    console.log('  GET  /qr          - Get QR code for scanning');
    console.log('  POST /pair-code  - Request pairing code for phone auth');
    console.log('  GET  /auth/status - Authentication status');
    console.log('  POST /logout      - Logout and clear session');
    console.log('  POST /send        - Send WhatsApp message');
    console.log('  GET  /info        - Get client info (when authenticated)');
});