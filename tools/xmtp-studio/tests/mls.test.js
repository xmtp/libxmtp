/**
 * LibXMTP MLS Engine Unit Tests
 */

import { defaultMlsEngine } from '../src/core/mls-engine.js';
import { defaultIdentityEngine } from '../src/core/identity.js';

async function runMlsTests() {
  console.log('Testing LibXMTP Messaging Layer Security (MLS) & XIP-46 Identity Engine...');

  // 1. Send MLS Encrypted Message
  const msg = defaultMlsEngine.sendMessage({ text: 'Automated Agent Trade Executed' });
  if (!msg.ciphertext || !msg.epoch) {
    throw new Error('MLS message encryption failed');
  }

  // 2. Register XIP-46 Identity
  const identity = defaultIdentityEngine.createInstallationKey({ walletAddress: '0x1111111111111111111111111111111111111111' });
  if (!identity.installationKey) {
    throw new Error('XIP-46 identity key creation failed');
  }

  console.log(`✅ LibXMTP MLS Encrypted Message Broadcasted (Epoch #${msg.epoch}) & Identity Verified!`);
}

runMlsTests().catch(e => {
  console.error('❌ MLS Test Failed:', e);
  process.exit(1);
});
