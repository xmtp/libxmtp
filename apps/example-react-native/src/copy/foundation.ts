/**
 * App shell strings: `DesignSystem/Foundation.xcstrings` (English). Route
 * titles and the startup and account error copy.
 */
export const foundationCopy = {
  appName: "XMTP Messenger",
  placeholder: "This screen is added in the next implementation stage.",
  cannotOpenAccount: "Cannot open account",
  cannotOpenApp: "Cannot open app",
  cannotOpenStorage: "Cannot open local account storage.",
  retry: "Retry",
  cancel: "Cancel",
  selectConversation: "Select a conversation",
  chooseChatFromSidebar: "Choose a chat from the sidebar.",
} as const;

/** Navigation titles for every route (`Foundation.xcstrings`). */
export const routeTitles = {
  connect: "Join a server",
  chooseHandle: "Choose a handle",
  chats: "Chats",
  requests: "Message requests",
  newConversation: "New message",
  settings: "Settings",
  changeHandle: "Change handle",
  chat: "Conversation",
  details: "Details",
  member: "Member",
  permissions: "Permissions",
  disappearing: "Disappearing messages",
  sharedMedia: "Media and files",
  groupFields: "Group fields",
  myFields: "My fields",
  reactions: "Reactions",
  emojiPicker: "Choose a reaction",
  attachmentRecovery: "Attachment recovery",
  /** Not an iOS route: the Debug design gallery. */
  gallery: "Design gallery",
  /** Not an iOS route: the edit-group and add-member sheets. */
  editGroup: "Edit group",
  addMember: "Add member",
} as const;
