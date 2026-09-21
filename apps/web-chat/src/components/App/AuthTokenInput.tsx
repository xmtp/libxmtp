import { PasswordInput, Stack, Text } from "@mantine/core";
import { useState } from "react";
import { useXMTP } from "@/contexts/XMTPContext";
import { useSettings } from "@/hooks/useSettings";
import { useServerAuthConfig } from "@/hooks/useServerAuthConfig";

export const AuthTokenInput: React.FC = () => {
  const { lockState } = useXMTP();
  const { authToken, backendUrl, setAuthToken } = useSettings();
  const { required, requiredScopes, loading } = useServerAuthConfig(backendUrl);
  const [value, setValue] = useState(authToken);
  const [previousAuthToken, setPreviousAuthToken] = useState(authToken);

  if (authToken !== previousAuthToken) {
    setPreviousAuthToken(authToken);
    setValue(authToken);
  }

  // The backend says it needs no credential. Keep the field when a token is
  // already stored, so a user can see and clear one they entered earlier.
  if (!required && authToken === "") {
    return null;
  }

  return (
    <Stack gap="xs">
      <Text fw="bold" size="lg">
        Auth token
      </Text>
      <PasswordInput
        aria-label="Backend auth token"
        value={value}
        disabled={lockState !== "available"}
        placeholder={loading ? "Checking the backend…" : "Paste the token"}
        onChange={(event) => {
          setValue(event.currentTarget.value);
        }}
        onBlur={() => {
          setAuthToken(value.trim());
        }}
      />
      <Text size="sm">
        {!required
          ? "This backend does not require a token. Clear the field to stop sending one."
          : requiredScopes.length > 0
            ? `This backend requires a token with: ${requiredScopes.join(", ")}`
            : "This backend requires a token. You are prompted if it is missing or rejected."}
      </Text>
    </Stack>
  );
};
