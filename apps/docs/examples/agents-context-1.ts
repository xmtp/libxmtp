import { Agent } from "@xmtp/agent-sdk";
const agent = await Agent.createFromEnv();

// #region example1
agent.on("message", async (ctx) => {
  if (ctx.isText()) {
    const sender = await ctx.getSenderAddress();
    await ctx.sendTextReply(`Hello ${sender ?? "there"}`);
  }
});
// #endregion example1
