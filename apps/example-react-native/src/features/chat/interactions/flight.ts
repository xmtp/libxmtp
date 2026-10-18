// The send flight of one open chat (App/Features/Chat/MessageFlight.swift,
// ChatContentView.swift): which row is in flight, and where each mounted
// bubble is, so the overlay can measure its target.

import type { ComponentRef } from "react";
import type { View } from "react-native";

import { ValueStore } from "@/core/store.ts";
import type { ChatRuntime } from "@/features/chat/chatRuntime";

type ViewElement = ComponentRef<typeof View>;

export interface Flight {
  /** The row the flight lands on; its bubble is hidden until it lands. */
  readonly id: string;
  readonly text: string;
  /**
   * `fly` moves a bubble from the composer to the row; `fade` (Reduce
   * Motion) fades the row in instead.
   */
  readonly mode: "fly" | "fade";
}

export class FlightState {
  readonly flight = new ValueStore<Flight | undefined>(undefined);
  /** The mounted bubbles by row ID. */
  readonly bubbles = new Map<string, ViewElement>();

  /** Registers the bubble of `id`; returns the unregister function. */
  register(id: string, view: ViewElement): () => void {
    this.bubbles.set(id, view);
    return () => {
      if (this.bubbles.get(id) === view) this.bubbles.delete(id);
    };
  }

  /** Ends the flight of `id` (its bubble shows again). */
  land(id: string): void {
    if (this.flight.getState()?.id === id) this.flight.set(undefined);
  }
}

const states = new WeakMap<ChatRuntime, FlightState>();

/** The flight state of an open chat. */
export function flightState(runtime: ChatRuntime): FlightState {
  let state = states.get(runtime);
  if (state === undefined) {
    state = new FlightState();
    states.set(runtime, state);
  }
  return state;
}
