import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { basename, dirname, relative } from "node:path";
import { docsLoader } from "@astrojs/starlight/loaders";
import { prepareDocument } from "./prepare.mjs";

export function siteLoader() {
  return {
    name: "xmtp-docs",
    async load(context) {
      await docsLoader().load(context);
      const specs = new URL("../../docs/specs/", context.config.root);
      const glossary = new URL(
        "../../docs/error_glossary.md",
        context.config.root,
      );
      const files = (await readdir(specs)).filter((name) =>
        name.endsWith(".md"),
      );
      async function loadFile(url, id, spec) {
        const filename = url.pathname.split("/").at(-1);
        const { title, body, order } = prepareDocument(
          await readFile(url, "utf8"),
          filename,
          { spec },
        );
        const filePath = relative(
          fileURLToPath(context.config.root),
          fileURLToPath(url),
        );
        const data = await context.parseData({
          id,
          filePath: fileURLToPath(url),
          data: {
            title,
            sidebar: { order },
            editUrl: `https://github.com/xmtp/libxmtp/edit/main/docs/${spec ? `specs/${filename}` : filename}`,
          },
        });
        // Rendering uses the route ID; source metadata keeps the real file path.
        const renderURL = new URL(
          `src/content/docs/${id}.md`,
          context.config.root,
        );
        context.store.set({
          id,
          data,
          body,
          filePath,
          digest: context.generateDigest(body),
          rendered: await context.renderMarkdown(body, { fileURL: renderURL }),
        });
      }
      const inputs = files.map((name) => ({
        url: new URL(name, specs),
        id: `specs/${name.replace(/\.md$/u, "").replaceAll("_", "-")}`,
        spec: true,
      }));
      inputs.push({
        url: glossary,
        id: "reference/error-glossary",
        spec: false,
      });
      for (const input of inputs)
        await loadFile(input.url, input.id, input.spec);
      if (context.watcher) {
        const specsPath = fileURLToPath(specs).replace(/\/$/u, "");
        context.watcher.add([specsPath, fileURLToPath(glossary)]);
        function inputFor(path) {
          if (path === fileURLToPath(glossary)) return inputs.at(-1);
          if (dirname(path) !== specsPath || !path.endsWith(".md")) return;
          const name = basename(path);
          return {
            url: new URL(name, specs),
            id: `specs/${name.replace(/\.md$/u, "").replaceAll("_", "-")}`,
            spec: true,
          };
        }
        async function update(path) {
          const input = inputFor(path);
          if (input) await loadFile(input.url, input.id, input.spec);
        }
        context.watcher.on("add", update);
        context.watcher.on("change", update);
        context.watcher.on("unlink", (path) => {
          const input = inputFor(path);
          if (input) context.store.delete(input.id);
        });
      }
    },
  };
}
