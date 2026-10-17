// #region example1
import {
  Agent,
  CommandRouter,
  type AttachmentUploadCallback,
} from "@xmtp/agent-sdk";
import { PinataSDK } from "pinata";

const agent = await Agent.createFromEnv();
const router = new CommandRouter();
const pinataJwt = process.env.PINATA_JWT;
const pinataGateway = process.env.PINATA_GATEWAY;
if (!pinataJwt || !pinataGateway) {
  throw new Error("Set PINATA_JWT and PINATA_GATEWAY");
}

router.command("/send-file", async (ctx) => {
  const file = new File(["Hello from XMTP"], "hello.txt", {
    type: "text/plain",
  });

  const uploadCallback: AttachmentUploadCallback = async (attachment) => {
    const pinata = new PinataSDK({
      pinataJwt,
      pinataGateway,
    });

    const mimeType = "application/octet-stream";
    const encryptedBlob = new Blob([Buffer.from(attachment.payload)], {
      type: mimeType,
    });
    const encryptedFile = new File(
      [encryptedBlob],
      attachment.filename || "untitled",
      {
        type: mimeType,
      },
    );
    const upload = await pinata.upload.public.file(encryptedFile);

    return pinata.gateways.public.convert(`${upload.cid}`);
  };

  await ctx.sendRemoteAttachment(file, uploadCallback);
});
agent.use(router.middleware());
await agent.start();
// #endregion example1
