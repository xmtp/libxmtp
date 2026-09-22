import { docsSchema } from "@astrojs/starlight/schema";
import { defineCollection } from "astro:content";

import { siteLoader } from "./loaders/site.mjs";

export const collections = {
  docs: defineCollection({ loader: siteLoader(), schema: docsSchema() }),
};
