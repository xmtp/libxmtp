import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
const accounts = await import(pathToFileURL(resolve(process.argv[2])).href);
let input = "";
for await (const value of process.stdin) input += value;
const request = JSON.parse(input);
const key = request.key ?? accounts.generatePrivateKey().slice(2);
const account = accounts.privateKeyToAccount(`0x${key}`);
const result = { key, address: account.address.toLowerCase() };
if (request.text !== undefined)
  result.signature = (
    await account.signMessage({ message: request.text })
  ).slice(2);
process.stdout.write(JSON.stringify(result));
