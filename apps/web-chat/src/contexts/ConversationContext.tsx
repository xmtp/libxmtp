import {
  type ConsentState,
  type Message as XmtpMessage,
} from "@xmtp/browser-sdk";
import { createContext, useContext, useMemo, useState } from "react";

type ConversationContextType = {
  conversationId: string;
  // undefined: not read yet
  consentState: ConsentState | undefined;
  replyTarget: XmtpMessage | undefined;
  setReplyTarget: React.Dispatch<React.SetStateAction<XmtpMessage | undefined>>;
};

const ConversationContext = createContext<ConversationContextType>({
  conversationId: "",
  consentState: undefined,
  replyTarget: undefined,
  setReplyTarget: () => {},
});

export type ConversationProviderProps = React.PropsWithChildren<{
  conversationId: string;
  consentState?: ConsentState;
}>;

export const ConversationProvider: React.FC<ConversationProviderProps> = ({
  children,
  conversationId,
  consentState,
}) => {
  const [replyTarget, setReplyTarget] = useState<XmtpMessage | undefined>(
    undefined,
  );

  const value = useMemo(
    () => ({ conversationId, consentState, replyTarget, setReplyTarget }),
    [conversationId, consentState, replyTarget, setReplyTarget],
  );

  return (
    <ConversationContext.Provider value={value}>
      {children}
    </ConversationContext.Provider>
  );
};

export const useConversationContext = () => {
  const context = useContext(ConversationContext);
  if (!context.conversationId) {
    throw new Error(
      "useConversationContext must be used within a ConversationProvider",
    );
  }
  return context;
};
