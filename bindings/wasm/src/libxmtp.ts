import init, {
  InitInput,
  new_keystore,
  set_private_key_bundle,
  save_invitation,
  decrypt_v1,
  decrypt_v2,
  encrypt_v1,
  encrypt_v2,
  save_invites,
  create_invite,
  get_v2_conversations,
  get_public_key_bundle,
  get_account_address,
} from "./pkg/libxmtp.js";

import { keystore, publicKey } from '@xmtp/proto'

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

// The actual class exposed to consumers of this API
//  // Decrypt a batch of V1 messages
//  decryptV1(req: keystore.DecryptV1Request): Promise<keystore.DecryptResponse>
//  // Decrypt a batch of V2 messages
//  decryptV2(req: keystore.DecryptV2Request): Promise<keystore.DecryptResponse>
//  // Encrypt a batch of V1 messages
//  encryptV1(req: keystore.EncryptV1Request): Promise<keystore.EncryptResponse>
//  // Encrypt a batch of V2 messages
//  encryptV2(req: keystore.EncryptV2Request): Promise<keystore.EncryptResponse>
//  // Decrypt and save a batch of invite for later use in decrypting messages on the invite topic
//  saveInvites(
//    req: keystore.SaveInvitesRequest
//  ): Promise<keystore.SaveInvitesResponse>
//  // Create the sealed invite and store the Topic keys in the Keystore for later use
//  createInvite(
//    req: keystore.CreateInviteRequest
//  ): Promise<keystore.CreateInviteResponse>
//  // Get V2 conversations
//  getV2Conversations(): Promise<keystore.ConversationReference[]>
//  // Used for publishing the contact
//  getPublicKeyBundle(): Promise<publicKey.SignedPublicKeyBundle>
//  // Technically duplicative of `getPublicKeyBundle`, but nice for ergonomics
//  getAccountAddress(): Promise<string>
export class Keystore {
  // Handle to the keystore object in the Wasm module
  private handle: string = "";
  // Pointer to the XMTP Wasm
  private wasmModule: XMTPWasm;

  constructor(wasmModule: XMTPWasm, handle: string) {
    this.wasmModule = wasmModule;
    this.handle = handle;
  }

  decryptV1(request: keystore.DecryptV1Request): Promise<keystore.DecryptResponse> {
    // First, serialize the request to a Uint8Array
    const requestBytes = keystore.DecryptV1Request.encode(request).finish();
    // Then, call the Wasm module to decrypt the request
    const responseBytes = this.wasmModule.decryptV1(this.handle, requestBytes);
    // Finally, deserialize the response
    return new Promise((resolve, reject) => {
      resolve(keystore.DecryptResponse.decode(responseBytes));
    });
  }

  decryptV2(request: keystore.DecryptV2Request): Promise<keystore.DecryptResponse> {
    // First, serialize the request to a Uint8Array
    const requestBytes = keystore.DecryptV2Request.encode(request).finish();
    const responseBytes = this.wasmModule.decryptV2(this.handle, requestBytes);
    return new Promise((resolve, reject) => {
      resolve(keystore.DecryptResponse.decode(responseBytes));
    });
  }

  encryptV1(request: keystore.EncryptV1Request): Promise<keystore.EncryptResponse> {
    const requestBytes = keystore.EncryptV1Request.encode(request).finish();
    const responseBytes = this.wasmModule.encryptV1(this.handle, requestBytes);
    return new Promise((resolve, reject) => {
      resolve(keystore.EncryptResponse.decode(responseBytes));
    });
  }

  encryptV2(request: keystore.EncryptV2Request): Promise<keystore.EncryptResponse> {
    const requestBytes = keystore.EncryptV2Request.encode(request).finish();
    const responseBytes = this.wasmModule.encryptV2(this.handle, requestBytes);
    return new Promise((resolve, reject) => {
      resolve(keystore.EncryptResponse.decode(responseBytes));
    });
  }

  saveInvites(request: keystore.SaveInvitesRequest): Promise<keystore.SaveInvitesResponse> {
    const requestBytes = keystore.SaveInvitesRequest.encode(request).finish();
    const responseBytes = this.wasmModule.saveInvites(this.handle, requestBytes);
    return new Promise((resolve, reject) => {
      resolve(keystore.SaveInvitesResponse.decode(responseBytes));
    });
  }

  createInvite(request: keystore.CreateInviteRequest): Promise<keystore.CreateInviteResponse> {
    const requestBytes = keystore.CreateInviteRequest.encode(request).finish();
    const responseBytes = this.wasmModule.createInvite(this.handle, requestBytes);
    return new Promise((resolve, reject) => {
      resolve(keystore.CreateInviteResponse.decode(responseBytes));
    });
  }

  getV2Conversations(): Promise<keystore.ConversationReference[]> {
    const listResponsesSerialized = this.wasmModule.getV2Conversations(this.handle);
    let listResponses: keystore.ConversationReference[] = [];
    for (const serialized of listResponsesSerialized) {
      listResponses.push(keystore.ConversationReference.decode(serialized));
    }
    return new Promise((resolve, reject) => {
      resolve(listResponses);
    });
  }

  getPublicKeyBundle(): Promise<publicKey.SignedPublicKeyBundle> {
    const responseBytes = this.wasmModule.getPublicKeyBundle(this.handle);
    return new Promise((resolve, reject) => {
      resolve(publicKey.SignedPublicKeyBundle.decode(responseBytes));
    });
  }

  getAccountAddress(): Promise<string> {
    return new Promise((resolve, reject) => {
      resolve(this.wasmModule.getAccountAddress(this.handle));
    });
  }
}

// Manages the Wasm module, which loads a singleton version of our Rust code
export class XMTPWasm {
  private constructor() {}

  newKeystore(): Keystore {
    const handle = new_keystore();
    return new Keystore(this, handle);
  }

  newKeystoreWithBundle(bundle: Uint8Array): Keystore {
    const handle = new_keystore();
    this.setPrivateKeyBundle(handle, bundle);
    return new Keystore(this, handle);
  }

  // Temporary measure for manual management of keystore handles
  newHandle(): string {
    return new_keystore();
  }

  setPrivateKeyBundle(handle: string, bundle: Uint8Array): boolean {
    return set_private_key_bundle(handle, bundle);
  }

  saveInvitation(handle: string, invite: Uint8Array): boolean {
    return save_invitation(handle, invite);
  }

  saveInvites(handle: string, invites: Uint8Array): Uint8Array {
    return save_invites(handle, invites);
  }

  createInvite(handle: string, request: Uint8Array): Uint8Array {
    return create_invite(handle, request);
  }

  getV2Conversations(handle: string): Uint8Array[] {
    return get_v2_conversations(handle);
  }

  decryptV1(handle: string, request: Uint8Array): Uint8Array {
    return decrypt_v1(handle, request);
  }

  decryptV2(handle: string, request: Uint8Array): Uint8Array {
    return decrypt_v2(handle, request);
  }

  encryptV1(handle: string, request: Uint8Array): Uint8Array {
    return encrypt_v1(handle, request);
  }

  encryptV2(handle: string, request: Uint8Array): Uint8Array {
    return encrypt_v2(handle, request);
  }

  getPublicKeyBundle(handle: string): Uint8Array {
    return get_public_key_bundle(handle);
  }

  getAccountAddress(handle: string): string {
    return get_account_address(handle);
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
