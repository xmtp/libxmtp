import { uniffiInitAsync } from "../../../target/sdk-generated/typescript-wasm/index.ts";
import {
  initPureWasm,
  sdkVersion,
} from "../../../target/sdk-generated/typescript-pure/index.ts";
import * as PurePublic from "../../../target/sdk-generated/typescript-pure/public-api.gen.ts";

const wasm = new URL(
  "../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
  import.meta.url,
);
await uniffiInitAsync(wasm);
await initPureWasm();
if (!sdkVersion().startsWith("1.12.")) throw new Error("pure WASM did not initialize");
// The pure public entry works over public values and throws public errors.
const encoded = PurePublic.encodeText("pure public");
if (!(encoded.content instanceof Uint8Array))
  throw new Error("pure public bytes are not a Uint8Array");
const decoded = PurePublic.decodeStandard(encoded);
if (decoded.kind !== "text" || decoded.value !== "pure public")
  throw new Error("pure public decode did not return the text union");
if (new PurePublic.TextCodec().decode(encoded) !== "pure public")
  throw new Error("pure public TextCodec did not decode");
let invalid: unknown;
try {
  PurePublic.decodeStandard({ ...encoded, content: new Uint8Array([0xff]) });
} catch (error) {
  invalid = error;
}
if (!(invalid instanceof PurePublic.XmtpError) || "tag" in invalid)
  throw new Error("pure public decode did not throw the public XmtpError");
console.log("XMTP SDK worker and pure WASM initialized");
