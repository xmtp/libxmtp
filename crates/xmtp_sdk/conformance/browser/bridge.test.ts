import { describe } from "vitest";

import { registerCallbacksTests } from "./bridge-callbacks";
import { registerCreateTests } from "./bridge-create";
import { registerOwnershipTests } from "./bridge-ownership";
import { registerTransportTests } from "./bridge-transport";

describe("browser bridge transport", () => {
  registerTransportTests();
  registerOwnershipTests();
  registerCreateTests();
  registerCallbacksTests();
});
