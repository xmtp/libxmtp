import { useLocalStorage } from "@mantine/hooks";
import { type LogLevel } from "@xmtp/browser-sdk";
import { useEffect } from "react";
import type { Hex } from "viem";

import type { ConnectorString } from "@/hooks/useWallet";

const legacyLoggingLevel = (value: string) => {
  switch (value) {
    case "error":
      return "error";
    case "warn":
      return "warn";
    case "info":
      return "info";
    case "debug":
      return "debug";
    case "trace":
      return "trace";
    default:
      return "off";
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
  const [authToken, setAuthToken] = useLocalStorage({
    key: "XMTP_AUTH_TOKEN",
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
  const [loggingLevel, setLoggingLevel] = useLocalStorage<LogLevel>({
    key: "XMTP_LOGGING_LEVEL",
    defaultValue: "warn",
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
  }, [loggingLevel, setLoggingLevel]);

  return {
    authToken,
    autoConnect,
    backendUrl,
    blockchain,
    connector,
    ephemeralAccountEnabled,
    ephemeralAccountKey,
    forceSCW,
    loggingLevel,
    useSCW,
    showDisclaimer,
    setAuthToken,
    setAutoConnect,
    setBlockchain,
    setConnector,
    setBackendUrl,
    setEphemeralAccountEnabled,
    setEphemeralAccountKey,
    setForceSCW,
    setLoggingLevel,
    setUseSCW,
    setShowDisclaimer,
  };
};
