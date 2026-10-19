import { type ComponentRef, useEffect, useRef, useState } from "react";
import { StyleSheet, View } from "react-native";
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withSpring,
} from "react-native-reanimated";
import { scheduleOnRN } from "react-native-worklets";

import {
  FLIGHT_MAX_WIDTH,
  FLIGHT_START_SCALE,
  flightTarget,
} from "@/core/messageInteractions.ts";
import { useChatContext } from "@/features/chat/ChatContext";
import { DeleteMessageAlert } from "@/features/chat/interactions/DeleteMessageAlert";
import { type Flight, flightState } from "@/features/chat/interactions/flight";
import { useReducedMotion } from "@/ui/accessibility";
import { Bubble, BubbleText } from "@/ui/Bubble";
import { springConfig } from "@/ui/motion";
import { useStore } from "@/ui/useStore";

/**
 * Chat-wide pieces of the message interactions: the "Delete message?" alert for
 * `actions.deleteTarget`, and the send flight from the composer to the new row.
 * Render it once, last, in the chat screen.
 */
export function ChatInteractionsLayer() {
  const { runtime } = useChatContext();
  useFlightTrigger();
  return (
    <>
      <FlightOverlay />
      <DeleteMessageAlert runtime={runtime} />
    </>
  );
}

/**
 * Starts a flight when the newest row becomes the own row with the submitted
 * text. It runs in the timeline subscription, so the row first renders
 * already hidden.
 */
function useFlightTrigger(): void {
  const { runtime } = useChatContext();
  const reduceMotion = useReducedMotion();
  const reduceRef = useRef(reduceMotion);
  reduceRef.current = reduceMotion;
  useEffect(() => {
    const flights = flightState(runtime);
    let submitted: string | undefined;
    let count = runtime.submissions.getState()?.count ?? 0;
    let lastId = runtime.timeline.getState().messages.at(-1)?.id;
    const stopSubmissions = runtime.submissions.subscribe(() => {
      const value = runtime.submissions.getState();
      if (value !== undefined && value.count !== count) {
        count = value.count;
        submitted = value.text;
      }
    });
    const stopTimeline = runtime.timeline.subscribe(() => {
      const { messages } = runtime.timeline.getState();
      const last = messages.at(-1)?.id;
      if (last === lastId) return;
      lastId = last;
      const row = flightTarget(messages, runtime.own, submitted);
      if (row === undefined) return;
      const text = submitted ?? "";
      submitted = undefined;
      const flight: Flight = {
        id: row.id,
        text,
        mode: reduceRef.current ? "fade" : "fly",
      };
      flights.flight.set(flight);
    });
    return () => {
      stopSubmissions();
      stopTimeline();
      flights.flight.set(undefined);
    };
  }, [runtime]);
}

interface Frame {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

function measure(
  view: ComponentRef<typeof View> | null,
): Promise<Frame | undefined> {
  return new Promise((resolve) => {
    if (view === null) {
      resolve(undefined);
      return;
    }
    view.measureInWindow((x, y, width, height) => {
      resolve(
        width === 0 && height === 0 ? undefined : { x, y, width, height },
      );
    });
  });
}

/**
 * The send flight: an outgoing bubble that starts at the composer's center at
 * 92 % size and springs to its row's bubble, which stays hidden until it lands.
 * The target is measured every frame, so it follows the list's scroll to the
 * newest row.
 */
function FlightOverlay() {
  const { runtime, composerRef } = useChatContext();
  const flights = flightState(runtime);
  const flight = useStore(flights.flight);
  const overlayRef = useRef<ComponentRef<typeof View>>(null);
  const [width, setWidth] = useState<number>();
  const progress = useSharedValue(0);
  const source = useSharedValue({ x: 0, y: 0 });
  const target = useSharedValue({ x: 0, y: 0 });
  const id = flight?.mode === "fly" ? flight.id : undefined;

  useEffect(() => {
    if (id === undefined) {
      setWidth(undefined);
      return undefined;
    }
    const run = { active: true };
    let frame = 0;
    const land = () => {
      flights.land(id);
    };
    const follow = async () => {
      const [overlay, bubble] = await Promise.all([
        measure(overlayRef.current),
        measure(flights.bubbles.get(id) ?? null),
      ]);
      if (!run.active || overlay === undefined || bubble === undefined) return;
      target.value = {
        x: bubble.x - overlay.x + bubble.width / 2,
        y: bubble.y - overlay.y + bubble.height / 2,
      };
      frame = requestAnimationFrame(() => {
        void follow();
      });
    };
    // LegendList mounts a new row a frame or more after the data changes:
    // wait up to about half a second for the target bubble.
    const start = async (attempt: number): Promise<void> => {
      const [overlay, composer, bubble] = await Promise.all([
        measure(overlayRef.current),
        measure(composerRef.current),
        measure(flights.bubbles.get(id) ?? null),
      ]);
      if (!run.active) return;
      if (bubble === undefined && attempt < 30) {
        frame = requestAnimationFrame(() => {
          void start(attempt + 1);
        });
        return;
      }
      if (
        overlay === undefined ||
        composer === undefined ||
        bubble === undefined
      ) {
        land();
        return;
      }
      source.value = {
        x: composer.x - overlay.x + composer.width / 2,
        y: composer.y - overlay.y + composer.height / 2,
      };
      target.value = {
        x: bubble.x - overlay.x + bubble.width / 2,
        y: bubble.y - overlay.y + bubble.height / 2,
      };
      setWidth(Math.min(bubble.width, FLIGHT_MAX_WIDTH));
      progress.value = 0;
      progress.value = withSpring(1, springConfig, (finished) => {
        if (finished === true) scheduleOnRN(land);
      });
      void follow();
    };
    void start(0);
    return () => {
      run.active = false;
      cancelAnimationFrame(frame);
    };
  }, [composerRef, flights, id, progress, source, target]);

  const flightStyle = useAnimatedStyle(() => {
    const p = progress.value;
    const x = source.value.x + (target.value.x - source.value.x) * p;
    const y = source.value.y + (target.value.y - source.value.y) * p;
    const scale = FLIGHT_START_SCALE + (1 - FLIGHT_START_SCALE) * p;
    return {
      transform: [
        { translateX: x },
        { translateY: y },
        { translateX: "-50%" },
        { translateY: "-50%" },
        { scale },
      ],
    };
  });

  return (
    <View
      ref={overlayRef}
      style={StyleSheet.absoluteFill}
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants">
      {flight?.mode === "fly" && width !== undefined ? (
        <Animated.View style={[styles.flight, { width }, flightStyle]}>
          <Bubble isOutgoing>
            <BubbleText isOutgoing>{flight.text}</BubbleText>
          </Bubble>
        </Animated.View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  flight: { left: 0, position: "absolute", top: 0 },
});
