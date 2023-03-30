import init, {
  InitInput,
  e2e_selftest,
} from "./pkg/libxmtp.js";

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

export class XMTPv3 {
  constructor() {}

  // Self test
  public selfTest(): boolean {
    return e2e_selftest();
  }
}


// Manages the Wasm module, which loads a singleton version of our Rust code
export class XMTPWasm {
  private constructor() {}

  // Get a new XMTPv3 instance
  getXMTPv3(): XMTPv3 {
    return new XMTPv3();
  }

  /**
   * There is a one time global setup fee (sub 30ms), but subsequent
   * requests to initialize will be instantaneous, so it's not imperative to reuse the same parser.
   */
  static initialize = async (options?: PackageLoadOptions) => {
    if (initialized === undefined) {
      //@ts-ignore
      const loadModule = options?.wasm ?? wasmInit();
      initialized = init(loadModule).then(() => void 0);
    }

    await initialized;
    return new XMTPWasm();
  };

  /**
   * Resets initialization so that one can initialize the module again. Only
   * intended for tests.
   */
  static resetModule = () => {
    initialized = undefined;
  };
}
