import { Agent } from '@xmtp/agent-sdk';
const agent = await Agent.createFromEnv();

// #region example2
import { downloadRemoteAttachment } from '@xmtp/agent-sdk/util';

agent.on('attachment', async (ctx) => {
  const receivedAttachment = await downloadRemoteAttachment(
    ctx.message.content,
  );
  console.log(`Received: ${receivedAttachment.filename}`);
  console.log(`Type: ${receivedAttachment.mimeType}`);
  // receivedAttachment.content contains the decrypted file bytes
});
// #endregion example2
