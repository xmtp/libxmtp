import { router, useNavigationContainerRef, usePathname } from "expo-router";
import { useEffect, useRef } from "react";
import { Appearance } from "react-native";

import {
  closedSessionRoute,
  openSessionRoute,
  type RouteAction,
} from "@/core/sessionRoute.ts";
import { useAppearance, useSessionState } from "@/session";

type Navigation = ReturnType<typeof useNavigationContainerRef>;

interface StateNode {
  readonly key?: string;
  readonly type?: string;
  readonly index?: number;
  readonly routeNames?: readonly string[];
  readonly routes: readonly { readonly state?: StateNode }[];
}

/** The stack in `app/_layout.tsx`: the first stack that has Chats. */
function rootStack(navigation: Navigation): StateNode | undefined {
  let state: StateNode | undefined = navigation.getRootState();
  while (state !== undefined) {
    if (state.type === "stack" && state.routeNames?.includes("chats") === true)
      return state;
    state = state.routes[state.index ?? 0]?.state;
  }
  return undefined;
}

/**
 * Runs a route action. A root switch pops the root stack to its first screen
 * and replaces it, closing pushed
 * screens and sheets (`RouteOwner.replace` clears the path and the sheet).
 */
function apply(navigation: Navigation, action: RouteAction): void {
  if (action.kind === "stay") return;
  const stack = rootStack(navigation);
  if (stack?.key !== undefined && stack.routes.length > 1) {
    navigation.dispatch({ type: "POP_TO_TOP", target: stack.key });
  }
  router.replace(action.path);
  if (action.push !== undefined) router.push(action.push);
}

/**
 * Follows the session (RootView.swift): the root screen changes with the
 * session state, and the appearance setting overrides the system color
 * scheme (`.preferredColorScheme`). Renders nothing; mount it once inside
 * `SessionProvider` next to the root stack.
 */
export function SessionRouter() {
  const state = useSessionState();
  const appearance = useAppearance();
  const pathname = usePathname();
  const navigation = useNavigationContainerRef();
  const path = useRef(pathname);
  useEffect(() => {
    path.current = pathname;
  }, [pathname]);

  useEffect(() => {
    Appearance.setColorScheme(
      appearance === "system" ? "unspecified" : appearance,
    );
  }, [appearance]);

  // The first state is `starting` (stay); later states arrive after the
  // root stack has mounted.
  useEffect(() => {
    if (state.kind !== "active") {
      apply(navigation, closedSessionRoute(state.kind, path.current));
      return;
    }
    const session = state.session;
    const effect: { live: boolean } = { live: true };
    void session.initialRoute().then((initial) => {
      if (!effect.live || !session.isCurrent()) return;
      apply(navigation, openSessionRoute(initial, path.current));
    });
    return () => {
      effect.live = false;
    };
  }, [state, navigation]);

  return null;
}
