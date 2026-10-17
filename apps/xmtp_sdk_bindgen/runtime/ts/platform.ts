import {
  StorageLocation,
  StorageLocation_Tags,
  type ClientOptions,
  type ClientLike,
} from "../xmtp_sdk";

declare const process: { cwd(): string } | undefined;

export function resolvedOptions(options: ClientOptions): ClientOptions {
  if (options.storage.location.tag !== StorageLocation_Tags.Default)
    return options;
  const directory =
    typeof process === "undefined" ? "xmtp-sdk" : `${process.cwd()}/xmtp`;
  return {
    ...options,
    storage: {
      ...options.storage,
      location: new StorageLocation.Directory({ directory }),
    },
  };
}

/** Native process hosts may retain an owner across JS runtime teardown. */
export function retainPlatformClient(_client: object): void {}
export function releasePlatformClient(_client: object): void {}

/** Roll back a native client that no public wrapper could accept. */
export async function discardPlatformClient(client: ClientLike): Promise<void> {
  await client.end();
}
