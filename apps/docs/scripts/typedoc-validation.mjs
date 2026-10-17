import { Application } from "typedoc";

import { assertTypeDocValidation } from "./validation-lib.mjs";

export async function validateTypeDoc(options, label) {
  // Validate the same option registry as the Markdown renderer.
  const app = await Application.bootstrapWithPlugins({
    ...options,
    plugin: ["typedoc-plugin-markdown", ...(options.plugin ?? [])],
  });
  const project = await app.convert();
  if (!project) throw new Error(`${label} TypeDoc conversion failed.`);
  app.validate(project);
  assertTypeDocValidation(app.logger, label);
}
