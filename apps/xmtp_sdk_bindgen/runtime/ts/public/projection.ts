import { ObjectProjection, installProjection } from "../../public-values.gen";
import type { Message as RuntimeMessage } from "../message";
import { boundMessageOf } from "./host";
import { boundMessage, liftBoundMessage, type Message } from "./message";

/** Converts host messages; the generated base converts every object. */
class HostProjection extends ObjectProjection {
  liftMessage(value: RuntimeMessage): Message {
    return liftBoundMessage(boundMessageOf(value));
  }

  lowerMessage(value: Message): RuntimeMessage {
    return boundMessage(value);
  }
}

installProjection(new HostProjection());
