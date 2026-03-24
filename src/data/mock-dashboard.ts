import type { DashboardSnapshot, DailyUsagePoint, SessionGroup, SessionSummary } from "./schema";

function buildDailyHistory(): DailyUsagePoint[] {
  const start = new Date("2025-09-24T00:00:00");
  const totalDays = 180;

  return Array.from({ length: totalDays }, (_, index) => {
    const date = new Date(start);
    date.setDate(start.getDate() + index);

    const weekday = date.getDay();
    const base = 280000 + ((index % 17) * 34000);
    const weeklyLift = weekday === 1 || weekday === 2 ? 220000 : weekday === 0 ? 70000 : 150000;
    const monthlyWave = Math.floor(((index % 31) / 31) * 260000);
    const totalTokens = base + weeklyLift + monthlyWave + ((index % 5) * 19000);
    const sessionCount = 3 + (index % 6) + (weekday === 1 ? 4 : 0);
    const activeSources = 2 + (index % 4);
    const exactShare = 0.52 + ((index % 9) * 0.035);

    return {
      date: date.toISOString().slice(0, 10),
      totalTokens,
      totalCostUsd: 0,
      exactShare: Math.min(0.92, Number(exactShare.toFixed(2))),
      activeSources,
      sessionCount
    };
  });
}

const dailyHistory = buildDailyHistory();
const week = dailyHistory.slice(-7);

const codexSessions: SessionSummary[] = [
  {
    id: "codex-7781",
    sourceId: "codex",
    title: "Build the Burned desktop shell",
    preview:
      "Scaffold a desktop-first observability app with local ingestion, SQLite, and a dashboard for daily token burn.",
    source: "Codex",
    workspace: "product",
    model: "gpt-5.4",
    startedAt: "Mar 22 21:14",
    totalTokens: 242000,
    costUsd: 0,
    calculationMethod: "native",
    status: "indexed"
  },
  {
    id: "codex-8012",
    sourceId: "codex",
    title: "Audit Codex log ingestion",
    preview:
      "Verified response.completed events, token counters, and conversation IDs before wiring the native usage path.",
    source: "Codex",
    workspace: "product",
    model: "gpt-5.4-mini",
    startedAt: "Mar 22 18:42",
    totalTokens: 198000,
    costUsd: 0,
    calculationMethod: "native",
    status: "indexed"
  }
];

const claudeSessions: SessionSummary[] = [
  {
    id: "claude-9812",
    sourceId: "claude_code",
    title: "Inspect Cherry sync worker failures",
    preview:
      "Tracked the retry path, isolated the stale cursor bug, and outlined the patch plan before recomputation.",
    source: "Claude Code",
    workspace: "sync-server",
    model: "claude-opus-4.1",
    startedAt: "Mar 22 18:42",
    totalTokens: 198000,
    costUsd: 0,
    calculationMethod: "native",
    status: "indexed"
  },
  {
    id: "claude-9837",
    sourceId: "claude_code",
    title: "Review local project transcripts",
    preview:
      "Pulled titles and previews from project JSONL files, then checked whether assistant usage was present per message.",
    source: "Claude Code",
    workspace: "product",
    model: "claude-sonnet-4-6",
    startedAt: "Mar 21 13:09",
    totalTokens: 121000,
    costUsd: 0,
    calculationMethod: "native",
    status: "indexed"
  }
];

const cursorSessions: SessionSummary[] = [
  {
    id: "cursor-4820",
    sourceId: "cursor",
    title: "Refactor adapter loading in Burned",
    preview:
      "Cursor session index is present locally; token truth still needs the admin usage surface for native numbers.",
    source: "Cursor",
    workspace: "burned-adapters",
    model: "unknown",
    startedAt: "Mar 22 16:08",
    totalTokens: 0,
    costUsd: 0,
    calculationMethod: "estimated",
    status: "indexed"
  },
  {
    id: "cursor-5007",
    sourceId: "cursor",
    title: "删除当前目录下的文件",
    preview:
      "The user requested to delete a list of specified files and directories from the current directory, then verified cleanup.",
    source: "Cursor",
    workspace: ".claude/projects",
    model: "unknown",
    startedAt: "Jun 16 08:13",
    totalTokens: 0,
    costUsd: 0,
    calculationMethod: "estimated",
    status: "indexed"
  }
];

const antigravitySessions: SessionSummary[] = [
  {
    id: "18daac03-0e7b-4604-837f-08337ea7e362",
    sourceId: "antigravity",
    title: "Poe Extractor Refinement Task List",
    preview: "Initializing the task list for Poe Extractor refinement.",
    source: "Antigravity",
    workspace: "brain",
    model: "unknown",
    startedAt: "Feb 3 13:16",
    totalTokens: 0,
    costUsd: 0,
    calculationMethod: "estimated",
    status: "indexed"
  },
  {
    id: "7beb108e-08f9-435b-ae17-9973ace0da13",
    sourceId: "antigravity",
    title: "Compiling OpenClaw Use Cases",
    preview:
      "Task plan to compile OpenClaw use cases from the GitHub repository into a detailed summary.",
    source: "Antigravity",
    workspace: "brain",
    model: "unknown",
    startedAt: "Mar 1 03:39",
    totalTokens: 0,
    costUsd: 0,
    calculationMethod: "estimated",
    status: "indexed"
  }
];

const cherrySessions: SessionSummary[] = [
  {
    id: "cherry-burn-01",
    sourceId: "cherry_studio",
    title: "Map long-form reflection topics",
    preview:
      "Recent Cherry topics are being indexed through the local History API, so titles and previews can be surfaced without scraping raw IndexedDB files first.",
    source: "Cherry Studio",
    workspace: "think_archive",
    model: "gpt-5-4-pro",
    startedAt: "Mar 22 13:04",
    totalTokens: 0,
    costUsd: 0,
    calculationMethod: "estimated",
    status: "indexed"
  },
  {
    id: "cherry-burn-02",
    sourceId: "cherry_studio",
    title: "Capture AI-era decision threads",
    preview:
      "The connector can see ordinary Cherry topics via the local History API and agent sessions through agents.db, but stable token usage is still best-effort.",
    source: "Cherry Studio",
    workspace: "history",
    model: "gemini-3-pro-preview",
    startedAt: "Mar 22 10:58",
    totalTokens: 0,
    costUsd: 0,
    calculationMethod: "estimated",
    status: "indexed"
  }
];

const sessionGroups: SessionGroup[] = [
  { sourceId: "codex", sourceName: "Codex", sourceState: "ready", sessions: codexSessions },
  {
    sourceId: "claude_code",
    sourceName: "Claude Code",
    sourceState: "ready",
    sessions: claudeSessions
  },
  { sourceId: "cursor", sourceName: "Cursor", sourceState: "partial", sessions: cursorSessions },
  {
    sourceId: "cherry_studio",
    sourceName: "Cherry Studio",
    sourceState: "partial",
    sessions: cherrySessions
  },
  {
    sourceId: "antigravity",
    sourceName: "Antigravity",
    sourceState: "partial",
    sessions: antigravitySessions
  }
];

export const mockDashboard: DashboardSnapshot = {
  headlineDate: "March 22, 2026",
  totalTokensToday: week[week.length - 1]?.totalTokens ?? 0,
  totalCostToday: 0,
  exactShare: week[week.length - 1]?.exactShare ?? 0,
  connectedSources: 5,
  activeSources: 4,
  burnRatePerHour: 132000,
  week,
  dailyHistory,
  sources: [
    {
      source: "Codex",
      tokens: 612000,
      costUsd: 0,
      sessions: 12,
      trend: "up",
      calculationMix: "native"
    },
    {
      source: "Claude Code",
      tokens: 428000,
      costUsd: 0,
      sessions: 9,
      trend: "up",
      calculationMix: "native"
    },
    {
      source: "Cursor",
      tokens: 0,
      costUsd: 0,
      sessions: 8,
      trend: "flat",
      calculationMix: "estimated"
    },
    {
      source: "Cherry Studio",
      tokens: 0,
      costUsd: 0,
      sessions: 24,
      trend: "flat",
      calculationMix: "estimated"
    },
    {
      source: "Antigravity",
      tokens: 0,
      costUsd: 0,
      sessions: 6,
      trend: "flat",
      calculationMix: "estimated"
    }
  ],
  sessions: [
    ...codexSessions,
    ...claudeSessions,
    ...cursorSessions,
    ...cherrySessions,
    ...antigravitySessions
  ].slice(
    0,
    8
  ),
  sessionGroups,
  sourceStatuses: [
    {
      id: "codex",
      name: "Codex",
      state: "ready",
      capabilities: [
        "local-sqlite",
        "native-tokens",
        "log-ingestion",
        "app-server-events"
      ],
      note: "Native session totals and per-turn token logs are wired in.",
      localPath: "/Users/kbaicai/.codex",
      sessionCount: 142,
      lastSeenAt: "2026-03-22 22:45"
    },
    {
      id: "claude_code",
      name: "Claude Code",
      state: "ready",
      capabilities: ["local-jsonl", "native-tokens", "cli-json", "analytics-api"],
      note: "Project session JSONL files expose assistant usage locally.",
      localPath: "/Users/kbaicai/.claude",
      sessionCount: 96,
      lastSeenAt: "2026-03-21 09:20"
    },
    {
      id: "cursor",
      name: "Cursor",
      state: "partial",
      capabilities: ["local-sqlite", "session-index", "admin-api-tokens"],
      note: "Local history is visible, but native token truth should come from the Admin API.",
      localPath: "/Users/kbaicai/Library/Application Support/Cursor",
      sessionCount: 74,
      lastSeenAt: "2026-03-22 20:10"
    },
    {
      id: "cherry_studio",
      name: "Cherry Studio",
      state: "partial",
      capabilities: ["history-api", "history-transcript", "local-agents-db", "zip-backups"],
      note: "The local History API is live for ordinary topics. Agent sessions come from agents.db; token usage is still best-effort.",
      localPath: "/Users/kbaicai/Library/Application Support/CherryStudio",
      sessionCount: 2221,
      lastSeenAt: "2026-03-22 08:02"
    },
    {
      id: "antigravity",
      name: "Antigravity",
      state: "partial",
      capabilities: ["raw-artifacts", "workspace-storage", "logs"],
      note: "Raw artifacts and logs are present; token fields remain unverified.",
      localPath: "/Users/kbaicai/.gemini/antigravity",
      sessionCount: 58,
      lastSeenAt: "2026-03-22 20:47"
    }
  ]
};
