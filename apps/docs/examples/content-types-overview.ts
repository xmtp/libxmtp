// #region example1
import type {
  ContentCodec,
  ContentTypeId,
  EncodedContent,
} from "@xmtp/node-sdk";

// Define the content type identifier.
export const CustomContentType: ContentTypeId = {
  authorityId: "your-domain.com",
  typeId: "your-custom-id",
  versionMajor: 1,
  versionMinor: 0,
};

// Implement a codec for string values.
export class CustomCodec implements ContentCodec<string> {
  readonly type = CustomContentType;

  encode(content: string): EncodedContent {
    return {
      type: this.type,
      parameters: new Map(),
      content: new TextEncoder().encode(content),
      fallback: content,
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
import { Agent } from "@xmtp/agent-sdk";

// Set XMTP_WALLET_KEY and XMTP_BACKEND_URL before this program runs.
const agent = await Agent.createFromEnv({
  codecs: [new CustomCodec()],
});
// #endregion example2

await agent.client.end();
