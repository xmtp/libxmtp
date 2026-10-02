import * as platformPath from "node:path";

// Tests supply the Windows path implementation on a non-Windows host.
export function packageAsset(root, requestPath, paths = platformPath) {
  const file = paths.resolve(root, `.${decodeURIComponent(requestPath)}`);
  const relative = paths.relative(root, file);
  if (
    relative === "" ||
    relative === ".." ||
    relative.startsWith(`..${paths.sep}`) ||
    paths.isAbsolute(relative)
  )
    return undefined;
  return file;
}
