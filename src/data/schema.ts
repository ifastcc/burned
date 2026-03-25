export type CalculationMethod = "native" | "derived" | "estimated";
export type SourceState = "ready" | "partial" | "missing";
export type AnalyticsState = "ready" | "session_only" | "unavailable";
export type PricingCoverage = "actual" | "partial" | "pending";

export type DailyUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number;
  exactShare: number;
  activeSources: number;
  sessionCount: number;
};

export type PeakUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number | null;
  sessionCount: number;
};

export type WindowDelta = {
  tokensDelta: number;
  tokensPercentChange: number | null;
};

export type UsageWindowSummary = {
  tokens: number;
  costUsd: number | null;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  activeDays: number;
  avgPerActiveDay: number;
  exactShare: number;
  pricingCoverage: PricingCoverage;
  peakDay: PeakUsagePoint | null;
  deltaVsPreviousPeriod: WindowDelta | null;
};

export type PeriodicBreakdownRow = {
  label: string;
  startDate: string;
  endDate: string;
  tokens: number;
  costUsd: number | null;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  activeDays: number;
  pricingCoverage: PricingCoverage;
};

export type PeriodicBreakdowns = {
  weekly: PeriodicBreakdownRow[];
  monthly: PeriodicBreakdownRow[];
};

export type SourceUsage = {
  sourceId: string;
  source: string;
  analyticsState: AnalyticsState;
  tokens: number | null;
  costUsd: number | null;
  sessions: number | null;
  trend: "up" | "flat" | "down" | null;
  pricingCoverage: PricingCoverage | null;
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
  analyticsState: AnalyticsState;
  calculationMix: CalculationMethod | "mixed";
  todaySummary: UsageWindowSummary | null;
  last7dSummary: UsageWindowSummary | null;
  last30dSummary: UsageWindowSummary | null;
  lifetimeSummary: UsageWindowSummary | null;
  periodicBreakdowns: PeriodicBreakdowns | null;
  week: DailyUsagePoint[];
  dailyHistory: DailyUsagePoint[];
  sessions: SessionSummary[];
};

export type CherryStudioSettings = {
  preferredBackupDir?: string | null;
  knownBackupDirs: string[];
  lastVerifiedAt?: string | null;
  lastSuccessArchive?: string | null;
};

export type AppSettings = {
  cherryStudio: CherryStudioSettings;
};
