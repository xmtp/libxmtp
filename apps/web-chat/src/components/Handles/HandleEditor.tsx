import {
  Alert,
  Button,
  Group,
  Loader,
  Radio,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useRef, useState } from "react";

import { ConfirmHandleChange } from "@/components/Handles/ConfirmHandleChange";
import { HandleText } from "@/components/Handles/HandleText";
import { useClient } from "@/contexts/XMTPContext";
import {
  checkHandleInput,
  HANDLE_SUGGESTION_PAGE,
  handleHint,
  MAX_HANDLE_NAME_LENGTH,
  splitHandle,
  type ClaimOutcome,
} from "@/helpers/handles";

type Phase =
  // the user types; the field shows the hint or an as-you-type error
  | { kind: "idle" }
  | { kind: "checking"; query: string }
  | { kind: "checkFailed"; query: string }
  // the typed handle is free
  | { kind: "available"; handle: string; others: string[] }
  // the typed handle is one of the user's other handles: a switch back
  | { kind: "own"; handle: string }
  | {
      kind: "suggestions";
      taken: string;
      candidates: string[];
      visible: number;
      selected: string;
      notice: string | null;
    }
  | { kind: "noFreeCodes"; query: string; notice: string | null }
  | { kind: "notAllowed"; handle: string };

export type HandleEditorProps = {
  currentHandle: string | null;
  otherHandles: string[];
  disabled?: boolean;
  /**
   * Claims a handle that the user chose, and gives the backend's answer. The
   * editor never claims a handle the user did not pick (PD9).
   */
  claim: (handle: string) => Promise<ClaimOutcome>;
};

/**
 * Finds a free handle for the name the user types, and claims the one the
 * user picks. The flow and copy follow the approved Android designs.
 */
export const HandleEditor: React.FC<HandleEditorProps> = ({
  currentHandle,
  otherHandles,
  disabled = false,
  claim,
}) => {
  const client = useClient();
  const minNameLength = client.serverConfiguration.handles.minNameLength;
  const [input, setInput] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [claiming, setClaiming] = useState<string | null>(null);
  const [claimError, setClaimError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  // an answer for older input must not replace the state for newer input
  const generation = useRef(0);

  const inputError = checkHandleInput(input, minNameLength);
  const isCurrent = input !== "" && input === currentHandle;
  const nameLength = splitHandle(input).name.length;
  const busy = disabled || claiming !== null;

  const showChoices = (
    taken: string,
    candidates: string[],
    selectedIndex: number,
    notice: string | null,
  ) => {
    if (candidates.length === 0) {
      setPhase({ kind: "noFreeCodes", query: taken, notice });
      return;
    }
    const selected = Math.min(selectedIndex, candidates.length - 1);
    setPhase({
      kind: "suggestions",
      taken,
      candidates,
      visible: Math.max(HANDLE_SUGGESTION_PAGE, selected + 1),
      selected: candidates[selected],
      notice,
    });
  };

  const check = async (query: string) => {
    const request = ++generation.current;
    setClaimError(null);
    if (otherHandles.includes(query)) {
      setPhase({ kind: "own", handle: query });
      return;
    }
    setPhase({ kind: "checking", query });
    try {
      const free = await client.availableHandles(query);
      if (request !== generation.current) return;
      if (free.includes(query)) {
        setPhase({
          kind: "available",
          handle: query,
          others: free.filter((handle) => handle !== query),
        });
        return;
      }
      // a taken handle with a code: offer free handles for its name part
      const { name, code } = splitHandle(query);
      const candidates =
        code === undefined ? free : await client.availableHandles(name);
      if (request !== generation.current) return;
      showChoices(query, candidates, 0, null);
    } catch {
      if (request === generation.current) {
        setPhase({ kind: "checkFailed", query });
      }
    }
  };

  // Someone else claimed the pick first. Show the other free handles, with
  // the next one selected, and wait for the user to pick again.
  const handleTaken = async (handle: string) => {
    const request = ++generation.current;
    const notice = `Someone claimed ${handle} a moment before you. Pick another handle.`;
    let taken = handle;
    let candidates: string[] = [];
    let index = 0;
    if (phase.kind === "available") {
      candidates = phase.others;
    } else if (phase.kind === "suggestions") {
      taken = phase.taken;
      index = Math.max(0, phase.candidates.indexOf(handle));
      candidates = phase.candidates.filter((other) => other !== handle);
    }
    if (candidates.length === 0) {
      setPhase({ kind: "checking", query: splitHandle(handle).name });
      try {
        const free = await client.availableHandles(splitHandle(handle).name);
        if (request !== generation.current) return;
        candidates = free.filter((other) => other !== handle);
      } catch {
        if (request === generation.current) {
          setPhase({ kind: "checkFailed", query: input });
        }
        return;
      }
    }
    showChoices(taken, candidates, index, notice);
  };

  const submitClaim = async (handle: string) => {
    setClaiming(handle);
    setClaimError(null);
    const outcome = await claim(handle);
    setClaiming(null);
    switch (outcome) {
      case "claimed":
        generation.current += 1;
        setInput("");
        setPhase({ kind: "idle" });
        break;
      case "taken":
        await handleTaken(handle);
        break;
      case "notAllowed":
        generation.current += 1;
        setPhase({ kind: "notAllowed", handle });
        break;
      case "failed":
        setClaimError(`Cannot claim ${handle}. Try again.`);
        break;
      case "tooSoon":
      case "limitReached":
        // the handle section shows these, and disables the editor
        break;
    }
  };

  const requestClaim = (handle: string) => {
    if (currentHandle) {
      setConfirming(handle);
    } else {
      void submitClaim(handle);
    }
  };

  const pick = (() => {
    switch (phase.kind) {
      case "available":
      case "own":
        return phase.handle;
      case "suggestions":
        return phase.selected;
      default:
        return null;
    }
  })();

  const canContinue =
    input !== "" &&
    inputError === null &&
    !isCurrent &&
    (phase.kind === "idle" || phase.kind === "checkFailed");

  const primary = (() => {
    if (claiming !== null) {
      return currentHandle
        ? `Changing to ${claiming}…`
        : `Claiming ${claiming}…`;
    }
    if (pick !== null) {
      return currentHandle ? `Change to ${pick}` : `Claim ${pick}`;
    }
    return "Continue";
  })();

  const fieldError =
    inputError ??
    (isCurrent
      ? "This is your current handle."
      : phase.kind === "notAllowed"
        ? "Choose a different name."
        : null);

  return (
    <Stack gap="sm">
      <TextInput
        label="Handle"
        placeholder="nick"
        autoComplete="off"
        value={input}
        disabled={busy}
        description={fieldError === null ? handleHint(minNameLength) : null}
        error={fieldError}
        inputWrapperOrder={["label", "input", "description", "error"]}
        rightSectionWidth={56}
        rightSection={
          nameLength > MAX_HANDLE_NAME_LENGTH ? (
            <Text size="xs" c="red">
              {nameLength}/{MAX_HANDLE_NAME_LENGTH}
            </Text>
          ) : null
        }
        onChange={(event) => {
          generation.current += 1;
          setInput(event.currentTarget.value.toLowerCase());
          setPhase({ kind: "idle" });
          setClaimError(null);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && canContinue && !busy) {
            void check(input);
          }
        }}
      />
      {phase.kind === "checking" && (
        <Group gap="xs" wrap="nowrap">
          <Loader size="xs" />
          <Text size="sm">Checking {phase.query}…</Text>
        </Group>
      )}
      {phase.kind === "available" && (
        <Text size="sm" c="teal">
          {phase.handle} is available.
        </Text>
      )}
      {phase.kind === "own" && (
        <Text size="sm">
          {phase.handle} is one of your handles. Switching back counts as a
          change.
        </Text>
      )}
      {phase.kind === "suggestions" && (
        <Stack gap="xs">
          <Text size="sm">{phase.taken} is taken. These handles are free:</Text>
          <Radio.Group
            aria-label="Free handles"
            value={phase.selected}
            onChange={(selected) => {
              setPhase({ ...phase, selected });
            }}>
            <Stack gap="xs">
              {phase.candidates.slice(0, phase.visible).map((handle) => (
                <Radio
                  key={handle}
                  value={handle}
                  disabled={busy}
                  label={<HandleText handle={handle} size="sm" />}
                />
              ))}
            </Stack>
          </Radio.Group>
          {phase.visible < phase.candidates.length && (
            <Group>
              <Button
                variant="subtle"
                size="xs"
                disabled={busy}
                onClick={() => {
                  setPhase({
                    ...phase,
                    visible: phase.visible + HANDLE_SUGGESTION_PAGE,
                  });
                }}>
                Show more
              </Button>
            </Group>
          )}
        </Stack>
      )}
      {phase.kind === "noFreeCodes" && (
        <Stack gap="xs">
          <Text size="sm">
            {splitHandle(phase.query).name} and the codes we checked are all
            taken. Look again, or try a different name.
          </Text>
          <Group>
            <Button
              variant="subtle"
              size="xs"
              disabled={busy}
              onClick={() => void check(phase.query)}>
              Look again
            </Button>
          </Group>
        </Stack>
      )}
      {(phase.kind === "suggestions" || phase.kind === "noFreeCodes") &&
        phase.notice !== null && (
          <Alert color="red" variant="light">
            {phase.notice}
          </Alert>
        )}
      {phase.kind === "checkFailed" && (
        <Alert color="red" variant="light">
          <Stack gap="xs" align="flex-start">
            <Text size="sm">Cannot check this handle. Try again.</Text>
            <Button
              variant="subtle"
              color="red"
              size="xs"
              onClick={() => void check(phase.query)}>
              Retry
            </Button>
          </Stack>
        </Alert>
      )}
      {phase.kind === "notAllowed" && (
        <Alert color="red" variant="light">
          {phase.handle} is not allowed on this server.
        </Alert>
      )}
      {claimError !== null && (
        <Alert color="red" variant="light">
          {claimError}
        </Alert>
      )}
      <Group justify="flex-end">
        <Button
          loading={claiming !== null}
          disabled={busy || (pick === null && !canContinue)}
          onClick={() => {
            if (pick !== null) {
              requestClaim(pick);
            } else {
              void check(input);
            }
          }}>
          {primary}
        </Button>
      </Group>
      {currentHandle !== null && (
        <ConfirmHandleChange
          current={currentHandle}
          next={confirming}
          onCancel={() => {
            setConfirming(null);
          }}
          onConfirm={(next) => {
            setConfirming(null);
            void submitClaim(next);
          }}
        />
      )}
    </Stack>
  );
};
