import type { Credential } from "@/types/options";

import { uuid } from "./uuid";

export type AuthRequest = { action: "auth.request"; id: string };
export type AuthResponse = { action: "auth.response"; id: string } & (
  | { credential: Credential }
  | { failed: true }
);

/** Requests app-thread credentials without transferring a callback or WASM object. */
export class WorkerAuth {
  #closed = false;
  #pending = new Map<
    string,
    { resolve: (value: Credential) => void; reject: (error: Error) => void }
  >();

  constructor(private readonly send: (request: AuthRequest) => void) {}

  request = (): Promise<Credential> => {
    if (this.#closed) return Promise.reject(new Error("auth callback failed"));
    const id = uuid();
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      try {
        this.send({ action: "auth.request", id });
      } catch {
        this.#pending.delete(id);
        reject(new Error("auth callback failed"));
      }
    });
  };

  receive(response: AuthResponse) {
    const pending = this.#pending.get(response.id);
    if (!pending) return;
    this.#pending.delete(response.id);
    if ("credential" in response) pending.resolve(response.credential);
    else pending.reject(new Error("auth callback failed"));
  }

  close() {
    this.#closed = true;
    for (const pending of this.#pending.values())
      pending.reject(new Error("auth callback failed"));
    this.#pending.clear();
  }
}
