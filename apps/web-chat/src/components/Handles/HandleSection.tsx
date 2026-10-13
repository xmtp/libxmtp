import { Alert, Button, Group, Loader, Stack, Text } from "@mantine/core";
import { useCallback, useState } from "react";

import { ConfirmHandleChange } from "@/components/Handles/ConfirmHandleChange";
import { HandleEditor } from "@/components/Handles/HandleEditor";
import { HandleText } from "@/components/Handles/HandleText";
import { useClient } from "@/contexts/XMTPContext";
import {
  claimFailure,
  splitHandle,
  type ClaimOutcome,
} from "@/helpers/handles";
import { useOwnHandles } from "@/hooks/useOwnHandles";

type Blocked = "tooSoon" | "limitReached";

type SwitchBack = {
  handle: string;
  state: "confirm" | "claiming" | "notAllowed" | "failed";
};

const BLOCKED_MESSAGES: Record<Blocked, string> = {
  tooSoon: "You changed your handle less than 7 days ago. Try again later.",
  limitReached:
    "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.",
};

const STILL_YOURS =
  "They are still yours, and people can still find you by them.";

/**
 * The Handle section of the identity modal: the current and other handles,
 * and the editor that chooses or changes the handle.
 */
export const HandleSection: React.FC = () => {
  const client = useClient();
  const { handle, otherHandles, loaded, refresh } = useOwnHandles();
  const [editing, setEditing] = useState(false);
  const [blocked, setBlocked] = useState<Blocked | null>(null);
  const [switchBack, setSwitchBack] = useState<SwitchBack | null>(null);
  const minNameLength = client.serverConfiguration.handles.minNameLength;

  // The backend decides every claim; the app shows its answer.
  const claim = useCallback(
    async (next: string): Promise<ClaimOutcome> => {
      let outcome: ClaimOutcome;
      try {
        await client.claimHandle(next);
        outcome = "claimed";
      } catch (error) {
        outcome = claimFailure(error);
      }
      if (outcome === "tooSoon" || outcome === "limitReached") {
        setBlocked(outcome);
      } else if (outcome === "claimed") {
        setEditing(false);
        setSwitchBack(null);
        await refresh().catch(console.error);
      }
      return outcome;
    },
    [client, refresh],
  );

  const handleSwitchBack = useCallback(
    async (previous: string) => {
      setSwitchBack({ handle: previous, state: "claiming" });
      const outcome = await claim(previous);
      switch (outcome) {
        case "notAllowed":
        case "failed":
        case "taken":
          setSwitchBack({
            handle: previous,
            state: outcome === "notAllowed" ? "notAllowed" : "failed",
          });
          break;
        default:
          setSwitchBack(null);
      }
    },
    [claim],
  );

  if (!loaded) {
    return (
      <Group justify="center" p="md">
        <Loader size="sm" />
      </Group>
    );
  }

  if (handle === null) {
    return (
      <Stack gap="md">
        <Text fw={700}>Choose a handle</Text>
        <Text size="sm" c="dimmed">
          Your handle is public. People on this server use it to find you and
          send you messages.
        </Text>
        <HandleEditor
          currentHandle={null}
          otherHandles={otherHandles}
          claim={claim}
        />
        <Text size="xs" c="dimmed">
          You can change your handle once every 7 days.
        </Text>
      </Stack>
    );
  }

  const blockedAlert = blocked !== null && (
    <Alert color="red" variant="light">
      {BLOCKED_MESSAGES[blocked]}
    </Alert>
  );

  if (!editing) {
    return (
      <Stack gap="sm">
        <Stack gap={0}>
          <HandleText handle={handle} size="xl" fw={700} />
          <Text size="sm" c="dimmed">
            Your handle on this server
          </Text>
        </Stack>
        {otherHandles.length > 0 && (
          <Stack gap={0}>
            <Text size="sm" fw={700}>
              Other handles
            </Text>
            <Text size="sm">{otherHandles.join(" · ")}</Text>
            <Text size="xs" c="dimmed">
              {STILL_YOURS}
            </Text>
          </Stack>
        )}
        {blockedAlert}
        {blocked !== "limitReached" && (
          <Group>
            <Button
              variant="default"
              onClick={() => {
                setEditing(true);
              }}>
              Change handle
            </Button>
          </Group>
        )}
      </Stack>
    );
  }

  const switchBackMessage = (() => {
    if (switchBack?.state === "notAllowed") {
      return splitHandle(switchBack.handle).name.length < minNameLength
        ? `${switchBack.handle} is shorter than this server now allows. It still finds you, but you cannot switch back to it.`
        : `${switchBack.handle} is not allowed on this server now. It still finds you, but you cannot switch back to it.`;
    }
    if (switchBack?.state === "failed") {
      return `Cannot switch back to ${switchBack.handle}. Try again.`;
    }
    return null;
  })();
  const busy = switchBack?.state === "claiming";

  return (
    <Stack gap="md">
      <Stack gap={0}>
        <Text size="sm" fw={700}>
          Current handle
        </Text>
        <HandleText handle={handle} size="lg" fw={700} />
      </Stack>
      {blockedAlert}
      {blocked !== "limitReached" && (
        <Stack gap="xs">
          <Text size="sm" fw={700}>
            New handle
          </Text>
          <HandleEditor
            currentHandle={handle}
            otherHandles={otherHandles}
            disabled={blocked !== null || busy}
            claim={claim}
          />
        </Stack>
      )}
      {otherHandles.length > 0 && (
        <Stack gap="xs">
          <Text size="sm" fw={700}>
            Your other handles
          </Text>
          <Text size="xs" c="dimmed">
            {STILL_YOURS} Switching back counts as a change.
          </Text>
          {otherHandles.map((previous) => (
            <Group key={previous} justify="space-between" wrap="nowrap">
              <HandleText handle={previous} size="sm" />
              {blocked !== "limitReached" && (
                <Button
                  size="xs"
                  variant="default"
                  aria-label={`Use ${previous}`}
                  loading={
                    switchBack?.state === "claiming" &&
                    switchBack.handle === previous
                  }
                  disabled={blocked !== null || busy}
                  onClick={() => {
                    setSwitchBack({ handle: previous, state: "confirm" });
                  }}>
                  Use
                </Button>
              )}
            </Group>
          ))}
          {switchBackMessage !== null && (
            <Alert color="red" variant="light">
              {switchBackMessage}
            </Alert>
          )}
        </Stack>
      )}
      <Text size="xs" c="dimmed">
        You can change your handle once every 7 days, up to 64 times in total.
      </Text>
      <Group justify="flex-end">
        <Button
          variant="default"
          disabled={busy}
          onClick={() => {
            setEditing(false);
            setSwitchBack(null);
          }}>
          Cancel
        </Button>
      </Group>
      <ConfirmHandleChange
        current={handle}
        next={switchBack?.state === "confirm" ? switchBack.handle : null}
        onCancel={() => {
          setSwitchBack(null);
        }}
        onConfirm={(previous) => {
          void handleSwitchBack(previous);
        }}
      />
    </Stack>
  );
};
