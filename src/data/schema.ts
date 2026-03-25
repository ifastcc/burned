export type CalculationMethod = "native" | "derived" | "estimated";
export type SourceState = "ready" | "partial" | "missing";
export type PricingCoverage = "complete" | "partial" | "pending";

export type LongContextSummary = {
  sessionCount: number;
  extraCostUsd: number;
};

export type LongContextSessionSummary = {
  peakInputTokens: number;
  extraCostUsd: number;
};

export type DailyUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number;
  pricingCoverage: PricingCoverage;
  exactShare: number;
  activeSources: number;
  sessionCount: number;
};

export type SourceUsage = {
  sourceId: string;
  source: string;
  tokens: number;
  costUsd: number;
  pricingCoverage: PricingCoverage;
  sessions: number;
  trend: "up" | "flat" | "down";
  calculationMix: CalculationMethod | "mixed";
};

export type SessionSummary = {
  id: string;
  sourceId: string;
  title: string;
  preview: string;
  source: string;
  workspace: string;
  model: string;
  startedAt: string;
  totalTokens: number;
  costUsd: number;
  pricingCoverage: PricingCoverage;
  longContext?: LongContextSessionSummary | null;
  calculationMethod: CalculationMethod;
  status: "indexed" | "recomputed" | "pending";
};

export type SourceStatus = {
  id: string;
  name: string;
  state: SourceState;
  capabilities: string[];
  note: string;
  localPath?: string | null;
  sessionCount?: number | null;
  lastSeenAt?: string | null;
};

export type SessionGroup = {
  sourceId: string;
  sourceName: string;
  sourceState: SourceState;
  sessions: SessionSummary[];
};

export type DashboardSnapshot = {
  headlineDate: string;
  totalTokensToday: number;
  totalCostToday: number;
  pricingCoverage: PricingCoverage;
  longContextToday: LongContextSummary;
  exactShare: number;
  connectedSources: number;
  activeSources: number;
  burnRatePerHour: number;
  week: DailyUsagePoint[];
  dailyHistory: DailyUsagePoint[];
  sources: SourceUsage[];
  sessions: SessionSummary[];
  sessionGroups: SessionGroup[];
  sourceStatuses: SourceStatus[];
};

export type SourceDetailSnapshot = {
  sourceId: string;
  sourceName: string;
  status: SourceStatus;
  calculationMix: CalculationMethod | "mixed";
  todayTokens: number;
  todayCostUsd: number;
  pricingCoverage: PricingCoverage;
  longContext: LongContextSummary;
  week: DailyUsagePoint[];
  dailyHistory: DailyUsagePoint[];
  sessions: SessionSummary[];
};
