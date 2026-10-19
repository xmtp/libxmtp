// Debug-only screen that drives the session runtime directly: join, restore,
// sign out, delete local data and stream hub delivery. Open it with
// `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/session-probe"`.
// Query parameters run an action: `?action=connect&url=…&token=…&name=…`,
// `restore`, `retry`, `signOut`, `delete`, `continueReset`, `probe`,
// `contend`, `allowRequests`, `handles`, `claim` (`name`) or
// `createGroup` (`name`, `members=handle,handle`); add `&n=<anything new>` to run the
// same action again.

import type { Message } from "@xmtp/react-native-sdk";
import { useXmtp } from "@xmtp/react-native-sdk/react";
import { Stack, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Button,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
  useColorScheme,
} from "react-native";

import { normalizeBackendUrl } from "@/core/backendUrl.ts";
import { errorMessage } from "@/core/sessionError.ts";
import type { SessionEvent } from "@/core/streamHub.ts";
import { launchEnvironment } from "@/platform/messengerPlatform";
import {
  probeBackend,
  sessionController,
  startSession,
  useActiveSession,
  useSessionState,
  useStore,
} from "@/session";
import type { ActiveSession } from "@/session/activeSession";

function describe(message: Message): string {
  const content = message.content;
  const body =
    content.kind === "text" || content.kind === "markdown"
      ? content.value
      : content.kind;
  return `${message.conversationId.slice(0, 8)} ${message.senderInboxId.slice(0, 8)}: ${body}`;
}

function describeEvent(event: SessionEvent): string {
  switch (event.kind) {
    case "handleChanged":
      return `handle ${event.inboxId.slice(0, 8)} → ${event.handle}`;
    case "messageDeleted":
      return `deleted ${event.messageId.slice(0, 8)} in ${event.conversationId.slice(0, 8)}`;
    case "changed":
      return `changed (${event.cause})`;
  }
}

function SessionDetails({
  session,
  log,
}: {
  session: ActiveSession;
  log: (line: string) => void;
}) {
  const connection = useStore(session.streams.connection);
  const scheme = useColorScheme();
  const handles = useStore(session.ownHandles);
  // The provider borrows the session client; it must be this session's.
  const provided = useXmtp().client;
  const [messages, setMessages] = useState<readonly string[]>([]);
  const [count, setCount] = useState(0);
  const [reading, setReading] = useState(session.streams.readingMessages);

  useEffect(() => {
    const stopMessages = session.streams.onMessage((message) => {
      setCount((value) => value + 1);
      setMessages((value) => [describe(message), ...value].slice(0, 8));
    });
    const stopConversations = session.streams.onConversation((conversation) => {
      log(`conversation ${conversation.id.slice(0, 8)}`);
    });
    const stopEvents = session.streams.onEvent((event) => {
      log(describeEvent(event));
    });
    const timer = setInterval(() => {
      setReading(session.streams.readingMessages);
    }, 500);
    return () => {
      stopMessages();
      stopConversations();
      stopEvents();
      clearInterval(timer);
    };
  }, [session, log]);

  return (
    <View testID="probe-session">
      <Text style={styles.line} testID="probe-inbox-id">
        inbox {session.inboxId}
      </Text>
      <Text style={styles.line}>profile {session.profileId}</Text>
      <Text style={styles.line}>generation {String(session.generation)}</Text>
      <Text style={styles.line}>
        server {session.server.name} ({session.server.url})
      </Text>
      <Text style={styles.line}>identity {session.profile.identity}</Text>
      <Text style={styles.line} testID="probe-connection">
        connection {connection.state ?? "—"} · banner{" "}
        {connection.banner ?? "none"}
      </Text>
      <Text style={styles.line}>
        message reader {reading ? "running" : "paused"}
      </Text>
      <Text style={styles.line}>
        handle {handles.loaded ? (handles.current ?? "none") : "…"} · others{" "}
        {handles.others.join(", ") || "none"}
      </Text>
      <Text style={styles.line}>
        private attachments {String(session.attachmentsAllowPrivateNetwork)}
      </Text>
      <Text style={styles.line} testID="probe-provider">
        XmtpProvider client{" "}
        {provided === null
          ? "none"
          : provided === session.client
            ? "is the session client"
            : "differs"}
      </Text>
      <Text
        style={[styles.heading, { color: scheme === "dark" ? "#fff" : "#000" }]}
        testID="probe-message-count">
        messages received {String(count)}
      </Text>
      {messages.map((line, index) => (
        <Text key={`${String(index)}-${line}`} style={styles.line}>
          {line}
        </Text>
      ))}
    </View>
  );
}

/** The probe. The root layout's `SessionProvider` lends the client to `XmtpProvider`. */
export default function SessionProbe() {
  const scheme = useColorScheme();
  const state = useSessionState();
  const session = useActiveSession();
  const appearance = useStore(sessionController.appearance);
  const params = useLocalSearchParams<{
    action?: string;
    url?: string;
    token?: string;
    name?: string;
    members?: string;
    n?: string;
  }>();
  const [url, setUrl] = useState(
    launchEnvironment().backendUrl ?? "http://127.0.0.1:5050",
  );
  const [token, setToken] = useState("");
  const [lines, setLines] = useState<readonly string[]>([]);
  const ran = useRef<string | undefined>(undefined);

  const log = useCallback((line: string) => {
    const time = new Date().toISOString().slice(11, 19);
    console.log(`[session-probe] ${line}`);
    setLines((value) => [`${time} ${line}`, ...value].slice(0, 30));
  }, []);

  const run = useCallback(
    async (name: string, work: () => Promise<unknown>) => {
      log(`${name}…`);
      try {
        const result = await work();
        log(
          `${name} done${result === undefined ? "" : `: ${JSON.stringify(result)}`}`,
        );
      } catch (error) {
        log(`${name} failed: ${errorMessage(error)}`);
      }
    },
    [log],
  );

  const actions = useCallback(
    (
      action: string,
      input: {
        url: string;
        token: string;
        name?: string;
        members?: string;
      },
    ) => {
      switch (action) {
        case "start":
          return run("start", () => startSession());
        case "restore":
          return run("restore", () => sessionController.restore());
        case "connect":
          return run("connect", () =>
            sessionController.connect({
              url: input.url,
              token: input.token,
              ...(input.name === undefined ? {} : { name: input.name }),
            }),
          );
        case "retry":
          return run("retryRestore", () => sessionController.retryRestore());
        case "signOut":
          return run("signOut", () => sessionController.signOut());
        case "delete":
          return run("deleteLocalData", () =>
            sessionController.deleteLocalData(),
          );
        case "continueReset":
          return run("continueReset", () => sessionController.continueReset());
        case "probe":
          return run("probe", () =>
            probeBackend(normalizeBackendUrl(input.url)),
          );
        case "contend":
          // Holds the consumer lease with a second default reader for 15 s, so
          // the hub's reader fails with ConsumerOwned and shows the retry banner.
          return run("contend", async () => {
            const current = sessionController.getState();
            if (current.kind !== "active") throw new Error("No open session");
            const { streams, client } = current.session;
            await streams.setForeground(false);
            const rival = client.conversations.streamAllMessages();
            await rival.ready();
            await streams.setForeground(true);
            await new Promise((resolve) => setTimeout(resolve, 15_000));
            await rival.end();
          });
        case "allowRequests":
          // Accepts every message request (consent unknown → allowed), for
          // checks that need allowed conversations before Requests exists.
          return run("allowRequests", async () => {
            const current = sessionController.getState();
            if (current.kind !== "active") throw new Error("No open session");
            const requests = await current.session.client.conversations.list({
              consentStates: ["unknown"],
            });
            for (const conversation of requests) {
              await conversation.updateConsentState("allowed");
            }
            return requests.length;
          });
        case "handles":
          return run("initialRoute", async () => {
            const current = sessionController.getState();
            if (current.kind !== "active") throw new Error("No open session");
            return current.session.initialRoute();
          });
        case "claim":
          // Claims the handle in `name` (before Choose handle exists).
          return run("claim", async () => {
            const current = sessionController.getState();
            if (current.kind !== "active") throw new Error("No open session");
            await current.session.client.claimHandle(input.name ?? "");
            return current.session.refreshOwnHandles();
          });
        case "createGroup":
          // Creates a group named `name` with the comma-separated handles
          // in `members`, so the app user is its super admin.
          return run("createGroup", async () => {
            const current = sessionController.getState();
            if (current.kind !== "active") throw new Error("No open session");
            const group =
              await current.session.client.conversations.createGroup(
                (input.members ?? "")
                  .split(",")
                  .filter((handle) => handle !== "")
                  .map((handle) => ({
                    kind: "handle" as const,
                    identifier: handle,
                  })),
                { name: input.name ?? "" },
              );
            return group.id;
          });
        default:
          log(`unknown action ${action}`);
          return Promise.resolve();
      }
    },
    [log, run],
  );

  useEffect(() => {
    void run("start", () => startSession());
  }, [run]);

  useEffect(() => {
    log(
      session === undefined
        ? `session ${state.kind}`
        : `session active inbox ${session.inboxId} generation ${String(session.generation)}`,
    );
  }, [log, session, state.kind]);

  useEffect(() => {
    const action = params.action;
    if (action === undefined) return;
    const id = `${action}|${params.url ?? ""}|${params.n ?? ""}`;
    if (ran.current === id) return;
    ran.current = id;
    const input = {
      url: params.url ?? url,
      token: params.token ?? token,
      ...(params.name === undefined ? {} : { name: params.name }),
      ...(params.members === undefined ? {} : { members: params.members }),
    };
    if (params.url !== undefined) setUrl(params.url);
    void actions(action, input);
  }, [
    actions,
    params.action,
    params.members,
    params.n,
    params.name,
    params.token,
    params.url,
    token,
    url,
  ]);

  if (!__DEV__) {
    return (
      <Text style={styles.line}>
        The session probe is available in Debug builds only.
      </Text>
    );
  }
  const color = scheme === "dark" ? "#fff" : "#000";
  return (
    <ScrollView
      contentContainerStyle={styles.container}
      contentInsetAdjustmentBehavior="automatic"
      testID="session-probe">
      <Stack.Screen options={{ title: "Session probe" }} />
      <Text style={[styles.heading, { color }]} testID="probe-state">
        state {state.kind}
      </Text>
      <Text style={[styles.line, { color }]}>appearance {appearance}</Text>
      <TextInput
        style={[styles.input, { color }]}
        value={url}
        onChangeText={setUrl}
        autoCapitalize="none"
        autoCorrect={false}
        testID="probe-url"
      />
      <TextInput
        style={[styles.input, { color }]}
        value={token}
        onChangeText={setToken}
        placeholder="Access token"
        autoCapitalize="none"
        autoCorrect={false}
        testID="probe-token"
      />
      <View style={styles.buttons}>
        {(
          [
            ["Join", "connect"],
            ["Probe", "probe"],
            ["Restore", "restore"],
            ["Retry", "retry"],
            ["Sign out", "signOut"],
            ["Delete", "delete"],
            ["Continue reset", "continueReset"],
            ["Route", "handles"],
            ["Contend", "contend"],
          ] as const
        ).map(([title, action]) => (
          <Button
            key={action}
            title={title}
            testID={`probe-${action}`}
            onPress={() => void actions(action, { url, token })}
          />
        ))}
      </View>
      {session === undefined ? null : (
        <SessionDetails session={session} log={log} />
      )}
      <Text style={[styles.heading, { color }]}>log</Text>
      {lines.map((line, index) => (
        <Text key={`${String(index)}-${line}`} style={[styles.line, { color }]}>
          {line}
        </Text>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { padding: 16, gap: 6 },
  heading: { fontSize: 17, fontWeight: "600", marginTop: 8 },
  line: { fontSize: 12, fontFamily: "Menlo", color: "#888" },
  input: {
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: "#888",
    borderRadius: 8,
    padding: 8,
    fontSize: 14,
  },
  buttons: { flexDirection: "row", flexWrap: "wrap" },
});
