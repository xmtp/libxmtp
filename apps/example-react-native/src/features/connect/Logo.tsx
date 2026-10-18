import Svg, { Path } from "react-native-svg";

import { useColors } from "@/ui/colors";

/** The XMTP mark (`Assets.xcassets/Logo.imageset/Logo.svg`) in `accentText`. */
export function Logo({ size }: { readonly size: number }) {
  const palette = useColors();
  return (
    <Svg
      width={size}
      height={size}
      viewBox="0 0 248 248"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants">
      <Path
        fill={palette.accentText}
        d="M0 124C0 55.5167 55.5153 0 123.997 0C192.434 0 245.31 54.4848 246.92 123.463C246.92 145.472 239.405 163.991 220.618 181.437C204.742 196.18 177.675 198.346 157.816 187.342C143.641 179.165 132.717 161.751 123.46 148.961L106.283 175.264H69.2451L104.136 123.461L70.3186 72.4675H108.43L123.729 98.7706L138.759 72.4675H177.139L142.248 123.463C142.248 123.463 158.888 148.961 168.013 158.892C177.139 168.823 193.779 169.091 204.514 158.355C216.318 146.551 218.96 137.152 219.008 123.463C219.192 70.4783 177.107 27.9134 123.997 27.9134C70.9311 27.9134 27.9127 70.9328 27.9127 124C27.9127 177.067 70.9311 220.087 123.997 220.087C131.337 220.087 138.373 219.47 145.2 217.939L151.105 245.048C141.39 247.226 133.54 248 123.997 248C55.5153 248 0 192.483 0 124Z"
      />
    </Svg>
  );
}
