// Which root screen the session state shows: the startup screen while it
// restores, the session's first screen, and the Connect root for every state
// without an open session.

/** The screens that replace the stack root (`router.replace`). */
export type RootPath = "/" | "/connect" | "/choose-handle" | "/chats";

/** The first screen of an open session (`ActiveSession.initialRoute`). */
export type InitialRoute = "choose-handle" | "chats";

/** The session state kinds that decide the route. */
export type SessionRouteState =
  | "starting"
  | "storageFailed"
  | "signedOut"
  | "opening"
  | "active"
  | "restoreFailed"
  | "resetIncomplete";

export type RouteAction =
  | { readonly kind: "stay" }
  /** Replace the stack with `path`, then push `push` (a deep link) on top. */
  | {
      readonly kind: "replace";
      readonly path: RootPath;
      readonly push?: string;
    };

const STAY: RouteAction = { kind: "stay" };

const ROOT_PATHS: ReadonlySet<string> = new Set<RootPath>([
  "/",
  "/connect",
  "/choose-handle",
  "/chats",
]);

/** Debug tools (`/dev/...`) open in any state and are never redirected. */
export function isDevPath(path: string): boolean {
  return path === "/dev" || path.startsWith("/dev/");
}

function replace(path: RootPath, current: string): RouteAction {
  return current === path ? STAY : { kind: "replace", path };
}

/**
 * The route for a state without an open session. `starting` and `opening`
 * keep the current screen (the startup spinner, or Connect while it joins).
 */
export function closedSessionRoute(
  state: Exclude<SessionRouteState, "active">,
  path: string,
): RouteAction {
  if (isDevPath(path)) return STAY;
  switch (state) {
    case "starting":
    case "opening":
      return STAY;
    case "storageFailed":
      return replace("/", path);
    case "signedOut":
    case "restoreFailed":
    case "resetIncomplete":
      return replace("/connect", path);
  }
}

/**
 * The route when a session opens. A root screen (startup, Connect) gives way
 * to the session's first screen. A deep link that launched the app (for
 * example `/chat/<id>`) is pushed again on top of Chats, so Back leads to
 * Chats; Choose handle comes first, so it drops the link.
 */
export function openSessionRoute(
  initial: InitialRoute,
  path: string,
): RouteAction {
  if (isDevPath(path)) return STAY;
  const root: RootPath = initial === "chats" ? "/chats" : "/choose-handle";
  if (ROOT_PATHS.has(path)) return replace(root, path);
  return root === "/chats"
    ? { kind: "replace", path: root, push: path }
    : { kind: "replace", path: root };
}
