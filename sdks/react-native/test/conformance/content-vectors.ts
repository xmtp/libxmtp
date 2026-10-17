import {
  decodeEncodedContent,
  decodeStandard,
  encodeEncodedContent,
  encodeStandard,
  type StandardContent,
} from "@xmtp/react-native-sdk";

import {
  standardCodecs,
  standardSamples,
  variantSamples,
} from "../../../node/test/standardSamples";
import { assert, bytesEqual, equal } from "./assertions";

// Preserve types as well as values. JSON alone would silently accept bigint
// strings or ordinary arrays where the native projection promises typed values.
function comparable(value: unknown): string {
  const normalize = (item: unknown): unknown => {
    if (typeof item === "bigint") return { bigint: item.toString() };
    if (item instanceof Uint8Array) return { bytes: Array.from(item) };
    if (item instanceof Map)
      return {
        map: Array.from(item)
          .sort(([a], [b]) => String(a).localeCompare(String(b)))
          .map(([key, value]) => [key, normalize(value)]),
      };
    if (Array.isArray(item)) return item.map(normalize);
    if (item && typeof item === "object")
      return Object.fromEntries(
        Object.entries(item)
          .filter(([, value]) => value !== undefined)
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([key, value]) => [key, normalize(value)]),
      );
    return item;
  };
  return JSON.stringify(normalize(value));
}

function valueOf(content: StandardContent) {
  return content.kind === "readReceipt"
    ? undefined
    : content.kind === "reaction" ||
        content.kind === "reply" ||
        content.kind === "deleteMessage"
      ? content
      : content.value;
}

/** The sibling catalogue runs unchanged through the installed native codecs. */
export function contentVectorJourney(): string[] {
  for (const content of [...standardSamples, ...variantSamples]) {
    const encoded = encodeStandard(content);
    const codec = standardCodecs.find(
      (candidate) => candidate.type.typeId === encoded.type.typeId,
    );
    assert(codec, `standard codec exists for ${content.kind}`);
    equal(
      comparable(codec.encode(valueOf(content) as never)),
      comparable(encoded),
      `native ${content.kind} codec matches catalogue envelope`,
    );
    const bytes = encodeEncodedContent(encoded);
    const lifted = decodeEncodedContent(bytes);
    bytesEqual(
      lifted.content,
      Array.from(encoded.content),
      `native ${content.kind} payload bytes roundtrip`,
    );
    equal(
      comparable(lifted),
      comparable(encoded),
      `native ${content.kind} envelope fields roundtrip`,
    );
    const decoded = decodeStandard(encoded);
    equal(
      decoded.kind,
      content.kind,
      `native ${content.kind} discriminant roundtrip`,
    );
    // Standard codecs can normalize omitted defaults. Compare the same known
    // sibling vector through both native APIs, as the sibling contract does.
    equal(
      comparable(codec.encode(codec.decode(encoded) as never)),
      comparable(encoded),
      `native ${content.kind} optional/map/byte fields roundtrip`,
    );
    equal(
      comparable(encodeStandard(decoded)),
      comparable(encoded),
      `native ${content.kind} decoded value retains envelope content`,
    );
  }
  return [
    "all shared sibling standard-content and variant vectors cross actual native codecs, preserving envelopes, bytes, options, maps and timestamps",
  ];
}
