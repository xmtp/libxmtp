import {
  StorageLocation,
  StorageLocation_Tags,
  XmtpError,
  ErrorCategory,
  type ClientOptions,
} from "../xmtp_sdk";
import { defaultStorageDirectory } from "./native-module";

/** Keep explicit paths and keys unchanged; the app sandbox supplies defaults. */
export function resolvedOptions(options: ClientOptions): ClientOptions {
  if (options.storage.location.tag !== StorageLocation_Tags.Default)
    return options;
  const directory = defaultStorageDirectory();
  if (directory.length === 0) {
    throw new XmtpError.StorageLocationRequired({
      code: "StorageLocationRequired",
      category: ErrorCategory.Storage,
      retryable: false,
      message:
        "Default storage needs an app sandbox and application identifier",
    });
  }
  return {
    ...options,
    storage: {
      ...options.storage,
      location: new StorageLocation.Directory({ directory }),
    },
  };
}

// The pointer is private binding state. It never appears in the public API.
import { pointerLiteralSymbol } from "@ubjs/core";

import {
  Client as BoundClient,
  sdkDiscardUnreturnedClient,
  type ClientLike,
} from "../xmtp_sdk";

declare const globalThis: {
  __xmtpNativeOwners: {
    track(pointer: bigint): void;
    untrack(pointer: bigint): void;
  };
};

function pointer(client: ClientLike): bigint {
  if (!BoundClient.instanceOf(client))
    throw new TypeError("not a native SDK client");
  return client[pointerLiteralSymbol];
}

export function retainPlatformClient(client: ClientLike): void {
  globalThis.__xmtpNativeOwners.track(pointer(client));
}
export function releasePlatformClient(client: ClientLike): void {
  globalThis.__xmtpNativeOwners.untrack(pointer(client));
}

export async function discardPlatformClient(client: ClientLike): Promise<void> {
  await sdkDiscardUnreturnedClient(client);
}
