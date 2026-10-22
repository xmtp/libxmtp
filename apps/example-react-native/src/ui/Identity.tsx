import { splitHandle, stripAt } from "@/core/titles.ts";
import { Span, Text, type TextProps } from "@/ui/Text";

type TextLikeProps = Omit<TextProps, "children" | "accessibilityLabel">;

export interface HandleTextProps extends TextLikeProps {
  handle: string;
}

/**
 * A handle: the name in the primary color and the `#code` suffix in the
 * secondary color (`HandleText`). The spoken label is the handle without `@`.
 */
export function HandleText({ handle, ...props }: HandleTextProps) {
  const { name, code } = splitHandle(handle);
  return (
    <Text {...props} accessibilityLabel={stripAt(handle)}>
      <Span color="label">{name}</Span>
      {code === "" ? null : <Span color="secondaryLabel">{code}</Span>}
    </Text>
  );
}

export interface IdentityTextProps extends TextLikeProps {
  inboxId: string;
  handle?: string | null;
  /** 8 characters of the inbox ID instead of 12. */
  short?: boolean;
}

/**
 * A handle, or the monospaced inbox ID prefix when there is none
 * (`IdentityText`). The prefix's spoken label is the full inbox ID.
 */
export function IdentityText({
  inboxId,
  handle,
  short = false,
  ...props
}: IdentityTextProps) {
  if (handle !== undefined && handle !== null && handle !== "") {
    return <HandleText handle={handle} {...props} />;
  }
  return (
    <Text {...props} monospaced accessibilityLabel={inboxId}>
      {inboxId.slice(0, short ? 8 : 12)}
    </Text>
  );
}

export interface ConversationTitleProps extends TextLikeProps {
  title: string;
}

/**
 * A conversation title. Titles that start with `@` draw the `@` and a handle
 * (`ConversationTitle`); other titles are plain text.
 */
export function ConversationTitle({ title, ...props }: ConversationTitleProps) {
  if (!title.startsWith("@")) return <Text {...props}>{title}</Text>;
  const { name, code } = splitHandle(title);
  return (
    <Text {...props} accessibilityLabel={title}>
      <Span color="label">@</Span>
      <Span color="label">{name}</Span>
      {code === "" ? null : <Span color="secondaryLabel">{code}</Span>}
    </Text>
  );
}
