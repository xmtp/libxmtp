import type { RecordLayout } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/codec.ts";

export const PROPERTY_LAYOUT: RecordLayout = {
  fields: {
    object: { kind: "object", name: "Group" },
    option: { kind: "optional", inner: { kind: "object", name: "Group" } },
    list: { kind: "sequence", inner: { kind: "object", name: "Group" } },
    map: {
      kind: "map",
      key: { kind: "value", type: "String" },
      value: { kind: "object", name: "Group" },
    },
    count: { kind: "value", type: "UInt64" },
    bytes: { kind: "value", type: "Bytes" },
  },
};
