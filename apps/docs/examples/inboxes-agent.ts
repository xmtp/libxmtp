import { XmtpError, type Agent } from "@xmtp/agent-sdk";

// #region agent-claim
/** Give the agent a handle for `name` once. The agent picks the code itself. */
export async function claimAgentHandle(agent: Agent, name: string) {
  // Claims are permanent and limited. Keep the handle the agent already has.
  const state = await agent.client.inboxState(false);
  if (state.handle !== undefined) return state.handle;

  for (const handle of await agent.client.availableHandles(name)) {
    try {
      await agent.client.claimHandle(handle);
      return handle;
    } catch (error) {
      // Another inbox took this handle after the lookup. Try the next one.
      if (!(error instanceof XmtpError.HandleTaken)) throw error;
    }
  }
  return undefined;
}
// #endregion agent-claim

// #region agent-helpers
export async function startByHandle(agent: Agent) {
  const dm = await agent.createDmWithHandle("nick#2341");
  const group = await agent.createGroupWithHandles(["nick", "ana"]);
  await agent.addMembersWithHandles(group, ["bo#0042"]);
  return { dm, group };
}
// #endregion agent-helpers
