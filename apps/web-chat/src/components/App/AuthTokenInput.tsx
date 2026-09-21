import { PasswordInput, Stack, Text } from "@mantine/core";
import { useEffect, useState } from "react";
import { useXMTP } from "@/contexts/XMTPContext";
import { useSettings } from "@/hooks/useSettings";

export const AuthTokenInput: React.FC = () => {
  const { lockState } = useXMTP();
  const { authToken, setAuthToken } = useSettings();
  const [value, setValue] = useState(authToken);

  useEffect(() => {
    setValue(authToken);
  }, [authToken]);

  return (
    <Stack gap="xs">
      <Text fw="bold" size="lg">
        Auth token
      </Text>
      <PasswordInput
        aria-label="Backend auth token"
        value={value}
        disabled={lockState !== "available"}
        placeholder="Only for backends that require one"
        onChange={(event) => {
          setValue(event.currentTarget.value);
        }}
        onBlur={() => {
          setAuthToken(value.trim());
        }}
      />
      <Text size="sm">
        Leave empty unless the backend requires a credential. You are prompted
        if it does.
      </Text>
    </Stack>
  );
};
