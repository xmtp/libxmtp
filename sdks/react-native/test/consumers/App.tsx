import { useEffect, useState } from "react";
import { Linking, Platform, Text, View } from "react-native";

import { assert } from "./assertions";
import { backgroundJourney } from "./background";
import { installedHarnessSha256 } from "./build-identity";
import {
  InstalledReactSmokeHost,
  runInstalledReactSmoke,
} from "./installed-react";
import { foundationJourney, type JourneyControl } from "./journey";
import { claimLaunch } from "./launch";
import {
  prepareRuntimeReload,
  resumeAfterRuntimeReload,
  runtimeLaunchURL,
} from "./runtime";

async function run(url: string): Promise<void> {
  if (!claimLaunch(url)) return;
  const parameters = new Map(
    url
      .slice(url.indexOf("?") + 1)
      .split("&")
      .map((item) => {
        const [key, value = ""] = item.split("=");
        return [decodeURIComponent(key!), decodeURIComponent(value)];
      }),
  );
  const endpoint = parameters.get("control");
  const token = parameters.get("token");
  assert(
    endpoint && token,
    "native launch must supply the test control endpoint and token",
  );
  const control: JourneyControl = {
    async call<Result>(operation: string, input?: unknown): Promise<Result> {
      const response = await fetch(`${endpoint}/${operation}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify(input ?? {}),
      });
      if (!response.ok)
        throw new Error(
          `control ${operation} failed with HTTP ${response.status}`,
        );
      return (await response.json()) as Result;
    },
  };
  try {
    const runtime = globalThis as typeof globalThis & {
      HermesInternal?: unknown;
      nativeFabricUIManager?: unknown;
    };
    assert(runtime.HermesInternal, "installed consumer must execute Hermes");
    const mode = parameters.get("mode");
    if (mode === "background") {
      const assertions = await backgroundJourney(control);
      await control.call("result", {
        status: "passed",
        platform: Platform.OS,
        mode,
        harnessSha256: installedHarnessSha256,
        phases: ["cold-background"],
        assertions,
      });
      return;
    }
    assert(
      runtime.nativeFabricUIManager,
      "installed consumer must execute New Architecture",
    );
    assert(
      mode === "foundation" || mode === "conformance",
      "native launch must request an admitted mode",
    );
    const conformance = mode === "conformance";
    const result =
      conformance && parameters.get("phase") === "reloaded"
        ? await resumeAfterRuntimeReload(control)
        : await foundationJourney(control, conformance);
    if (conformance && parameters.get("phase") !== "reloaded") {
      await prepareRuntimeReload(control, url, result);
    }
    const reactAssertions = conformance
      ? await runInstalledReactSmoke(control)
      : [];
    await control.call("result", {
      status: "passed",
      platform: Platform.OS,
      ...result,
      mode,
      harnessSha256: installedHarnessSha256,
      phases: [...result.phases, ...(conformance ? ["react"] : [])],
      assertions: [
        "Hermes runtime",
        "New Architecture Fabric runtime",
        ...result.assertions,
        ...reactAssertions,
      ],
    });
  } catch (error) {
    await control.call("result", {
      status: "failed",
      platform: Platform.OS,
      mode: parameters.get("mode"),
      harnessSha256: installedHarnessSha256,
      message: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}

export default function App() {
  const [status, setStatus] = useState("Waiting for native test launch");
  useEffect(() => {
    const launch = (url: string | null | undefined) => {
      if (url)
        void run(url).then(
          () => setStatus("Passed"),
          () => setStatus("Failed"),
        );
    };
    const replacement = runtimeLaunchURL();
    if (replacement) launch(replacement);
    else void Linking.getInitialURL().then(launch);
    const subscription = Linking.addEventListener("url", ({ url }) =>
      launch(url),
    );
    return () => subscription.remove();
  }, []);
  return (
    <View>
      <Text testID="xmtp-result">{status}</Text>
      <InstalledReactSmokeHost />
    </View>
  );
}
