import { useLocalStorage } from "@mantine/hooks";
import { LogLevel, type ClientOptions } from "@xmtp/browser-sdk";
import { useEffect } from "react";
import type { Hex } from "viem";
import type { ConnectorString } from "@/hooks/useWallet";

const legacyLoggingLevel = (value: string) => {
  switch (value) {
    case "error":
      return LogLevel.Error;
    case "warn":
      return LogLevel.Warn;
    case "info":
      return LogLevel.Info;
    case "debug":
      return LogLevel.Debug;
    case "trace":
      return LogLevel.Trace;
    default:
      return LogLevel.Off;
  }
};

export const useSettings = () => {
  const [backendUrl, setBackendUrl] = useLocalStorage({
    key: "XMTP_BACKEND_URL",
    defaultValue: import.meta.env.XMTP_BACKEND_URL,
    getInitialValueInEffect: false,
  });
  const [ephemeralAccountKey, setEphemeralAccountKey] =
    useLocalStorage<Hex | null>({
      key: "XMTP_EPHEMERAL_ACCOUNT_KEY",
      defaultValue: null,
      getInitialValueInEffect: false,
    });
  const [encryptionKey, setEncryptionKey] = useLocalStorage({
    key: "XMTP_ENCRYPTION_KEY",
    defaultValue: "",
    getInitialValueInEffect: false,
  });
  const [ephemeralAccountEnabled, setEphemeralAccountEnabled] = useLocalStorage(
    {
      key: "XMTP_USE_EPHEMERAL_ACCOUNT",
      defaultValue: false,
      getInitialValueInEffect: false,
    },
  );
  const [loggingLevel, setLoggingLevel] = useLocalStorage<
    ClientOptions["loggingLevel"]
  >({
    key: "XMTP_LOGGING_LEVEL",
    defaultValue: LogLevel.Warn,
    getInitialValueInEffect: false,
  });
  const [forceSCW, setForceSCW] = useLocalStorage<boolean>({
    key: "XMTP_FORCE_SCW",
    defaultValue: false,
    getInitialValueInEffect: false,
  });
  const [useSCW, setUseSCW] = useLocalStorage<boolean>({
    key: "XMTP_USE_SCW",
    defaultValue: false,
    getInitialValueInEffect: false,
  });
  const [blockchain, setBlockchain] = useLocalStorage<number>({
    key: "XMTP_BLOCKCHAIN",
    defaultValue: 1,
    getInitialValueInEffect: false,
  });
  const [connector, setConnector] = useLocalStorage<ConnectorString>({
    key: "XMTP_CONNECTOR",
    defaultValue: "Injected",
    getInitialValueInEffect: false,
  });
  const [autoConnect, setAutoConnect] = useLocalStorage<boolean>({
    key: "XMTP_AUTO_CONNECT",
    defaultValue: false,
    getInitialValueInEffect: false,
  });
  const [showDisclaimer, setShowDisclaimer] = useLocalStorage<boolean>({
    key: "XMTP_SHOW_DISCLAIMER",
    defaultValue: true,
    getInitialValueInEffect: false,
  });
  // fix for old logging level values
  useEffect(() => {
    if (typeof loggingLevel === "string") {
      setLoggingLevel(legacyLoggingLevel(loggingLevel));
    }
  }, [loggingLevel]);

  return {
    autoConnect,
    backendUrl,
    blockchain,
    connector,
    encryptionKey,
    ephemeralAccountEnabled,
    ephemeralAccountKey,
    forceSCW,
    loggingLevel,
    useSCW,
    showDisclaimer,
    setAutoConnect,
    setBlockchain,
    setConnector,
    setEncryptionKey,
    setBackendUrl,
    setEphemeralAccountEnabled,
    setEphemeralAccountKey,
    setForceSCW,
    setLoggingLevel,
    setUseSCW,
    setShowDisclaimer,
  };
};
