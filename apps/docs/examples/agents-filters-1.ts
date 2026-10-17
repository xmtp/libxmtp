import { Agent } from "@xmtp/agent-sdk";
const agent = await Agent.createFromEnv();

// #region example1
import { filter } from "@xmtp/agent-sdk";

agent.on("message", async (ctx) => {
  if (filter.fromSelf(ctx.message, ctx.client)) return;
  if (
    filter.hasContent(ctx.message) &&
    ctx.isText() &&
    (await filter.isGroupAdminAsync(ctx.conversation, ctx.message))
  ) {
    await ctx.sendTextReply("Received text");
  }
});

await agent.start();
// #endregion example1
