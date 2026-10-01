// Build the same rich result from actual delivered values on both SDKs.
export function enrichLive(events, ids) {
  const byId = new Map();
  for (const event of events) {
    if (byId.has(event.id)) throw new Error("Duplicate live event");
    byId.set(event.id, event);
  }
  const keys = new Map(ids.map((id, index) => [id, String(index)]));
  const reactions = new Map(ids.map((id) => [id, []]));
  for (const event of events) {
    if (event.kind !== "reaction") continue;
    const target = reactions.get(event.reference);
    if (!target || !event.reaction)
      throw new Error("Missing live reaction target or content");
    target.push(event.reaction);
  }
  return ids.map((id) => {
    const event = byId.get(id);
    if (!event || !["text", "reply", "attachment"].includes(event.kind))
      throw new Error("Missing live primary content");
    let parent = null;
    let parentText = null;
    if (event.kind === "reply") {
      parent = keys.get(event.reference);
      const original = byId.get(event.reference);
      if (
        parent === undefined ||
        original?.kind !== "text" ||
        typeof original.text !== "string"
      )
        throw new Error("Missing delivered reply parent");
      parentText = original.text;
      if (
        event.eager_parent_text !== undefined &&
        event.eager_parent_text !== parentText
      )
        throw new Error("Eager reply parent differs from the delivered parent");
    }
    if (event.kind !== "attachment" && typeof event.text !== "string")
      throw new Error("Missing live text or reply body");
    if (event.kind === "attachment" && !event.attachment)
      throw new Error("Missing live attachment");
    const deliveredReactions = reactions.get(id);
    for (const reaction of event.eager_reactions ?? []) {
      if (
        !deliveredReactions.some(
          (value) =>
            value.content === reaction.content &&
            value.schema === reaction.schema &&
            value.action === reaction.action,
        )
      )
        throw new Error("Eager reaction differs from the delivered reactions");
    }
    return {
      key: keys.get(id),
      text: event.kind === "attachment" ? null : event.text,
      reply_to: parent,
      parent_text: parentText,
      attachment: event.attachment ?? null,
      reactions: deliveredReactions,
    };
  });
}
