import { Converter, ReflectionKind } from "typedoc";

export function load(app) {
  app.converter.on(Converter.EVENT_RESOLVE_BEGIN, ({ project }) => {
    // Generated symbol brands enforce TypeScript identity, not an app API.
    for (const reflection of project.getReflectionsByKind(
      ReflectionKind.Property,
    )) {
      if (reflection.name === "[objectBrand]") {
        project.removeReflection(reflection);
      }
    }
  });
}
