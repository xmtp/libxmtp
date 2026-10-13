import { Text, type TextProps } from "@mantine/core";

import { splitHandle } from "@/helpers/handles";

export type HandleTextProps = TextProps & {
  handle: string;
  withPrefix?: boolean;
};

/** A handle, with its code dimmed so that the name part stands out. */
export const HandleText: React.FC<HandleTextProps> = ({
  handle,
  withPrefix = false,
  ...textProps
}) => {
  const { name, code } = splitHandle(
    withPrefix ? handle.replace(/^@+/, "") : handle,
  );
  return (
    <Text span {...textProps}>
      {withPrefix && "@"}
      {name}
      {code !== undefined && (
        <Text span inherit c="dimmed">
          #{code}
        </Text>
      )}
    </Text>
  );
};
