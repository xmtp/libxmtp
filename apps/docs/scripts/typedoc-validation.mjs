import { Application } from "typedoc";
import { assertTypeDocValidation } from "./validation-lib.mjs";

export async function validateTypeDoc(options, label) {
  const app = await Application.bootstrap(options);
  const project = await app.convert();
  if (!project) throw new Error(`${label} TypeDoc conversion failed.`);
  app.validate(project);
  assertTypeDocValidation(app.logger, label);
}
