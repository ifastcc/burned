export type CalculationMethod = "native" | "derived" | "estimated";
export type SourceState = "ready" | "partial" | "missing";

export type DailyUsagePoint = {
  date: string;
  totalTokens: number;
  totalCostUsd: number;
  exactShare: number;
  activeSources: number;
  sessionCount: number;
};

export type SourceUsage = {
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

export type CherryStudioSettings = {
  preferredBackupDir?: string | null;
  knownBackupDirs: string[];
  lastVerifiedAt?: string | null;
  lastSuccessArchive?: string | null;
};

export type AppSettings = {
  cherryStudio: CherryStudioSettings;
};
