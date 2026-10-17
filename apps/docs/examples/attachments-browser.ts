import {
  type Client,
  type Group,
  type RemoteAttachment,
} from "@xmtp/browser-sdk";
import { initPureWasm, RemoteAttachmentCodec } from "@xmtp/browser-sdk/pure";

export async function sendAttachment(
  client: Client,
  group: Group,
  path: string,
) {
  // #region upload
  await initPureWasm();
  const attachments = client.attachments;
  if (!attachments.offered) {
    throw new Error("The backend does not offer attachments");
  }

  const pending = await attachments.create({
    kind: "path",
    path,
    filename: "photo.jpg",
    mimeType: "image/jpeg",
  });
  const remote = pending.remoteAttachment;
  await group.send(new RemoteAttachmentCodec(), remote);
  await pending.upload();
  // #endregion upload
  return remote;
}

export async function downloadAttachment(
  client: Client,
  remote: RemoteAttachment,
) {
  // #region download
  const downloaded = await client.attachments.download(remote);
  const path = downloaded.path;
  const mimeType = downloaded.mimeType;
  const filename = downloaded.filename;
  // #endregion download
  return { path, mimeType, filename };
}
