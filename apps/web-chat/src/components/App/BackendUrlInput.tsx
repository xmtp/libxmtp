import { Stack, Text, TextInput } from "@mantine/core";
import { useState } from "react";
import { useXMTP } from "@/contexts/XMTPContext";
import { isValidBackendUrl } from "@/helpers/backend";
import { useSettings } from "@/hooks/useSettings";

export const BackendUrlInput: React.FC = () => {
  const { lockState } = useXMTP();
  const { backendUrl, setBackendUrl } = useSettings();
  const [value, setValue] = useState(backendUrl);
  const [previousBackendUrl, setPreviousBackendUrl] = useState(backendUrl);
  const valid = isValidBackendUrl(value);

  if (backendUrl !== previousBackendUrl) {
    setPreviousBackendUrl(backendUrl);
    setValue(backendUrl);
  }

  return (
    <Stack gap="xs">
      <Text fw="bold" size="lg">
        Backend URL
      </Text>
      <TextInput
        aria-label="XMTP backend URL"
        value={value}
        // The deployed app ships with no default backend, so an empty field is
        // the first thing a new user sees. Explain it rather than leaving the
        // disabled Connect button unexplained.
        error={valid ? null : "Enter a valid http:// or https:// URL"}
        disabled={lockState !== "available"}
        placeholder="http://127.0.0.1:5050"
        onChange={(event) => {
          setValue(event.currentTarget.value);
        }}
        onBlur={() => {
          if (valid) setBackendUrl(value);
        }}
      />
      <Text size="sm">Enter the XMTP backend you want to connect to</Text>
    </Stack>
  );
};
