import type { DashboardSnapshot } from "./schema";

export function createEmptyDashboardSnapshot(now = new Date()): DashboardSnapshot {
  const isoDate = [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, "0"),
    String(now.getDate()).padStart(2, "0")
  ].join("-");

  return {
    headlineDate: isoDate,
    totalTokensToday: 0,
    totalCostToday: 0,
    pricingCoverage: "complete",
    longContextToday: {
      sessionCount: 0,
      extraCostUsd: 0
    },
    exactShare: 0,
    connectedSources: 0,
    activeSources: 0,
    burnRatePerHour: 0,
    week: [],
    dailyHistory: [],
    sources: [],
    sessions: [],
    sessionGroups: [],
    sourceStatuses: []
  };
}
