import init, {
  InitInput,
  new_voodoo_instance,
  create_outbound_session,
  create_inbound_session,
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

export class VoodooInstance {
  // Handle to the voodooinstance object in the Wasm module
  private handle: string = "";
  // Pointer to the XMTP Wasm
  private wasmModule: XMTPWasm;

  constructor(wasmModule: XMTPWasm, handle: string) {
    this.wasmModule = wasmModule;
    this.handle = handle;
  }

  createOutboundSession(otherHandle: string, msg: string): string {
    return this.wasmModule.createOutboundSession(this.handle, otherHandle, msg);
  }

  createInboundSession(otherHandle: string, msg: string): string {
    return this.wasmModule.createInboundSession(this.handle, otherHandle, msg);
  }
}

// Keep around for old test cases
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

  newVoodooInstance(): VoodooInstance {
    const handle = new_voodoo_instance();
    return new VoodooInstance(this, handle);
  }

  createOutboundSession(sendHandle: string, receiveHandle: string, msg: string): string {
    return create_outbound_session(sendHandle, receiveHandle, msg);
  }

  createInboundSession(sendHandle: string, receiveHandle: string, msg: string): string {
    return create_inbound_session(sendHandle, receiveHandle, msg);
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
