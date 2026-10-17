import { ObjectProjection, installProjection } from "../../public-values.gen";
import type { Message as RuntimeMessage } from "../message";
import { publicClient } from "./client";
import { boundMessageOf, streamOwner } from "./host";
import { boundMessage, liftBoundMessage, type Message } from "./message";
import { installStreamOwner } from "./streams";

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

installStreamOwner((source, ownerKey) => {
  const owner = streamOwner(source, ownerKey);
  return owner === undefined ? undefined : publicClient(owner);
});
