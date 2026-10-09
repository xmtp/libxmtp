import { fileURLToPath } from "node:url";

import { createStarlightTypeDocPlugin } from "starlight-typedoc";

import { sdkEntry } from "./scripts/sdk-entry.mjs";
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

  const entryPoints = [sdkEntry(fileURLToPath(packageRoot), true)];
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
  sdkReference("node", "node-sdk", "Node SDK", "node-sdk", {
    // The public ClientOptions adds codecs to this private generated base.
    // Error variants are public as XmtpError static constructors, not exports.
    // TypeDoc rejects an exception that no longer matches a referenced type.
    intentionallyNotExported: [
      "ClientOptions",
      "XmtpErrorAttachment",
      "XmtpErrorAuthRequired",
      "XmtpErrorBackendMismatch",
      "XmtpErrorCallbackFailed",
      "XmtpErrorCancelled",
      "XmtpErrorChainNotAccepted",
      "XmtpErrorChannelNotConfigured",
      "XmtpErrorClientClosed",
      "XmtpErrorClientVersionTooOld",
      "XmtpErrorCodecDecodeFailed",
      "XmtpErrorCodecEncodeFailed",
      "XmtpErrorCodecNotFound",
      "XmtpErrorConfigurationInvalid",
      "XmtpErrorConfigurationUnavailable",
      "XmtpErrorConsumerOwned",
      "XmtpErrorCredential",
      "XmtpErrorCredentialCallbackFailed",
      "XmtpErrorCredentialExhausted",
      "XmtpErrorCredentialMissing",
      "XmtpErrorCredentialRejected",
      "XmtpErrorDuplicateField",
      "XmtpErrorForeignCursor",
      "XmtpErrorIdentityMismatch",
      "XmtpErrorIdentityNotFound",
      "XmtpErrorInvalidArgument",
      "XmtpErrorInvalidCursor",
      "XmtpErrorInvalidInput",
      "XmtpErrorLagged",
      "XmtpErrorMalformedEnvelope",
      "XmtpErrorMigrationFailed",
      "XmtpErrorMigrationOutput",
      "XmtpErrorMigrationRecordRead",
      "XmtpErrorMigrationUnsupportedSchema",
      "XmtpErrorNotUserField",
      "XmtpErrorNotificationApi",
      "XmtpErrorNotificationGroup",
      "XmtpErrorNotificationNotFound",
      "XmtpErrorNotificationStorage",
      "XmtpErrorOutOfRange",
      "XmtpErrorPermissionDenied",
      "XmtpErrorPublishedButUnconfirmed",
      "XmtpErrorRecoveryExhausted",
      "XmtpErrorRequestTimeout",
      "XmtpErrorResourceExhausted",
      "XmtpErrorSigner",
      "XmtpErrorStorage",
      "XmtpErrorStorageBusy",
      "XmtpErrorStorageLocation",
      "XmtpErrorStorageLocationRequired",
      "XmtpErrorTaskRunnerDisabled",
      "XmtpErrorTypeChanged",
      "XmtpErrorTypeMismatch",
      "XmtpErrorUnimplemented",
      "XmtpErrorUnknown",
      "XmtpErrorUnknownField",
      "XmtpErrorUnsupportedType",
      "XmtpErrorUserLimitExceeded",
    ],
  }),
  sdkReference("browser", "browser-sdk", "Browser SDK", "browser-sdk", {
    // The public API uses the wasm types for these duplicate generated names.
    // Error variants are public through XmtpError, not as separate exports.
    // TypeDoc rejects an exception that no longer matches a referenced type.
    intentionallyNotExported: [
      "ClientOptions",
      "ErrorDetails",
      "PublicIdentity",
      "XmtpErrorAttachment",
      "XmtpErrorAuthRequired",
      "XmtpErrorBackendMismatch",
      "XmtpErrorCallbackFailed",
      "XmtpErrorCancelled",
      "XmtpErrorChainNotAccepted",
      "XmtpErrorChannelNotConfigured",
      "XmtpErrorClientClosed",
      "XmtpErrorClientVersionTooOld",
      "XmtpErrorCodecDecodeFailed",
      "XmtpErrorCodecEncodeFailed",
      "XmtpErrorCodecNotFound",
      "XmtpErrorConfigurationInvalid",
      "XmtpErrorConfigurationUnavailable",
      "XmtpErrorConsumerOwned",
      "XmtpErrorCredential",
      "XmtpErrorCredentialCallbackFailed",
      "XmtpErrorCredentialExhausted",
      "XmtpErrorCredentialMissing",
      "XmtpErrorCredentialRejected",
      "XmtpErrorDuplicateField",
      "XmtpErrorForeignCursor",
      "XmtpErrorIdentityMismatch",
      "XmtpErrorIdentityNotFound",
      "XmtpErrorInvalidArgument",
      "XmtpErrorInvalidCursor",
      "XmtpErrorInvalidInput",
      "XmtpErrorLagged",
      "XmtpErrorMalformedEnvelope",
      "XmtpErrorMigrationFailed",
      "XmtpErrorMigrationOutput",
      "XmtpErrorMigrationRecordRead",
      "XmtpErrorMigrationUnsupportedSchema",
      "XmtpErrorNotificationApi",
      "XmtpErrorNotificationGroup",
      "XmtpErrorNotificationNotFound",
      "XmtpErrorNotificationStorage",
      "XmtpErrorNotUserField",
      "XmtpErrorOutOfRange",
      "XmtpErrorPermissionDenied",
      "XmtpErrorPublishedButUnconfirmed",
      "XmtpErrorRecoveryExhausted",
      "XmtpErrorRequestTimeout",
      "XmtpErrorResourceExhausted",
      "XmtpErrorSigner",
      "XmtpErrorStorage",
      "XmtpErrorStorageBusy",
      "XmtpErrorStorageLocation",
      "XmtpErrorStorageLocationRequired",
      "XmtpErrorTaskRunnerDisabled",
      "XmtpErrorTypeChanged",
      "XmtpErrorTypeMismatch",
      "XmtpErrorUnimplemented",
      "XmtpErrorUnknown",
      "XmtpErrorUnknownField",
      "XmtpErrorUnsupportedType",
      "XmtpErrorUserLimitExceeded",
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
