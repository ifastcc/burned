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
      previousDefaultDate: "2026-03-23",
      nextDefaultDate: "2026-03-24",
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
      previousDefaultDate: "2026-03-23",
      nextDefaultDate: "2026-03-24",
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

test("getDefaultSelectedDate prefers yesterday when the week includes both today and yesterday", () => {
  assert.equal(typeof dateUtils.getDefaultSelectedDate, "function");
  if (typeof dateUtils.getDefaultSelectedDate !== "function") {
    return;
  }

  assert.equal(
    dateUtils.getDefaultSelectedDate({
      availableDates: [
        "2026-03-19",
        "2026-03-20",
        "2026-03-21",
        "2026-03-22",
        "2026-03-23",
        "2026-03-24",
        "2026-03-25",
      ],
      todayDate: "2026-03-25",
    }),
    "2026-03-24",
  );
});

test("getDefaultSelectedDate falls back to the latest available day when yesterday is missing", () => {
  assert.equal(typeof dateUtils.getDefaultSelectedDate, "function");
  if (typeof dateUtils.getDefaultSelectedDate !== "function") {
    return;
  }

  assert.equal(
    dateUtils.getDefaultSelectedDate({
      availableDates: [
        "2026-03-19",
        "2026-03-20",
        "2026-03-21",
        "2026-03-22",
        "2026-03-23",
        "2026-03-25",
      ],
      todayDate: "2026-03-25",
    }),
    "2026-03-25",
  );
});

test("buildWeeklyBurnCopy uses relative Chinese titles and exact dates for recent days", () => {
  assert.equal(typeof dateUtils.buildWeeklyBurnCopy, "function");
  if (typeof dateUtils.buildWeeklyBurnCopy !== "function") {
    return;
  }

  assert.deepEqual(
    dateUtils.buildWeeklyBurnCopy({
      date: "2026-03-24",
      todayDate: "2026-03-25",
      locale: "zh-CN",
    }),
    {
      title: "昨天消耗",
      metaDate: "周二 3/24",
    },
  );
});

test("buildWeeklyBurnCopy switches to absolute Chinese date titles and short meta for older days", () => {
  assert.equal(typeof dateUtils.buildWeeklyBurnCopy, "function");
  if (typeof dateUtils.buildWeeklyBurnCopy !== "function") {
    return;
  }

  assert.deepEqual(
    dateUtils.buildWeeklyBurnCopy({
      date: "2026-03-21",
      todayDate: "2026-03-25",
      locale: "zh-CN",
    }),
    {
      title: "3月21日消耗",
      metaDate: "周六",
    },
  );
});

test("buildWeeklyBurnCopy includes the year in English titles once the selected day crosses years", () => {
  assert.equal(typeof dateUtils.buildWeeklyBurnCopy, "function");
  if (typeof dateUtils.buildWeeklyBurnCopy !== "function") {
    return;
  }

  assert.deepEqual(
    dateUtils.buildWeeklyBurnCopy({
      date: "2025-12-31",
      todayDate: "2026-01-03",
      locale: "en-US",
    }),
    {
      title: "Burn on Dec 31, 2025",
      metaDate: "Wed",
    },
  );
});
