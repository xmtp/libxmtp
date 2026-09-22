import { fileURLToPath } from "node:url";

import { createStarlightTypeDocPlugin } from "starlight-typedoc";

import { validateTypeDoc } from "./scripts/typedoc-validation.mjs";

const sdkRoot = new URL("../../sdks/", import.meta.url);

function sdkReference(
  packageDirectory,
  packageName,
  label,
  output,
  typeDoc = {},
) {
  const [plugin, sidebarGroup] = createStarlightTypeDocPlugin();
  const packageRoot = new URL(`${packageDirectory}/`, sdkRoot);

  const entryPoints = [fileURLToPath(new URL("src/index.ts", packageRoot))];
  const tsconfig = fileURLToPath(new URL("tsconfig.json", packageRoot));
  const typeDocOptions = {
    excludeExternals: false,
    excludePrivate: true,
    excludeProtected: true,
    readme: "none",
    treatWarningsAsErrors: true,
    ...typeDoc,
  };

  return {
    validator: {
      name: `xmtp-typedoc-validator-${packageName}`,
      hooks: {
        async "config:setup"() {
          await validateTypeDoc(
            {
              entryPoints,
              tsconfig,
              emit: "none",
              ...typeDocOptions,
            },
            label,
          );
        },
      },
    },
    plugin: plugin({
      entryPoints,
      tsconfig,
      output: `reference/${output}`,
      sidebar: { label, collapsed: true },
      typeDoc: typeDocOptions,
    }),
    sidebarGroup,
  };
}

const references = [
  sdkReference("node", "node-sdk", "Node SDK", "node-sdk"),
  sdkReference("browser", "browser-sdk", "Browser SDK", "browser-sdk", {
    // Worker implementation types are not package exports.
    intentionallyNotExported: [
      "WorkerConversation",
      "WorkerBridge",
      "ClientWorkerAction",
    ],
  }),
  sdkReference("agent", "agent-sdk", "Agent SDK", "agent-sdk", {
    exclude: [fileURLToPath(new URL("node/src/**", sdkRoot))],
  }),
];

export const referencePlugins = references.flatMap(({ validator, plugin }) => [
  validator,
  plugin,
]);
export const referenceSidebar = references.map(
  ({ sidebarGroup }) => sidebarGroup,
);
