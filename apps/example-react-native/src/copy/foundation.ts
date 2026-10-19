/**
 * App shell strings (English): route titles and the startup and account
 * error copy.
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

/** Navigation titles for every route. */
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
  /** Debug builds only: the design gallery. */
  gallery: "Design gallery",
  /** Debug builds only: the session probe. */
  sessionProbe: "Session probe",
  /** The Debug chat probe. */
  chatProbe: "Chat probe",
  /** The edit-group and add-member sheets. */
  editGroup: "Edit group",
  addMember: "Add member",
} as const;
