import { Group, NativeSelect, Stack, Text } from "@mantine/core";
import { type LogLevel } from "@xmtp/browser-sdk";

import { useSettings } from "@/hooks/useSettings";

const loggingLevelStringToEnum: Record<string, LogLevel> = {
  Off: "off",
  Error: "error",
  Warn: "warn",
  Info: "info",
  Debug: "debug",
  Trace: "trace",
};

const loggingLevelEnumToString = {
  ["off"]: "Off",
  ["error"]: "Error",
  ["warn"]: "Warn",
  ["info"]: "Info",
  ["debug"]: "Debug",
  ["trace"]: "Trace",
};

export const LoggingSelect: React.FC = () => {
  const { loggingLevel, setLoggingLevel } = useSettings();

  const handleChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    setLoggingLevel(
      loggingLevelStringToEnum[
        event.currentTarget.value as keyof typeof loggingLevelStringToEnum
      ],
    );
  };

  return (
    <Stack gap="xs">
      <Group gap="xs" justify="space-between">
        <Text fw="bold" size="lg">
          Logging level
        </Text>
        <NativeSelect
          data={Object.keys(loggingLevelStringToEnum)}
          value={loggingLevelEnumToString[loggingLevel]}
          onChange={handleChange}
        />
      </Group>
      <Text size="sm">Enable logging to help debug issues</Text>
    </Stack>
  );
};
