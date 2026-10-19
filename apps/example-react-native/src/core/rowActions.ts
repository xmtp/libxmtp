// Row actions of the Chats and Message requests lists: the swipe group that
// keeps one row open (a SwiftUI `List` with `swipeActions`), the Android
// long-press menu, and the VoiceOver custom actions.

/** A row action: a context-menu item, a swipe button or both. */
export interface RowAction {
  readonly label: string;
  readonly destructive?: boolean;
  readonly onPress: () => void;
}

export interface Closable {
  close(): void;
}

/**
 * Keeps at most one row's swipe actions open. While a row is open, a tap
 * anywhere in the list only closes it, as in a UIKit list.
 */
export class SwipeGroup<Row extends Closable = Closable> {
  #open: Row | undefined;

  opened(row: Row): void {
    if (this.#open !== row) this.#open?.close();
    this.#open = row;
  }

  closed(row: Row): void {
    if (this.#open === row) this.#open = undefined;
  }

  /**
   * Closes the open row. Returns true when a row was open: the tap that
   * called this must do nothing else.
   */
  closeAll(): boolean {
    const open = this.#open;
    this.#open = undefined;
    open?.close();
    return open !== undefined;
  }
}

/** Android's `AlertDialog` shows at most three buttons. */
export const ANDROID_ALERT_BUTTON_LIMIT = 3;

export interface MenuAlertButton {
  readonly text: string;
  readonly style?: "cancel" | "destructive";
  readonly onPress?: () => void;
}

/**
 * The buttons of the Android long-press menu. Cancel is added while it fits
 * in the three-button limit; the alert must be cancelable in any case, so
 * Back or a tap outside closes it without an action.
 */
export function menuAlertButtons(
  actions: readonly RowAction[],
  cancel: string,
): readonly MenuAlertButton[] {
  if (actions.length > ANDROID_ALERT_BUTTON_LIMIT) {
    throw new RangeError("An Android row menu holds at most three actions.");
  }
  const buttons: MenuAlertButton[] = actions.map((action) => ({
    text: action.label,
    ...(action.destructive === true ? { style: "destructive" as const } : {}),
    onPress: action.onPress,
  }));
  if (buttons.length < ANDROID_ALERT_BUTTON_LIMIT) {
    buttons.push({ text: cancel, style: "cancel" });
  }
  return buttons;
}

/**
 * VoiceOver custom actions for a row: every menu and swipe action once, in
 * menu order (SwiftUI exposes `swipeActions` and `contextMenu` buttons on
 * the row element).
 */
export function rowAccessibilityActions(
  menu: readonly RowAction[],
  swipe: readonly RowAction[],
): readonly RowAction[] {
  const actions: RowAction[] = [];
  for (const action of [...menu, ...swipe]) {
    if (!actions.some((known) => known.label === action.label)) {
      actions.push(action);
    }
  }
  return actions;
}
