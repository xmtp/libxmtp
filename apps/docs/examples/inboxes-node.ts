import {
  Client,
  XmtpError,
  type Backend,
  type Group,
  type Signer,
} from "@xmtp/node-sdk";

export async function manageInboxes(
  client: Client,
  inboxIds: string[],
  backend: Backend,
  installationIds: string[],
  signer: Signer,
) {
  // #region manage
  const state = await client.inboxState(true);
  const states = await Client.inboxStates(inboxIds, backend);
  await client.revokeInstallations(signer, installationIds);
  await client.revokeAllOtherInstallations(signer);
  // #endregion manage
  return { state, states };
}

// #region hint
/** Return a hint for the name that the user typed, or undefined. */
export function handleHint(client: Client, typed: string) {
  const handle = typed.toLowerCase();
  if (!/^[a-z][a-z0-9_-]{0,26}(#[0-9]{4})?$/.test(handle))
    return "Start with a letter. Use letters, digits, _ and -.";
  // A hint only: the backend decides each claim.
  const { minNameLength } = client.serverConfiguration.handles;
  const [name = ""] = handle.split("#");
  if (name.length < minNameLength)
    return `Use at least ${minNameLength} characters before #.`;
  return undefined;
}
// #endregion hint

// #region claim
/**
 * Let the user pick a free handle, then claim it. `pick` shows the
 * candidates and returns the user's choice, or undefined to cancel. After
 * another inbox takes the choice first, `pick` also gets that handle.
 */
export async function chooseHandle(
  client: Client,
  name: string,
  pick: (candidates: string[], taken?: string) => Promise<string | undefined>,
) {
  let candidates = await client.availableHandles(name);
  let taken: string | undefined;
  for (;;) {
    const choice = await pick(candidates, taken);
    if (choice === undefined) return undefined;
    try {
      await client.claimHandle(choice);
      return choice;
    } catch (error) {
      // Show other errors to the user. Never claim a handle they did not pick.
      if (!(error instanceof XmtpError.HandleTaken)) throw error;
      taken = choice;
      candidates = await client.availableHandles(name);
    }
  }
}
// #endregion claim

// #region display
/** Name each member by its current handle. Other handles are secondary. */
export async function memberLabels(group: Group) {
  const members = await group.members();
  return members.map((member) => ({
    inboxId: member.inboxId,
    title: member.handle ?? member.inboxId,
    alsoKnownAs: member.otherHandles,
  }));
}

/** Find an inbox by a handle. An old handle also finds it. */
export async function findByHandle(client: Client, typed: string) {
  const inboxId = await client.inboxIdFor({
    identifier: typed,
    kind: "handle",
  });
  if (inboxId === undefined) return undefined;
  const [state] = await client.inboxStates([inboxId], true);
  // Show the current handle, also when the user typed an old one.
  return { inboxId, title: state?.handle ?? inboxId };
}
// #endregion display
