import assert from "node:assert/strict";
import test from "node:test";

import * as dateUtils from "../src/date-utils.mjs";

test("toIsoDateInTimeZone keeps late-evening New York timestamps on the same local day", () => {
  const date = new Date("2026-03-24T03:06:54.000Z");

  assert.equal(dateUtils.toIsoDateInTimeZone(date, "America/New_York"), "2026-03-23");
});

test("toIsoDateInTimeZone rolls forward correctly in east-of-UTC time zones", () => {
  const date = new Date("2026-03-23T16:30:00.000Z");

  assert.equal(dateUtils.toIsoDateInTimeZone(date, "Asia/Shanghai"), "2026-03-24");
});

test("resolveSelectedDateAfterRefresh advances to the new latest day after rollover", () => {
  assert.equal(typeof dateUtils.resolveSelectedDateAfterRefresh, "function");
  if (typeof dateUtils.resolveSelectedDateAfterRefresh !== "function") {
    return;
  }

  assert.equal(
    dateUtils.resolveSelectedDateAfterRefresh({
      currentDate: "2026-03-23",
      previousLatestDate: "2026-03-23",
      nextLatestDate: "2026-03-24",
      availableDates: [
        "2026-03-18",
        "2026-03-19",
        "2026-03-20",
        "2026-03-21",
        "2026-03-22",
        "2026-03-23",
        "2026-03-24",
      ],
    }),
    "2026-03-24",
  );
});

test("resolveSelectedDateAfterRefresh preserves deliberate older selections", () => {
  assert.equal(typeof dateUtils.resolveSelectedDateAfterRefresh, "function");
  if (typeof dateUtils.resolveSelectedDateAfterRefresh !== "function") {
    return;
  }

  assert.equal(
    dateUtils.resolveSelectedDateAfterRefresh({
      currentDate: "2026-03-21",
      previousLatestDate: "2026-03-23",
      nextLatestDate: "2026-03-24",
      availableDates: [
        "2026-03-18",
        "2026-03-19",
        "2026-03-20",
        "2026-03-21",
        "2026-03-22",
        "2026-03-23",
        "2026-03-24",
      ],
    }),
    "2026-03-21",
  );
});
