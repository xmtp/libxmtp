import init, {
  InitInput,
  client_create,
  client_read_from_persistence,
  client_write_to_persistence,
  register,
} from "./pkg/bindings_wasm.js";

type AccountHandle = number;

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

/**
 * There is a one time global setup fee (sub 30ms), but subsequent
 * requests to initialize will be instantaneous, so it's not imperative to reuse the same parser.
 */
const initializeModule = async (options?: PackageLoadOptions) => {
  if (initialized === undefined) {
    //@ts-ignore
    const loadModule = options?.wasm ?? wasmInit();
    initialized = init(loadModule).then(() => void 0);
  }

  await initialized;
};

/**
 * Resets initialization so that one can initialize the module again. Only
 * intended for tests.
 */
const resetModule = () => {
  initialized = undefined;
};

export class Client {
  private clientId: number;

  private constructor(clientId: number) {
    this.clientId = clientId;
  }

  public writeToPersistence(key: string, value: Uint8Array): void {
    // Error handling is ignored here - writeToPersistence will eventually be removed
    // from the wasm interface
    client_write_to_persistence(this.clientId, key, value);
  }

  public readFromPersistence(key: string): Uint8Array | undefined {
    // Error handling is ignored here - readFromPersistence will eventually be removed
    // from the wasm interface
    return client_read_from_persistence(this.clientId, key);
  }

  public static async create(account_id: number) {
    await initializeModule();
    let clientId = client_create(account_id);
    return new Client(clientId);
  }

  public static async createTest() {
    await initializeModule();

    const account_id = register((s: String) => {
      return "VGhpc0lzQVRlc3RTdHJpbmc="; // TODO: Replace with valid signature
    });
    let clientId = client_create(account_id);
    return new Client(clientId);
  }

  public static resetAll() {
    resetModule();
  }

  public static async create_account(
    f: (s: String) => String
  ): Promise<AccountHandle> {
    await initializeModule();
    return register(f);
  }
}
