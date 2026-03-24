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
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
const appSource = fs.readFileSync(appPath, "utf8");
const schemaSource = fs.readFileSync(schemaPath, "utf8");

test("zh-CN hero copy leads with a punchier today-focused question", () => {
  assert.equal(showcaseCopy["zh-CN"].tagline, "你今天已经烧掉多少 token？");
});

test("package.json exposes a real app startup flow without replacing pnpm dev", () => {
  assert.equal(packageJson.scripts.dev, "node ./scripts/dev-server.mjs");
  assert.equal(packageJson.scripts["dev:app"], "pnpm build && node ./bin/burned.mjs");
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

test("connector detail page exposes summary cards and periodic breakdowns", () => {
  assert.match(appSource, /source-summary-card/);
  assert.match(appSource, /periodic-breakdown/);
  assert.match(appSource, /session-sort/);
  assert.match(appSource, /\bToday\b/);
  assert.match(appSource, /\b7D\b/);
  assert.match(appSource, /\b30D\b/);
  assert.match(appSource, /\bLifetime\b/);
});

test("connector cards route into the source detail page", () => {
  assert.match(appSource, /className="conn-card"/);
  assert.match(appSource, /onOpenSource/);
});

test("source rows keep routing into the source detail page", () => {
  assert.match(appSource, /SourceList/);
  assert.match(appSource, /window\.history\.pushState/);
});

test("pricing states stay distinct across actual, pending, and non-USD billing", () => {
  assert.match(appSource, /pricing pending/);
  assert.match(appSource, /estimatedCost/);
  assert.doesNotMatch(appSource, /Antigravity.*\$0\.00/);
  assert.doesNotMatch(appSource, /antigravity.*estimatedCost/i);
});
