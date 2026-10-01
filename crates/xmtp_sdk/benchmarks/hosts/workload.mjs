import { enrichLive } from "./live.mjs";

// Equal application work for the Node process and real Chromium worker.
export async function seed(api, fixture, paths, stream) {
  const state = {
    senderKey: api.newKey(),
    senderPath: paths.sender,
    receiverKey: api.newKey(),
    receiverPath: paths.receiver,
    ids: [],
    eventIds: [],
  };
  const sender = await api.create(state.senderKey, state.senderPath);
  let receiver;
  try {
    state.senderInbox = api.inbox(sender);
    if (stream) {
      receiver = await api.create(state.receiverKey, state.receiverPath);
      state.receiverInbox = api.inbox(receiver);
    }
    const group = await api.createGroup(
      sender,
      receiver ? [state.receiverInbox] : [],
    );
    state.groupId = api.groupId(group);
    if (receiver) {
      await api.syncConversations(receiver);
      await api.group(receiver, state.groupId);
    }
    for (const row of fixture.messages) {
      const id = await api.prepare(group, row, state.ids, state.senderInbox);
      state.ids.push(id);
      state.eventIds.push(id);
      for (const reaction of row.reactions) {
        state.eventIds.push(
          await api.prepareReaction(group, id, state.senderInbox, reaction),
        );
      }
    }
    if (!stream) {
      await api.publish(group);
      await group.sync();
    }
    return state;
  } finally {
    if (receiver) await api.close(receiver);
    await api.close(sender);
  }
}

export async function measure(api, fixture, state, workload, coldPath) {
  if (workload === "cold_start" || workload.startsWith("callback_")) {
    const key = api.newKey();
    let callbackStart;
    let callbackCount = 0;
    const start = performance.now();
    const client = await api.create(
      key,
      coldPath,
      workload === "callback_slow" ? fixture.callback_delay_ms : 0,
      (time) => {
        callbackStart = time;
        callbackCount += 1;
      },
    );
    const end = performance.now();
    await api.close(client);
    if (workload.startsWith("callback_") && callbackCount === 0)
      throw new Error("Signer callback was not invoked");
    return {
      duration_ms:
        end - (workload.startsWith("callback_") ? callbackStart : start),
      completed: true,
      callback_count: callbackCount,
    };
  }
  const sender = await api.open(
    state.senderKey,
    state.senderPath,
    state.senderInbox,
  );
  let receiver;
  let stream;
  try {
    const group = await api.group(sender, state.groupId);
    const keyById = new Map(state.ids.map((id, i) => [id, String(i)]));
    if (workload === "page") {
      const start = performance.now();
      const messages = (await api.page(group, 1000)).map((message) =>
        api.normalize(message, keyById),
      );
      return {
        duration_ms: performance.now() - start,
        observed_messages: messages,
      };
    }
    if (workload !== "stream") throw new Error(`Unknown workload ${workload}`);
    receiver = await api.open(
      state.receiverKey,
      state.receiverPath,
      state.receiverInbox,
    );
    const receivedGroup = await api.group(receiver, state.groupId);
    stream = await api.stream(receiver, receivedGroup);
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const expectedEvents = new Set(state.eventIds);
    const seen = new Set();
    const live = [];
    const start = performance.now();
    // Publishing and reading run together. The complete operation is timed.
    const publisher = api.publish(group);
    const consumer = (async () => {
      for await (const message of stream) {
        if (expectedEvents.has(message.id) && !seen.has(message.id)) {
          live.push(api.live(message));
          seen.add(message.id);
        }
        if (seen.size === expectedEvents.size) break;
      }
      if (seen.size !== expectedEvents.size)
        throw new Error("Stream ended with missing messages");
    })();
    await Promise.all([publisher, consumer]);
    await stream.end();
    stream = undefined;
    const messages = enrichLive(live, state.ids);
    return {
      duration_ms: performance.now() - start,
      observed_messages: messages,
      streamed_events: seen.size,
      streamed_primary: state.ids.length,
    };
  } finally {
    if (stream) await stream.end();
    if (receiver) await api.close(receiver);
    await api.close(sender);
  }
}
