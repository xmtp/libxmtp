import type { Client, ClientOptions, Signer } from "@xmtp/react-native-sdk";
import type { ReactNode } from "react";

export interface InitializeClientOptions {
  readonly signer: Signer;
  readonly options: ClientOptions;
}

export interface XmtpContextValue {
  readonly client: Client | null;
  readonly error: Error | null;
  readonly isLoading: boolean;
  initialize(this: void, options: InitializeClientOptions): Promise<Client>;
  disconnect(this: void): Promise<void>;
}

export interface XmtpProviderProps {
  readonly children: ReactNode;
  readonly client?: Client | null;
  readonly onError?: (error: Error) => void;
}
