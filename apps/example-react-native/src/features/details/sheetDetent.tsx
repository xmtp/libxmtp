import { createContext, type ReactNode, use, useState } from "react";
import { StyleSheet, useWindowDimensions, View } from "react-native";

/** True while the sheet rests below its largest detent. */
const PartialDetentContext = createContext(false);

/** A sheet shorter than this share of the window is at a partial detent. */
const largeDetentShare = 0.85;

export interface SheetDetentProviderProps {
  /** True when the sheet opens at a partial detent. */
  readonly initiallyPartial: boolean;
  readonly children?: ReactNode;
}

/**
 * Tracks whether the sheet whose layout renders it sits at a partial detent,
 * from its height (the keyboard raises a sheet to its large detent without a
 * detent event). iOS 26 draws a sheet at a partial detent as glass with
 * translucent list cells, and as an opaque grouped list at its large detent.
 */
export function SheetDetentProvider({
  initiallyPartial,
  children,
}: SheetDetentProviderProps) {
  const window = useWindowDimensions();
  const [partial, setPartial] = useState(initiallyPartial);
  return (
    <View
      style={styles.fill}
      onLayout={(event) => {
        setPartial(
          event.nativeEvent.layout.height < window.height * largeDetentShare,
        );
      }}>
      <PartialDetentContext value={partial}>{children}</PartialDetentContext>
    </View>
  );
}

/** `FormScreen` props for a sheet screen at its current detent. */
export function useSheetFormProps(): {
  readonly cardColor?: "tertiarySystemFill";
  readonly style?: { readonly backgroundColor: "transparent" };
} {
  const partial = use(PartialDetentContext);
  return partial
    ? {
        cardColor: "tertiarySystemFill",
        style: { backgroundColor: "transparent" },
      }
    : {};
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
});
