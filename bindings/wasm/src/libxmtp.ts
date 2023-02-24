import init, { InitInput, set_private_key_bundle, save_invitation, decrypt_v2 } from "./pkg/libxmtp.js";

export interface PackageLoadOptions {
  /**
   * Controls how the Wasm module is instantiated.
   */
  wasm?: InitInput;
}

let wasmInit: (() => InitInput) | undefined = undefined;
export const setWasmInit = (arg: () => InitInput) => {
  wasmInit = arg;
};

let initialized: Promise<void> | undefined = undefined;

export class XmtpApi {
  private constructor() {}

  public setPrivateKeyBundle(bundle: Uint8Array): boolean {
    return set_private_key_bundle(bundle);
  }

  public saveInvitation(invite: Uint8Array): boolean {
    return save_invitation(invite);
  }

  public decryptV2(ciphertext: Uint8Array): Uint8Array {
    return decrypt_v2(ciphertext);
  }

  /**
   * There is a one time global setup fee (sub 30ms), but subsequent
   * requests to initialize will be instantaneous, so it's not imperative to reuse the same parser.
   */
  public static initialize = async (options?: PackageLoadOptions) => {
    if (initialized === undefined) {
      //@ts-ignore
      const loadModule = options?.wasm ?? wasmInit();
      initialized = init(loadModule).then(() => void 0);
    }

    await initialized;
    return new XmtpApi();
  };

  /**
   * Resets initialization so that one can initialize the module again. Only
   * intended for tests.
   */
  public static resetModule = () => {
    initialized = undefined;
  };
}
