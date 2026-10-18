/** Witness only HTTP transport completion/cancellation, never native callback drain. */
export function runtimeResponseSettlement(response) {
  if (response.destroyed) return Promise.resolve("closed");
  return new Promise((resolve) => {
    const finish = () => {
      response.off("close", close);
      resolve("finished");
    };
    const close = () => {
      response.off("finish", finish);
      resolve("closed");
    };
    response.once("finish", finish);
    response.once("close", close);
  });
}

/** Process-local control state survives the consumer's genuine Hermes reload. */
export function runtimeControl(setup, sign) {
  let prepared;
  let resume;
  let heldSigns = 0;
  let staleCalls = 0;
  let signOutcome = "pending";
  let responseOutcome = "pending";
  let observing = false;
  let resumed = false;
  let observationComplete = false;
  let settleSign;
  let settleResponse;
  const signSettled = new Promise((resolve) => {
    settleSign = resolve;
  });
  const responseSettled = new Promise((resolve) => {
    settleResponse = resolve;
  });
  const snapshot = () => ({
    heldSigns,
    staleCalls,
    signOutcome,
    responseOutcome,
    observing,
  });
  let noteHeld;
  const held = new Promise((resolve) => {
    noteHeld = resolve;
  });
  const release = new Promise((resolve) => {
    resume = resolve;
  });
  const dispatch = async (path, input, responseFinished) => {
    switch (path) {
      case "/runtime/setup":
        return setup;
      case "/runtime/hold-sign":
        heldSigns++;
        if (heldSigns !== 1)
          throw new Error("only one old wallet callback may be held");
        if (!responseFinished)
          throw new Error("held signer needs an HTTP settlement witness");
        void responseFinished.then((outcome) => {
          responseOutcome = outcome;
          settleResponse();
        });
        noteHeld();
        await release;
        try {
          const result = await sign(input.text);
          signOutcome = "fulfilled";
          return result;
        } catch (error) {
          signOutcome = "rejected";
          throw error;
        } finally {
          settleSign();
        }
      case "/runtime/prepare":
        await held;
        if (prepared) throw new Error("runtime preparation cannot repeat");
        prepared = input;
        return {};
      case "/runtime/resume":
        if (!prepared)
          throw new Error(
            "replacement runtime arrived before held native constructor",
          );
        resumed = true;
        resume();
        return prepared;
      case "/runtime/stale":
        staleCalls++;
        return {};
      case "/runtime/observe":
        if (!prepared || observing)
          throw new Error("runtime observation needs one prepared reload");
        observing = true;
        await signSettled;
        await responseSettled;
        // A cancelled old HTTP request is terminal too. Neither transport
        // outcome proves Rust/JS callback drain; retain a bounded late-call window.
        await new Promise((resolve) => setTimeout(resolve, 2000));
        observationComplete = true;
        return { ...snapshot(), observationMs: 2000 };
      case "/runtime/status":
        return snapshot();
      default:
        throw new Error(`unknown runtime control operation ${path}`);
    }
  };
  return Object.assign(dispatch, {
    assertObservation() {
      if (
        !resumed ||
        !observationComplete ||
        signOutcome !== "fulfilled" ||
        !["finished", "closed"].includes(responseOutcome) ||
        staleCalls !== 0
      )
        throw new Error(
          "reload result requires completed successful signer observation",
        );
    },
  });
}
