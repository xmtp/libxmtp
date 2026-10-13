import { Text, type TextProps } from "@mantine/core";

import { splitHandle } from "@/helpers/handles";

export type HandleTextProps = TextProps & {
  handle: string;
};

/** A handle, with its code dimmed so that the name part stands out. */
export const HandleText: React.FC<HandleTextProps> = ({
  handle,
  ...textProps
}) => {
  const { name, code } = splitHandle(handle);
  return (
    <Text span {...textProps}>
      {name}
      {code !== undefined && (
        <Text span inherit c="dimmed">
          #{code}
        </Text>
      )}
    </Text>
  );
};
