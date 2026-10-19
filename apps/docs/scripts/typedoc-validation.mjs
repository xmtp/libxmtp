import { Converter } from "typedoc";

import { assertTypeDocValidation } from "./validation-lib.mjs";

export function typeDocValidation(label) {
  return (app) => {
    // Starlight replaces app.logger after bootstrap. Keep its diagnostics too.
    const bootstrapLogger = app.logger;
    app.converter.on(Converter.EVENT_END, ({ project }) => {
      app.validate(project);
      assertTypeDocValidation(bootstrapLogger, label);
      assertTypeDocValidation(app.logger, label);
    });
  };
}
