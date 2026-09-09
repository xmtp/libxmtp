/**
 * Pre-configured URLs for the XMTP network based on the environment
 *
 * @deprecated Use `createBackend()` instead.
 * @constant
 * @property {string} local - The local URL for the XMTP network
 * @property {string} dev - The development URL for the XMTP network
 * @property {string} production - The production URL for the XMTP network
 */
export const ApiUrls = {
  local: "http://localhost:5557",
  dev: "https://api.dev.xmtp.network:5558",
  production: "https://api.production.xmtp.network:5558",
} as const;
