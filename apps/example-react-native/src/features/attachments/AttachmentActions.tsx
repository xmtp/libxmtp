import { createContext, type Dispatch, type SetStateAction } from "react";

import type { SymbolName } from "@/ui/Symbol";

/** File actions join the enclosing message's long-press menu. */
export interface AttachmentAction {
  readonly id: string;
  readonly title: string;
  readonly symbol: SymbolName;
  readonly disabled?: boolean;
  readonly run: () => void;
}

export const AttachmentActions = createContext<
  | {
      readonly register: Dispatch<SetStateAction<readonly AttachmentAction[]>>;
      readonly open: () => void;
    }
  | undefined
>(undefined);
