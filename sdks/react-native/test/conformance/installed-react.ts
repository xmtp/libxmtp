import { Client, generateLocalSigner, XmtpError } from "@xmtp/react-native-sdk";
import { XmtpProvider, useClient, useXmtp } from "@xmtp/react-native-sdk/react";
import { createElement, useEffect, useRef, useState } from "react";

import { assert, equal } from "./assertions";
import { within } from "./async";
import type { JourneyControl } from "./journey";

interface Props {
  control: JourneyControl;
  onComplete(assertions: string[]): void;
  onFailure(error: unknown): void;
}

/** Mount this in the real RN tree: the imported helpers own real native clients. */
export function InstalledReactSmoke(props: Props) {
  return createElement(XmtpProvider, null, createElement(Probe, props));
}

function Probe({ control, onComplete, onFailure }: Props) {
  const xmtp = useXmtp();
  const clientHook = useClient();
  const started = useRef(false);
  const reported = useRef(false);
  const [finished, setFinished] = useState(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void (async () => {
      const { backendUrl } = await control.call<{ backendUrl: string }>(
        "runtime/setup",
      );
      const key = await generateLocalSigner();
      const request = {
        signer: {
          identity: () => key.identity(),
          kind: () => key.kind(),
          sign: (input: Parameters<typeof key.sign>[0]) => key.sign(input),
        },
        options: {
          backend: { url: backendUrl },
          deviceSync: false,
          storage: { location: "inMemory" as const },
        },
      };
      const [first, second] = await within(
        Promise.all([xmtp.initialize(request), clientHook.initialize(request)]),
        "installed React helpers initialize",
      );
      assert(
        first instanceof Client,
        "React entry must return the root package's Client class",
      );
      equal(
        first,
        second,
        "both installed React hooks must share one native client",
      );
      assert(
        await first.isRegistered(),
        "provider-owned native client must be registered and usable",
      );
      await within(xmtp.disconnect(), "installed React disconnect");
      let closed: unknown;
      try {
        await first.isRegistered();
      } catch (error) {
        closed = error;
      }
      assert(
        closed instanceof XmtpError.ClientClosed,
        "provider disconnect must end its real native client before resolving",
      );
      setFinished(true);
    })().catch(onFailure);
  }, [control, clientHook, onFailure, xmtp]);
  useEffect(() => {
    if (!finished || reported.current) return;
    reported.current = true;
    try {
      equal(
        xmtp.client,
        null,
        "installed provider must clear the published client",
      );
      equal(
        clientHook.client,
        null,
        "both hooks must observe the disconnected state",
      );
      equal(xmtp.isLoading, false, "installed provider must settle loading");
      onComplete([
        "installed /react hooks share a real root Client and disconnect closes native ownership before clearing both hook views",
      ]);
    } catch (error) {
      onFailure(error);
    }
  }, [
    clientHook.client,
    finished,
    onComplete,
    onFailure,
    xmtp.client,
    xmtp.isLoading,
  ]);
  return null;
}

interface ProbeRequest {
  control: JourneyControl;
  resolve(value: string[]): void;
  reject(error: unknown): void;
}
let mount: ((request: ProbeRequest | null) => void) | undefined;

/** The consumer root mounts this once, in its actual native React tree. */
export function InstalledReactSmokeHost() {
  const [request, setRequest] = useState<ProbeRequest | null>(null);
  useEffect(() => {
    mount = setRequest;
    return () => {
      mount = undefined;
    };
  }, []);
  if (!request) return null;
  return createElement(InstalledReactSmoke, {
    control: request.control,
    onComplete(assertions) {
      setRequest(null);
      request.resolve(assertions);
    },
    onFailure(error) {
      setRequest(null);
      request.reject(error);
    },
  });
}

export function runInstalledReactSmoke(
  control: JourneyControl,
): Promise<string[]> {
  assert(mount, "installed consumer must mount the React smoke host");
  const publish = mount;
  return within(
    new Promise<string[]>((resolve, reject) => {
      publish({ control, resolve, reject });
    }),
    "installed React smoke",
  );
}
