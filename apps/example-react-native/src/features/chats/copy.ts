export const chatsCopy = {
  title: "Chats",
  newMessage: "New message",
  newMessagePending: "New message arrives in a later change.",
  sdkVersion: (version: string) => `XMTP SDK ${version}`,
  backend: (url: string) => `Backend ${url}`,
  noBackend: "No backend URL in the launch environment",
} as const;
