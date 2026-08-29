/**
 * LibXMTP Studio Web Server
 */

import express from 'express';
import cors from 'cors';
import path from 'path';
import { fileURLToPath } from 'url';
import { XMTP_CONFIG } from '../config.js';
import { defaultMlsEngine } from '../core/mls-engine.js';
import { defaultIdentityEngine } from '../core/identity.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WEB_ROOT = path.join(__dirname, '../../web');

const app = express();
const PORT = process.env.PORT || 3428;

app.use(cors());
app.use(express.json());
app.use(express.static(WEB_ROOT));

// 1. Config & Protocol Specs
app.get('/api/config', (req, res) => {
  res.json({
    protocol: XMTP_CONFIG.protocol,
    groups: XMTP_CONFIG.sampleGroups,
  });
});

// 2. Send Encrypted MLS Message
app.post('/api/mls/send', (req, res) => {
  try {
    const message = defaultMlsEngine.sendMessage(req.body);
    res.json({ success: true, message });
  } catch (err) {
    res.status(400).json({ error: err.message });
  }
});

// 3. Get Messages Ledger
app.get('/api/mls/messages', (req, res) => {
  res.json(defaultMlsEngine.getMessages());
});

// 4. Register XIP-46 Identity
app.post('/api/identity/register', (req, res) => {
  const result = defaultIdentityEngine.createInstallationKey(req.body);
  res.json(result);
});

if (process.env.NODE_ENV !== 'test') {
  app.listen(PORT, () => {
    console.log(`\n======================================================`);
    console.log(`💬 LibXMTP MLS Encrypted Messaging Studio Running!`);
    console.log(`🌐 Web Dashboard: http://localhost:${PORT}`);
    console.log(`🔒 Encryption: MLS (RFC 9420) + XIP-46 Multi-Wallet Keys`);
    console.log(`======================================================\n`);
  });
}

export default app;
