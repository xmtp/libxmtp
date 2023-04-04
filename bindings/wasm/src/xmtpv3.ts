import init, {
  InitInput,
  new_voodoo_instance,
  create_outbound_session,
  create_inbound_session,
  decrypt_message,
  encrypt_message,
  get_public_account_json,
  add_or_get_public_account_from_json,
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

type SessionResult = {
  sessionId: string;
  payload: string;
};

export class VoodooInstance {
  // Handle to the voodooinstance object in the Wasm module
  public handle: string = "";
  // Pointer to the XMTP Wasm
  private wasmModule: XMTPWasm;

  constructor(wasmModule: XMTPWasm, handle: string) {
    this.wasmModule = wasmModule;
    this.handle = handle;
  }

  async createOutboundSession(
    other: VoodooInstance,
    msg: string
  ): Promise<SessionResult> {
    const [sessionId, payload] = this.wasmModule.createOutboundSession(
      this.handle,
      other.handle,
      msg
    );

    return { sessionId, payload };
  }

  async createInboundSession(
    other: VoodooInstance,
    msg: string
  ): Promise<SessionResult> {
    const [sessionId, payload] = this.wasmModule.createInboundSession(
      other.handle,
      this.handle,
      msg
    );
    return { sessionId, payload };
  }

  async encryptMessage(sessionId: string, msg: string): Promise<string> {
    return this.wasmModule.encryptMessage(this.handle, sessionId, msg);
  }

  async decryptMessage(sessionId: string, ciphertext: string): Promise<string> {
    return this.wasmModule.decryptMessage(this.handle, sessionId, ciphertext);
  }

  toPublicJSON(): string {
    return this.wasmModule.getPublicAccountJSON(this.handle);
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

  createOutboundSession(
    sendHandle: string,
    receiveHandle: string,
    msg: string
  ): [string, string] {
    return create_outbound_session(sendHandle, receiveHandle, msg) as [
      string,
      string
    ];
  }

  createInboundSession(
    sendHandle: string,
    receiveHandle: string,
    msg: string
  ): [string, string] {
    return create_inbound_session(sendHandle, receiveHandle, msg) as [
      string,
      string
    ];
  }

  encryptMessage(
    sendHandle: string,
    sessionId: string,
    message: string
  ): string {
    return encrypt_message(sendHandle, sessionId, message);
  }

  decryptMessage(
    handle: string,
    sessionId: string,
    ciphertext: string
  ): string {
    return decrypt_message(handle, sessionId, ciphertext);
  }

  getPublicAccountJSON(handle: string): string {
    return get_public_account_json(handle);
  }

  addOrGetPublicAccountFromJSON(json: string): VoodooInstance {
    const handle = add_or_get_public_account_from_json(json);
    return new VoodooInstance(this, handle);
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
