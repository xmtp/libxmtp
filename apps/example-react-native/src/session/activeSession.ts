// One open session: its SDK client, preferences, stream hub and caches.

import type {
  Client,
  ClientEvent,
  Conversation,
  EventKind,
  Message,
  ServerConfiguration,
} from "@xmtp/react-native-sdk";

import { normalizeBackendUrl } from "@/core/backendUrl.ts";
import { effectiveHandleMinimum } from "@/core/handles.ts";
import { IdentityDirectory } from "@/core/identityDirectory.ts";
import type { ProfilePaths } from "@/core/paths.ts";
import { ProfilePreferences } from "@/core/preferences.ts";
import { SendJournal } from "@/core/sendJournal.ts";
import type { ActiveProfile, BindRequest } from "@/core/sessionController.ts";
import type { SessionKey } from "@/core/sessionFence.ts";
import type { SessionScope } from "@/core/sessionScope.ts";
import { ValueStore } from "@/core/store.ts";
import {
  type StreamReader,
  type StreamSource,
  StreamHub,
} from "@/core/streamHub.ts";
import { removePath } from "@/platform/files";
import { DevicePreferenceFiles } from "@/session/deviceStorage";
import {
  type OpenedClient,
  allowsPrivateAttachments,
} from "@/session/sdkClients";

/** The signed-in user's handles (Settings, Change handle). */
export interface OwnHandles {
  /** False until the first read finished. */
  readonly loaded: boolean;
  readonly current?: string;
  readonly others: readonly string[];
}

/** Adapts the SDK readers to the stream hub. */
function streamSource(client: Client): StreamSource<Message, Conversation> {
  return {
    messages: (onState) =>
      client.conversations.streamAllMessages({
        onConnectionStateChange: (_previous, current) => {
          onState(current);
        },
      }),
    conversations: () => client.conversations.stream(),
    events: async (
      kinds: readonly EventKind[],
    ): Promise<StreamReader<ClientEvent>> => {
      const events = await client.events({ kinds: [...kinds] });
      return {
        next: () => events.next(),
        end: async () => {
          await events.return();
        },
      };
    },
  };
}

export class ActiveSession {
  readonly client: Client;
  readonly key: SessionKey;
  readonly inboxId: string;
  readonly profileId: string;
  readonly generation: number;
  readonly profile: ActiveProfile;
  readonly paths: ProfilePaths;
  /** Session-owned work. The session cancels and awaits it before ending the client. */
  readonly scope: SessionScope;
  readonly preferences: ProfilePreferences;
  readonly streams: StreamHub<Message, Conversation>;
  /** Inbox → handle cache, updated by handle-changed events. */
  readonly identities: IdentityDirectory;
  readonly ownHandles = new ValueStore<OwnHandles>({
    loaded: false,
    others: [],
  });
  /** True when attachment URLs may be private (launch opt-in and a loopback backend). */
  readonly attachmentsAllowPrivateNetwork: boolean;
  readonly #isCurrent: () => boolean;

  constructor(
    request: BindRequest<OpenedClient>,
    options: { readonly foreground: boolean },
  ) {
    this.client = request.client.client;
    this.key = request.key;
    this.inboxId = request.profile.inboxId;
    this.profileId = request.profile.id;
    this.generation = request.key.generation;
    this.profile = request.profile;
    this.scope = request.scope;
    this.#isCurrent = request.isCurrent;
    this.paths = request.paths;
    this.preferences = new ProfilePreferences(
      new DevicePreferenceFiles(this.paths.preferences),
    );
    this.attachmentsAllowPrivateNetwork = allowsPrivateAttachments(
      normalizeBackendUrl(request.profile.url),
    );
    try {
      // Exported copies of verified files never outlive a session.
      removePath(this.paths.exports);
    } catch (error) {
      console.warn("Cannot remove exported files", error);
    }
    if (request.profile.name !== undefined) {
      try {
        this.preferences.setServerLabel(request.profile.name);
      } catch (error) {
        console.warn("Cannot save the server name", error);
      }
    }
    this.identities = new IdentityDirectory(
      async (ids) =>
        (await this.client.inboxStates([...ids], false)).map((state) => ({
          inboxId: state.inboxId,
          ...(state.handle === undefined ? {} : { handle: state.handle }),
          otherHandles: state.otherHandles,
        })),
      () => this.isCurrent(),
    );
    this.streams = new StreamHub(streamSource(this.client), {
      foreground: options.foreground,
      isCurrent: () => this.isCurrent(),
    });
    this.streams.onEvent((event) => {
      if (event.kind !== "handleChanged") return;
      this.identities.handleChanged(event.inboxId, event.handle);
      if (event.inboxId === this.inboxId) {
        this.ownHandles.set({
          loaded: true,
          current: event.handle,
          others: this.identities.identity(event.inboxId)?.otherHandles ?? [],
        });
      }
    });
    this.scope.addCleanup(() => this.streams.close());
  }

  /** True while this session is open. With a generation, while that generation is. */
  isCurrent(generation: number = this.generation): boolean {
    return generation === this.generation && this.#isCurrent();
  }

  /** The server name: the saved label, else its URL. */
  get server(): { readonly url: string; readonly name: string } {
    let label: string | undefined;
    try {
      label = this.preferences.serverLabel;
    } catch {
      label = undefined;
    }
    return {
      url: this.profile.url,
      name: label ?? this.profile.name ?? this.profile.url,
    };
  }

  /** The cached server configuration, or undefined when an offline open has none. */
  serverConfiguration(): ServerConfiguration | undefined {
    try {
      return this.client.serverConfiguration;
    } catch {
      return undefined;
    }
  }

  /** The handle minimum length on this server (3 when unknown or out of range). */
  handleMinimum(): number {
    const minimum = this.serverConfiguration()?.handles.minNameLength;
    return minimum === undefined ? 3 : effectiveHandleMinimum(minimum);
  }

  /** The upload limit in bytes, or undefined when the server offers no attachments. */
  maxUploadBytes(): bigint | undefined {
    return this.serverConfiguration()?.attachments?.maxUploadBytes;
  }

  /** The send and delete journal (`send-drafts`). */
  sendJournal(): SendJournal {
    return new SendJournal(this.preferences.sendJournalStorage());
  }

  /** Reads the own handles (`inboxState(false)`) and publishes them. */
  async refreshOwnHandles(): Promise<OwnHandles> {
    const state = await this.client.inboxState(false);
    const handles: OwnHandles = {
      loaded: true,
      ...(state.handle === undefined ? {} : { current: state.handle }),
      others: state.otherHandles,
    };
    if (this.isCurrent()) {
      this.ownHandles.set(handles);
      this.identities.merge([
        {
          inboxId: this.inboxId,
          ...(state.handle === undefined ? {} : { handle: state.handle }),
          otherHandles: state.otherHandles,
        },
      ]);
    }
    return handles;
  }

  /**
   * The first screen after a join or restore (`chooseInitialRoute`): Choose
   * handle when the inbox has no handle and the user did not skip; else Chats
   * (also when the handle read fails).
   */
  async initialRoute(): Promise<"choose-handle" | "chats"> {
    try {
      const handles = await this.refreshOwnHandles();
      return handles.current === undefined && !this.preferences.skippedHandle
        ? "choose-handle"
        : "chats";
    } catch {
      return "chats";
    }
  }
}
