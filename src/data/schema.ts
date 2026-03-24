export type CalculationMethod = "native" | "derived" | "estimated";
export type SourceState = "ready" | "partial" | "missing";

export type DailyUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number;
  exactShare: number;
  activeSources: number;
  sessionCount: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  pricingCoverage: PricingCoverage;
};

export type PeakUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number;
};

export type PricingCoverage = "actual" | "partial" | "pending";

export type WindowDelta = {
  tokensDelta: number;
  tokensPercentChange: number | null;
};

export type UsageWindowSummary = {
  tokens: number;
  costUsd: number;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  activeDays: number;
  avgPerActiveDay: number;
  exactShare: number;
  peakDay: PeakUsagePoint | null;
  pricingCoverage: PricingCoverage;
  deltaVsPreviousPeriod: WindowDelta | null;
};

export type PeriodicBreakdownRow = {
  label: string;
  startDate: string;
  endDate: string;
  tokens: number;
  costUsd: number;
  sessions: number;
  pricedSessions: number;
  pendingPricingSessions: number;
  pricingCoverage: PricingCoverage;
  activeDays: number;
};

export type PeriodicBreakdownSet = {
  weekly: PeriodicBreakdownRow[];
  monthly: PeriodicBreakdownRow[];
};

export type BillingState = {
  kind: "credits" | "quota";
  state: "ready" | "partial" | "unavailable";
  current: number | null;
  limit: number | null;
  unit: string | null;
  updatedAt: string | null;
  note: string | null;
};

export type SourceUsage = {
  sourceId: string;
  source: string;
  tokens: number;
  costUsd: number;
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
  pricedSessions: number;
  pendingPricingSessions: number;
  pricingCoverage: PricingCoverage;
  pricingState: "actual" | "pending";
  calculationMethod: CalculationMethod;
  status: "indexed" | "recomputed" | "pending";
  parentSessionId?: string | null;
  sessionRole: "primary" | "subagent";
  agentLabel?: string | null;
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
  calculationMix: CalculationMethod | "mixed";
  todayTokens: number;
  todayCostUsd: number;
  week: DailyUsagePoint[];
  dailyHistory: DailyUsagePoint[];
  sessions: SessionSummary[];
  todaySummary: UsageWindowSummary;
  last7dSummary: UsageWindowSummary;
  last30dSummary: UsageWindowSummary;
  lifetimeSummary: UsageWindowSummary;
  periodicBreakdowns: PeriodicBreakdownSet;
  billingState: BillingState | null;
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
