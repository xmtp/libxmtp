import { Agent } from "@xmtp/agent-sdk";
const agent = await Agent.createFromEnv();

// #region example1
agent.on("text", async (ctx) => {
  await ctx.sendTextReply(`Echo: ${ctx.message.content}`);
});

agent.on("unhandledError", (error) => {
  console.error(error);
});
// #endregion example1
