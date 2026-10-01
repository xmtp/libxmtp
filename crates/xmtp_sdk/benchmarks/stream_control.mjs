import { publicApi } from "./hosts/sdk.mjs";
import { measure } from "./hosts/workload.mjs";

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonical(value[key])]),
    );
  return value;
}
const equal = (a, b) =>
  JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
const bytes = (hex) =>
  Uint8Array.from(hex.match(/../g).map((value) => parseInt(value, 16)));

export async function runStreamControls(fixture, target, measured = measure) {
  const records = [];
  for (const side of ["old", "new"]) {
    const modern = side === "new";
    const ids = fixture.messages.map((row) => `p${row.key}`);
    const eventIds = [];
    const events = [];
    const history = [];
    for (const row of fixture.messages) {
      const message = { id: `p${row.key}`, reactions: [] };
      let kind;
      let content;
      if (row.reply_to !== null) {
        kind = "reply";
        content = modern
          ? {
              kind,
              referenceId: `p${row.reply_to}`,
              body: { kind: "text", value: row.text },
            }
          : {
              referenceId: `p${row.reply_to}`,
              content: row.text,
              inReplyTo: { content: row.parent_text },
            };
        if (modern)
          message.inReplyToContent = { kind: "text", value: row.parent_text };
      } else if (row.attachment) {
        kind = "attachment";
        const value = {
          filename: row.attachment.filename,
          mimeType: row.attachment.mime_type,
          content: bytes(row.attachment.bytes_hex),
        };
        content = modern ? { kind, value } : value;
      } else {
        kind = "text";
        content = modern ? { kind, value: row.text } : row.text;
      }
      message.content = content;
      message.contentType = { typeId: kind };
      history.push({
        ...message,
        reactions: row.reactions.map((reaction) =>
          modern ? { reaction } : { content: reaction },
        ),
      });
      events.push(message);
      eventIds.push(message.id);
      for (const reaction of row.reactions) {
        const value = {
          id: `r${row.key}`,
          reactions: [],
          contentType: { typeId: "reaction" },
          content: modern
            ? { kind: "reaction", reference: message.id, reaction }
            : { reference: message.id, ...reaction },
        };
        events.push(value);
        eventIds.push(value.id);
      }
    }
    for (const fault of [
      "drop_content",
      "change_text",
      "change_reply",
      "change_attachment",
      "change_reaction",
      "good",
    ]) {
      const live = structuredClone(events);
      if (fault === "drop_content")
        live.forEach((value) => {
          delete value.content;
        });
      if (fault === "change_text") {
        if (modern) live[0].content.value = "corrupt live text";
        else live[0].content = "corrupt live text";
      }
      if (fault === "change_reply") {
        if (modern) live[1].content.body.value = "corrupt live reply";
        else live[1].content.content = "corrupt live reply";
      }
      if (fault === "change_attachment") {
        const value = modern ? live[2].content.value : live[2].content;
        value.content[0] ^= 255;
      }
      if (fault === "change_reaction") {
        const event = live.find((value) => value.id.startsWith("r"));
        (modern ? event.content.reaction : event.content).content =
          "corrupt live reaction";
      }
      let historyReads = 0;
      const api = {
        ...publicApi(
          {
            ReactionAction: { Added: "added" },
            ReactionSchema: { Unicode: "unicode" },
          },
          {},
          side,
          target,
          "control",
          {},
        ),
        open: async () => ({}),
        close: async () => {},
        group: async () => ({}),
        publish: async () => {},
        stream: async () => ({
          async *[Symbol.asyncIterator]() {
            yield* live;
          },
          end: async () => {},
        }),
        page: async (_, count) => {
          historyReads += 1;
          return structuredClone(history.slice(0, count));
        },
      };
      let failure;
      let result;
      try {
        result = await measured(
          api,
          fixture,
          { ids, eventIds },
          "stream",
          "unused",
        );
        if (!equal(result.observed_messages, fixture.messages))
          throw new Error(
            "Live semantic digest differs from the correct fixture",
          );
      } catch (error) {
        failure = String(error);
      }
      records.push({
        target,
        side,
        fault,
        rejected: Boolean(failure),
        failure: failure ?? null,
        history_reads: historyReads,
        correct_history_messages: history.length,
        observed_messages: result?.observed_messages.length ?? 0,
      });
      if ((fault === "good") === Boolean(failure))
        throw new Error(JSON.stringify(records));
      if (historyReads !== 0)
        throw new Error(
          `The stream result read history: ${JSON.stringify(records)}`,
        );
    }
  }
  return records;
}
