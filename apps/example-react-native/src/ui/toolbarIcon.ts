import { unstable_getMaterialSymbolSourceAsync } from "expo-symbols";
import { useEffect, useState } from "react";
import { type ImageSourcePropType, Platform } from "react-native";

import { symbolNames, type SymbolName } from "@/ui/Symbol";

/** The icon for `Stack.Toolbar.Button`: SF Symbol name or image source. */
export type ToolbarIcon = SymbolName | ImageSourcePropType;

/**
 * A `Stack.Toolbar.Button` icon. iOS takes the SF Symbol name; Android needs
 * an image, so this renders the matching Material Symbol (undefined until it
 * is ready).
 */
export function useToolbarIcon(
  name: SymbolName,
  color = "#000000",
): ToolbarIcon | undefined {
  const [source, setSource] = useState<ImageSourcePropType | undefined>(
    undefined,
  );
  useEffect(() => {
    if (Platform.OS !== "android") return;
    let current = true;
    void unstable_getMaterialSymbolSourceAsync(
      symbolNames[name],
      24,
      color,
    ).then(
      (image) => {
        if (current && image !== null) setSource(image);
      },
      () => {},
    );
    return () => {
      current = false;
    };
  }, [name, color]);
  return Platform.OS === "android" ? source : name;
}
