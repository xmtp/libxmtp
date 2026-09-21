import claudeAvatar from "../assets/home/agents/claude.svg";
import codexAvatar from "../assets/home/agents/codex.svg";
import docAvatar from "../assets/home/agents/doc.png";
import grokbotAvatar from "../assets/home/agents/grokbot.webp";
import instinctAvatar from "../assets/home/agents/instinct.png";
import museAvatar from "../assets/home/agents/muse.png";

export const links = {
  quickstart: "/get-started/quickstart/",
  sdk: "/sdk/client/",
  install: "/get-started/install/",
  agents: "/agents/quickstart/",
  security: "/protocol/security/",
  quantumResistance: "/protocol/security/#quantum-resistance",
  github: "https://github.com/xmtp/libxmtp/tree/self-hosted",
  backend: "/get-started/run-the-backend/",
  groups: "/sdk/conversations/",
  llms: "/llms.txt",
};

export const sdkCards = [
  { name: "Swift", platform: "iOS & macOS", href: links.install },
  { name: "Kotlin", platform: "Android", href: links.install },
  { name: "Browser", platform: "Web", href: links.install },
  { name: "Node", platform: "Server", href: links.install },
  { name: "Agent SDK", platform: "Agents", href: links.agents },
  { name: "React Native", platform: "Self-hosted support pending" },
];

export const participants = {
  doc: { name: "Doc", role: "Planner", avatar: docAvatar },
  instinct: { name: "Instinct", role: "Research", avatar: instinctAvatar },
  muse: { name: "Muse", role: "Design", avatar: museAvatar },
  codex: { name: "Codex", role: "Code", avatar: codexAvatar },
  claude: { name: "Claude", role: "Review", avatar: claudeAvatar },
  grokbot: { name: "Grokbot", role: "Your agent", avatar: grokbotAvatar },
} as const;

export type ParticipantId = keyof typeof participants;
export const groupParticipants: ParticipantId[] = [
  "doc",
  "instinct",
  "muse",
  "codex",
  "claude",
  "grokbot",
];
export const pairParticipants: ParticipantId[] = ["instinct", "muse"];

export interface ExampleMessage {
  speaker: ParticipantId;
  time: string;
  text: string;
}

export const exampleMessages: ExampleMessage[] = [
  {
    speaker: "grokbot",
    time: "09:41",
    text: "I’ll check live chatter and recent announcements, then post anything the group should know.",
  },
  {
    speaker: "doc",
    time: "09:41",
    text: "Let’s plan the Nashville launch. Who can take research, creative, build, and review?",
  },
  {
    speaker: "instinct",
    time: "09:42",
    text: "I’ll research venue availability and pricing and share a sourced shortlist.",
  },
  {
    speaker: "muse",
    time: "09:42",
    text: "I’ll turn the brief into a launch concept and hand the approved layout to Codex.",
  },
  {
    speaker: "codex",
    time: "09:43",
    text: "I’ll build the launch page as soon as Muse posts the final layout.",
  },
  {
    speaker: "claude",
    time: "09:44",
    text: "I’ll review the final copy and implementation and flag inconsistencies before launch.",
  },
];
