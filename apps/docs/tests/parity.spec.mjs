import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { expect, test } from "@playwright/test";

const PAGES = [
  { name: "home", old: "/", current: "/" },
  {
    name: "quickstart",
    old: "/agents/get-started/build-an-agent",
    current: "/get-started/quickstart/",
  },
  {
    name: "sdk-guide",
    old: "/chat-apps/core-messaging/send-messages",
    current: "/sdk/send-messages/",
  },
  {
    name: "protocol",
    old: "/protocol/overview",
    current: "/protocol/overview/",
  },
  {
    name: "content-types",
    old: "/chat-apps/content-types/content-types",
    current: "/content-types/overview/",
  },
  { name: "spec", current: "/specs/001-backend-api/" },
  { name: "errors", current: "/reference/error-glossary/" },
  { name: "node-reference", current: "/reference/node-sdk/" },
];

test("capture old and new parity pages with a contact sheet", async ({
  browser,
}, testInfo) => {
  test.setTimeout(180_000);
  const output = testInfo.outputPath("parity");
  await mkdir(output, { recursive: true });
  const rows = [];
  for (const theme of ["light", "dark"]) {
    for (const width of [1280, 390]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
        colorScheme: theme,
        deviceScaleFactor: 1,
      });
      const page = await context.newPage();
      for (const item of PAGES) {
        for (const site of item.old ? ["old", "new"] : ["new"]) {
          const path = site === "old" ? item.old : item.current;
          const base =
            site === "old"
              ? "https://docs.xmtp.org"
              : (process.env.DOCS_BASE_URL ?? "http://127.0.0.1:4322");
          const response = await page.goto(new URL(path, base).href, {
            waitUntil: "networkidle",
          });
          expect(response?.ok(), `${site} ${path}`).toBe(true);
          await page.addStyleTag({
            content:
              "*,*::before,*::after{animation:none!important;transition:none!important}",
          });
          const filename = `${item.name}-${site}-${theme}-${width}.png`;
          await page.screenshot({
            path: join(output, filename),
            fullPage: true,
          });
          rows.push({ item: item.name, site, theme, width, filename });
        }
      }
      await context.close();
    }
  }
  const cards = rows
    .map(
      (row) =>
        `<figure><figcaption>${row.item} · ${row.site} · ${row.theme} · ${row.width}px</figcaption><img loading="lazy" src="${row.filename}"></figure>`,
    )
    .join("");
  await writeFile(
    join(output, "index.html"),
    `<!doctype html><meta charset="utf-8"><title>Documentation parity</title><style>body{font:14px system-ui;background:#eee;color:#111}main{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}figure{margin:0;background:white;padding:8px}img{width:100%;height:auto}</style><main>${cards}</main>`,
  );
});
