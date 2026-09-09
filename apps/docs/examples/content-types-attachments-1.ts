declare function createImageFile(): File;

// #region example1
import { CommandRouter, type AttachmentUploadCallback } from '@xmtp/agent-sdk';
import { PinataSDK } from 'pinata';

const router = new CommandRouter();

router.command('/send-image', async (ctx) => {
  const file = createImageFile();

  const uploadCallback: AttachmentUploadCallback = async (attachment) => {
    const pinata = new PinataSDK({
      pinataJwt: `${process.env.PINATA_JWT}`,
      pinataGateway: `${process.env.PINATA_GATEWAY}`,
    });

    const mimeType = 'application/octet-stream';
    const encryptedBlob = new Blob([Buffer.from(attachment.payload)], {
      type: mimeType,
    });
    const encryptedFile = new File(
      [encryptedBlob],
      attachment.filename || 'untitled',
      {
        type: mimeType,
      },
    );
    const upload = await pinata.upload.public.file(encryptedFile);

    return pinata.gateways.public.convert(`${upload.cid}`);
  };

  await ctx.sendRemoteAttachment(file, uploadCallback);
});
// #endregion example1
