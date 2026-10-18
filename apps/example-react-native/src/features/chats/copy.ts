export const chatsCopy = {
  title: "Chats",
  settings: "Settings",
  newMessage: "New message",
  sdkVersion: (version: string) => `XMTP SDK ${version}`,
  backend: (url: string) => `Backend ${url}`,
  noBackend: "No backend URL in the launch environment",
} as const;
