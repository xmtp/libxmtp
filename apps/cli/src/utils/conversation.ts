import { Dm, Group } from "@xmtp/node-sdk";

export function isGroup(conversation: Group | Dm): conversation is Group {
  return conversation instanceof Group;
}

export function isDm(conversation: Group | Dm): conversation is Dm {
  return conversation instanceof Dm;
}

export function requireGroup(conversation: Group | Dm): Group {
  if (!isGroup(conversation)) {
    throw new Error("This command is only available for group conversations");
  }
  return conversation;
}

export function requireDm(conversation: Group | Dm): Dm {
  if (!isDm(conversation)) {
    throw new Error("This command is only available for DM conversations");
  }
  return conversation;
}

/** Read one state snapshot for CLI output. */
export async function conversationState(conversation: Group | Dm) {
  const state = await conversation.state();
  return "common" in state ? state.common : state;
}
