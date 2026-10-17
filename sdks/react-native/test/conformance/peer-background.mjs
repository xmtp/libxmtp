import { writeFile } from "node:fs/promises";

/** The driver, not JavaScript or an SDK test hook, changes OS lifecycle state. */
export function backgroundControl(setup, peer, phaseFile) {
  let published = false;
  let observed = false;
  return async (path, input) => {
    switch (path) {
      case "/background/setup":
        return setup;
      case "/background/send": {
        if (published) throw new Error("background peer must publish once");
        await peer.conversations.sync();
        const dm = await peer.conversations.getById(input.conversationId);
        if (!dm || dm.kind !== "dm")
          throw new Error("background peer did not receive DM");
        await dm.sync();
        const messageId = await dm.sendText(
          "background-to-foreground sentinel",
        );
        published = true;
        return { messageId };
      }
      case "/background/observe":
        if (!published)
          throw new Error(
            "background observation requires a published peer message",
          );
        await new Promise((resolve) => setTimeout(resolve, 2000));
        observed = true;
        return {};
      case "/background/foreground":
        if (!observed || !phaseFile)
          throw new Error(
            "foreground action requires a completed background observation and driver phase file",
          );
        await writeFile(phaseFile, JSON.stringify({ action: "foreground" }));
        return {};
      default:
        throw new Error(`unknown background control operation ${path}`);
    }
  };
}
