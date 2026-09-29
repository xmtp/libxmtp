import { ObjectProjection, installProjection } from "../../public-values.gen";
import type { BoundMessage } from "./host";
import { boundMessage, liftBoundMessage, type Message } from "./message";

/** Converts host messages; the generated base converts every object. */
class HostProjection extends ObjectProjection {
  liftMessage(value: BoundMessage): Message {
    return liftBoundMessage(value);
  }

  lowerMessage(value: Message): BoundMessage {
    return boundMessage(value);
  }
}

installProjection(new HostProjection());
