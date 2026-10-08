import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";

import { buildSite } from "./site-freshness.mjs";

const repositoryRoot = resolve(import.meta.dirname, "../../..");
await buildSite({
  repositoryRoot,
  distRoot: join(repositoryRoot, "apps/docs/dist"),
  build: () => {
    const options = {
      cwd: join(repositoryRoot, "apps/docs"),
      stdio: "inherit",
    };
    execFileSync("pnpm", ["check:examples"], options);
    execFileSync("pnpm", ["exec", "astro", "build"], options);
  },
});
