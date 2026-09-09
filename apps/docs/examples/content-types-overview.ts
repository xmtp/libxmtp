import type { Signer } from '@xmtp/node-sdk';
declare const signer: Signer;
const backendUrl = process.env.XMTP_BACKEND_URL;
if (!backendUrl) throw new Error('Set XMTP_BACKEND_URL');

// #region example1
import type {
  ContentCodec,
  ContentTypeId,
  EncodedContent,
} from '@xmtp/content-type-primitives';

// Define the content type identifier
export const CustomContentType: ContentTypeId = {
  authorityId: 'your-domain.com',
  typeId: 'your-custom-id',
  versionMajor: 1,
  versionMinor: 0,
};

// Implement the codec as a class
export class CustomCodec implements ContentCodec<string> {
  contentType = CustomContentType;

  encode(content: string): EncodedContent {
    return {
      type: this.contentType,
      parameters: {},
      content: new TextEncoder().encode(content),
    };
  }

  decode(content: EncodedContent): string {
    return new TextDecoder().decode(content.content);
  }

  fallback(content: string): string | undefined {
    return content;
  }

  shouldPush(): boolean {
    return false;
  }
}
// #endregion example1

// #region example2
import { Agent } from '@xmtp/agent-sdk';

const client = await Agent.create(signer, {
  backendUrl,
  codecs: [new CustomCodec()],
});
// #endregion example2
