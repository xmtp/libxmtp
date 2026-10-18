import { Stack } from "expo-router";
import {
  DarkTheme,
  DefaultTheme,
  ThemeProvider,
} from "expo-router/react-navigation";
import { useColorScheme } from "react-native";

export default function RootLayout() {
  // The native header picks its light or dark style from the navigation theme
  // (expo/expo#50357); without a ThemeProvider it stays light in dark mode.
  const scheme = useColorScheme();
  return (
    <ThemeProvider value={scheme === "dark" ? DarkTheme : DefaultTheme}>
      <Stack>
        <Stack.Screen name="index" options={{ headerShown: false }} />
        <Stack.Screen
          name="chats"
          options={{ headerLargeTitleEnabled: true }}
        />
      </Stack>
    </ThemeProvider>
  );
}
