import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { showcaseCopy } from "../src/showcase-copy.mjs";

const dirname = path.dirname(fileURLToPath(import.meta.url));
const packageJsonPath = path.join(dirname, "..", "package.json");
const appPath = path.join(dirname, "..", "src", "App.tsx");
const schemaPath = path.join(dirname, "..", "src", "data", "schema.ts");
const i18nPath = path.join(dirname, "..", "src", "i18n.ts");
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
const appSource = fs.readFileSync(appPath, "utf8");
const schemaSource = fs.readFileSync(schemaPath, "utf8");
const i18nSource = fs.readFileSync(i18nPath, "utf8");

test("zh-CN hero copy leads with a punchier today-focused question", () => {
  assert.equal(showcaseCopy["zh-CN"].tagline, "你今天已经烧掉多少 token？");
});

test("package.json exposes a real app startup flow without replacing pnpm dev", () => {
  assert.equal(packageJson.scripts.dev, "node ./scripts/dev-server.mjs");
  assert.equal(packageJson.scripts["dev:app"], "pnpm build && node ./bin/burned.mjs");
});

test("package.json exposes release shortcuts for patch, minor, and major publishes", () => {
  assert.equal(packageJson.scripts["release:patch"], "node ./scripts/release.mjs patch");
  assert.equal(packageJson.scripts["release:minor"], "node ./scripts/release.mjs minor");
  assert.equal(packageJson.scripts["release:major"], "node ./scripts/release.mjs major");
});

test("trend area splits the 7-day and 30-day stories into separate cards", () => {
  assert.match(appSource, /function WeeklyBurnCard\(/);
  assert.match(appSource, /function MonthlyTrendCard\(/);
  assert.doesNotMatch(appSource, /function TrendPanel\(/);
});

test("trend cards expose interactive inspection affordances", () => {
  assert.match(appSource, /className="trend-inspector"/);
  assert.match(appSource, /flame-hitbox/);
  assert.match(appSource, /spark-point-button/);
});

test("weekly card leads with a day-focus summary instead of repeating the date", () => {
  assert.match(appSource, /title=\{sc\.weekFocusTitle\}/);
  assert.match(appSource, /className="weekly-focus-value"/);
  assert.match(appSource, /className="weekly-focus-meta"/);
  assert.match(appSource, /const latestDay = data\[data.length - 1\];/);
  assert.doesNotMatch(
    appSource,
    /<h2 className="trend-title">\{formatDayStamp\(activeDay\.date, locale\)\}<\/h2>/,
  );
});

test("the app never falls back to mock dashboard data", () => {
  const removedFileName = `${["mock", "dashboard"].join("-")}.ts`;
  const removedSymbol = ["mock", "Dashboard"].join("");
  const removedEnvFlag = ["VITE", "USE", "MOCK", "DASHBOARD"].join("_");
  const removedDashboardPath = path.join(
    dirname,
    "..",
    "src",
    "data",
    removedFileName
  );

  assert.equal(fs.existsSync(removedDashboardPath), false);
  assert.equal(appSource.includes(removedSymbol), false);
  assert.equal(appSource.includes(removedEnvFlag), false);
});

test("source usage records carry stable ids for source-detail navigation", () => {
  assert.match(schemaSource, /sourceId: string;/);
});

test("trend inspectors surface daily price state", () => {
  assert.match(appSource, /trend-inspector-cost/);
  assert.match(appSource, /day\.totalCostUsd/);
});

test("source rows drill into a dedicated source detail page", () => {
  assert.match(appSource, /function getSourceSnapshot\(/);
  assert.match(appSource, /function SourceDetailPage\(/);
  assert.match(appSource, /window\.history\.pushState/);
});

test("homepage removes the passive connector grid", () => {
  assert.doesNotMatch(appSource, /function ConnectorGrid\(/);
  assert.doesNotMatch(appSource, /SectionHeader label=\{sc\.connected\}/);
});

test("supported locales scale beyond the current two-option switch", () => {
  assert.match(i18nSource, /ja-JP/);
  assert.match(i18nSource, /ko-KR/);
  assert.match(i18nSource, /de-DE/);
  assert.match(i18nSource, /fr-FR/);
  assert.match(i18nSource, /es-ES/);
});

test("source usage rows and detail snapshots carry analytics-state fields", () => {
  assert.match(schemaSource, /analyticsState:/);
  assert.match(schemaSource, /pricingCoverage:/);
  assert.match(schemaSource, /todaySummary:/);
  assert.match(schemaSource, /last7dSummary:/);
  assert.match(schemaSource, /last30dSummary:/);
  assert.match(schemaSource, /lifetimeSummary:/);
});
