import { unstable_getMaterialSymbolSourceAsync } from "expo-symbols";
import { useEffect, useState } from "react";
import { type ImageSourcePropType, Platform } from "react-native";

import { symbolNames, type SymbolName } from "@/ui/Symbol";

/** The icon for `Stack.Toolbar.Button`: SF Symbol name or image source. */
export type ToolbarIcon = SymbolName | ImageSourcePropType;

/** A transparent 1 x 1 PNG, shown on Android until the Material icon renders. */
const blankIcon: ImageSourcePropType = {
  uri: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
};

/**
 * A `Stack.Toolbar.Button` icon. iOS takes the SF Symbol name; Android needs
 * an image, so this renders the matching Material Symbol (blank until ready).
 */
export function useToolbarIcon(
  name: SymbolName,
  color = "#000000",
): ToolbarIcon {
  const [source, setSource] = useState<ImageSourcePropType>(blankIcon);
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
