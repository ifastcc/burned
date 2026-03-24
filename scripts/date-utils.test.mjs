import assert from "node:assert/strict";
import test from "node:test";

import { toIsoDateInTimeZone } from "../src/date-utils.mjs";

test("toIsoDateInTimeZone keeps late-evening New York timestamps on the same local day", () => {
  const date = new Date("2026-03-24T03:06:54.000Z");

  assert.equal(toIsoDateInTimeZone(date, "America/New_York"), "2026-03-23");
});

test("toIsoDateInTimeZone rolls forward correctly in east-of-UTC time zones", () => {
  const date = new Date("2026-03-23T16:30:00.000Z");

  assert.equal(toIsoDateInTimeZone(date, "Asia/Shanghai"), "2026-03-24");
});
