// #region example1
import { Agent } from "@xmtp/agent-sdk";

const agent = await Agent.createFromEnv();

agent.on("text", async (ctx) => {
  await ctx.conversation.sendText("Hello from my XMTP agent!");
});

agent.on("start", () => {
  console.log(`Agent address: ${agent.address}`);
});

await agent.start();
// #endregion example1
