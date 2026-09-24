const worker = new Worker(new URL("./browser.worker.ts", import.meta.url), {
  type: "module",
});
console.log("conformance worker started");
worker.onmessage = (event: MessageEvent<{ result: string }>) => {
  console.log(event.data.result);
  document.body.dataset.result = event.data.result;
  if (event.data.result === "PASS" || event.data.result.startsWith("FAIL")) {
    worker.terminate();
  }
};
worker.onerror = (event) => {
  document.body.dataset.result = event.message;
  worker.terminate();
};
