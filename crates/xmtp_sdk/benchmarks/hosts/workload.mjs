// The same workloads for the Node process and the Chromium page.

// Create the sender (and the receiver for a stream), a group, and the fixture
// messages. Page publishes them now; stream publishes inside the timer.
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

export async function measure(api, state, workload, coldPath) {
  if (workload === "cold_start") {
    const key = api.newKey();
    const start = performance.now();
    const client = await api.create(key, coldPath);
    const end = performance.now();
    await api.close(client);
    return {
      duration_ms: end - start,
      timing_window: { start_ms: start, end_ms: end },
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
      const end = performance.now();
      return {
        duration_ms: end - start,
        timing_window: { start_ms: start, end_ms: end },
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
    // An untimed grace period lets the subscription start.
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const expected = new Set(state.eventIds);
    const seen = new Set();
    const start = performance.now();
    // Publishing and reading run together. The complete operation is timed.
    const publisher = api.publish(group);
    const consumer = (async () => {
      const iterator = stream[Symbol.asyncIterator]();
      while (seen.size !== expected.size) {
        const { value: message, done } = await iterator.next();
        if (done) break;
        if (expected.has(message.id)) {
          if (seen.has(message.id))
            throw new Error("Duplicate expected stream event");
          seen.add(message.id);
        }
      }
      if (seen.size !== expected.size)
        throw new Error("Stream ended with missing messages");
    })();
    await Promise.all([publisher, consumer]);
    const end = performance.now();
    return {
      duration_ms: end - start,
      timing_window: { start_ms: start, end_ms: end },
      streamed_events: seen.size,
    };
  } finally {
    // Cleanup runs after the timer stops.
    if (stream) await stream.end();
    if (receiver) await api.close(receiver);
    await api.close(sender);
  }
}
