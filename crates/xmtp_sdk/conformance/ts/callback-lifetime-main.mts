import { checkNodeCallbackLifetime } from "./node-callback-lifetime.mts";

const backendURL = process.env.XMTP_BACKEND_URL;
if (!backendURL) throw new Error("XMTP_BACKEND_URL is required");
await checkNodeCallbackLifetime(backendURL);
