export const SEARCH_CASES = [
  ["quickstart", "/get-started/quickstart/"],
  ["build an agent", "/agents/quickstart/"],
  ["consent", "/sdk/consent/"],
  ["push notifications", "/sdk/push-notifications/"],
  ["content type", "/content-types/overview/"],
  ["group chat", "/sdk/conversations/"],
  ["error code", "/reference/error-glossary/"],
  ["rate limit", "/reference/limits/"],
  ["MLS", "/protocol/overview/"],
  ["stream messages", "/sdk/stream/"],
];

export async function checkSearch(pagefind, cases = SEARCH_CASES) {
  await pagefind.init();
  const failures = [];
  for (const [query, expected] of cases) {
    const response = await pagefind.search(query);
    const top = response.results[0]
      ? (await response.results[0].data()).url
      : "(none)";
    if (top !== expected)
      failures.push(`${query}: got ${top}; expected ${expected}`);
  }
  return failures;
}
