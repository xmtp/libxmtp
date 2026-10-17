import { createServer } from "node:http";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
const accounts = await import(pathToFileURL(resolve(process.argv[2])).href);
const port = Number(process.argv[3]);
createServer(async (request, response) => {
  try {
    if (request.method !== "POST" || request.url !== "/")
      throw new Error("Unsupported signer request");
    let body = "";
    for await (const bytes of request) {
      body += bytes;
      if (body.length > 65536) throw new Error("Request too large");
    }
    const input = JSON.parse(body);
    const key = input.key ?? accounts.generatePrivateKey().slice(2);
    const account = accounts.privateKeyToAccount(`0x${key}`);
    const value = { key, address: account.address.toLowerCase() };
    if (input.text !== undefined)
      value.signature = (
        await account.signMessage({ message: input.text })
      ).slice(2);
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(value));
  } catch (error) {
    response.writeHead(400);
    response.end(String(error));
  }
}).listen(port, "127.0.0.1", () =>
  process.stderr.write(`Benchmark signer listening on ${port}\n`),
);
