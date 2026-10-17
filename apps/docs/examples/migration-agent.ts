import {
  Agent,
  AgentStreamingError,
  createIdentifier,
  createRemoteAttachmentFromFile,
  createSigner,
  createUser,
  downloadRemoteAttachment,
  filter,
  type AgentMiddleware,
} from "@xmtp/agent-sdk";

// #region create
const user = createUser();
const signer = createSigner(user);
const agent = await Agent.create(signer, {
  backend: { url: "https://your-backend.example.com" },
  storage: { location: { directory: "./agent-data" } },
  deviceSync: false,
});
// #endregion create

// #region env
const environmentAgent = await Agent.createFromEnv({
  storage: { location: { directory: "./agent-data" } },
});
// #endregion env

// #region messages
agent.on("text", async (ctx) => {
  console.log(ctx.content);
  console.log(ctx.message.content.kind);
  if (await ctx.isAllowed()) {
    await ctx.sendTextReply("Received");
    await ctx.sendReaction("👍", "unicode");
  }
});
agent.on("reply", (ctx) => {
  console.log(ctx.content.referenceId, ctx.content.body);
});
// #endregion messages

// #region middleware
const requireAdmin: AgentMiddleware = async (ctx, next) => {
  if (!(await filter.isGroupAdminAsync(ctx.conversation, ctx.message))) return;
  if (ctx.isText()) {
    ctx.content = ctx.content.trim();
  }
  await next();
};
agent.use(requireAdmin);
// #endregion middleware

// #region conversations
const peer = "0x0000000000000000000000000000000000000001";
const dm = await agent.createDmWithAddress(peer);
const group = await agent.createGroupWithAddresses([peer]);
await agent.addMembersWithAddresses(group, [
  "0x0000000000000000000000000000000000000002",
]);
const context = await agent.getConversationContext(dm.id);
console.log(context?.getClientAddress(), createIdentifier(user));
// #endregion conversations

// #region attachments
const file = new File(["hello"], "hello.txt", { type: "text/plain" });
const remote = await createRemoteAttachmentFromFile(agent.client, file);
await dm.sendRemoteAttachment(remote);
const downloaded = await downloadRemoteAttachment(agent.client, remote);
console.log(downloaded.filename);
// #endregion attachments

// #region lifecycle
agent.on("unhandledError", (error) => console.error(error));
agent.errors.use((error, _ctx, next) => {
  if (error instanceof AgentStreamingError) {
    console.error("Streams ended", error.cause);
    return;
  }
  return next(error);
});
await agent.start({
  onConnectionStateChange: (_previous, current) => console.log(current),
  onClose: (reason) => console.log(reason.kind),
});
// After the app resolves the cause of a terminal error, it can call start again.
await agent.stop();
await agent.start();
await environmentAgent.stop();
// #endregion lifecycle
