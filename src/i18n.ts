import type { CalculationMethod, SessionSummary, SourceState } from "./data/schema";

export type Locale =
  | "en-US"
  | "zh-CN"
  | "ja-JP"
  | "ko-KR"
  | "de-DE"
  | "fr-FR"
  | "es-ES";
export type Granularity = "day" | "week" | "month";

type CopyPack = {
  app: {
    eyebrow: string;
    title: string;
    description: (date: string) => string;
    historyChip: string;
    observedToday: string;
    visibleConnectors: string;
    noActiveSource: string;
    moreSources: (count: number) => string;
    refreshNow: string;
    refreshing: string;
    autoRefresh: string;
    lastScanned: (time: string) => string;
    lastScannedPending: string;
    refreshError: string;
    metrics: {
      today: string;
      todayDetail: (exact: string) => string;
      sevenDayAverage: string;
      sevenDayAverageDetail: (exact: string) => string;
      activeConnectors: string;
      activeConnectorsDetail: (active: number, connected: number) => string;
      nativeCoverage: string;
      nativeCoverageDetail: string;
    };
    locale: {
      label: string;
      english: string;
      chinese: string;
    };
  };
  trend: {
    kicker: string;
    title: string;
    emptyTitle: string;
    tabAriaLabel: string;
    earlier: string;
    later: string;
    totalBurn: string;
    confidence: string;
    peakConnectors: string;
    sessionActivity: string;
    confidenceDetail: (granularity: string) => string;
    peakConnectorsDetail: string;
    sessionActivityDetail: string;
    noPriorSlice: (granularity: string) => string;
    previousWindowDelta: (delta: string, granularity: string) => string;
    sourceMixKicker: string;
    sourceMixTitle: string;
    sessionsLabel: (count: number, mix: string) => string;
    costPending: string;
    selectedRange: string;
    selectedRangeDetail: (delta: string) => string;
    averagePerPeriod: (granularity: string) => string;
    averagePerPeriodDetail: (granularity: string) => string;
    peakPeriod: string;
    peakPeriodDetail: (label: string) => string;
    scaleNote: (max: string) => string;
    granularity: Record<Granularity, string>;
    weekSpan: string;
    dayCount: (count: number) => string;
  };
  sources: {
    kicker: string;
    title: string;
    summary: string;
    todayScope: string;
  };
  sessions: {
    kicker: string;
    title: string;
    summary: string;
    indexedCount: (count: number) => string;
    empty: string;
  };
  connectors: {
    lastSeen: string;
  };
  common: {
    ready: string;
    partial: string;
    missing: string;
    native: string;
    derived: string;
    estimated: string;
    mixed: string;
    indexed: string;
    recomputed: string;
    pending: string;
    unknown: string;
  };
};

const copies: Partial<Record<Locale, CopyPack>> = {
  "en-US": {
    app: {
      eyebrow: "Burned / desktop burn tracker",
      title:
        "See the burn clearly across your AI tools without drowning in their storage internals.",
      description: (date) =>
        `Burned keeps the source-native data where it belongs, but gives you a readable timeline for ${date}. Switch between day, week, and month slices, page backward through history, and drill into sessions by connector when something spikes.`,
      historyChip: "180-day rolling history",
      observedToday: "Observed today",
      visibleConnectors: "Visible connectors",
      noActiveSource: "No active source yet",
      moreSources: (count) => ` +${count} more`,
      refreshNow: "Refresh now",
      refreshing: "Refreshing...",
      autoRefresh: "Full rescan every minute while this window is visible",
      lastScanned: (time) => `Last full scan ${time}`,
      lastScannedPending: "Waiting for the first full scan",
      refreshError: "Refresh failed. Burned will try again automatically.",
      metrics: {
        today: "Today's tokens",
        todayDetail: (exact) => `Full count ${exact} tokens`,
        sevenDayAverage: "7-day daily avg",
        sevenDayAverageDetail: (exact) => `Full count ${exact} tokens per day`,
        activeConnectors: "Active connectors",
        activeConnectorsDetail: (active, connected) =>
          `${active}/${connected} connectors wrote usage today`,
        nativeCoverage: "Native coverage",
        nativeCoverageDetail: "Share of today's tokens backed by source-native usage"
      },
      locale: {
        label: "Language",
        english: "EN",
        chinese: "中文"
      }
    },
    trend: {
      kicker: "Burn timeline",
      title: "Move through daily, weekly, and monthly burn",
      emptyTitle: "No usage history has been indexed yet.",
      tabAriaLabel: "Time granularity",
      earlier: "Earlier",
      later: "Later",
      totalBurn: "Total burn",
      confidence: "Native coverage",
      peakConnectors: "Peak connectors",
      sessionActivity: "Sessions in view",
      confidenceDetail: (granularity) => `Weighted by the selected ${granularity} window`,
      peakConnectorsDetail: "Highest simultaneous connector count in the slice",
      sessionActivityDetail: "Summed active sessions across the visible timeline",
      noPriorSlice: (granularity) => `No prior ${granularity.toLowerCase()} slice yet`,
      previousWindowDelta: (delta, granularity) =>
        `${delta} vs previous ${granularity.toLowerCase()} window`,
      sourceMixKicker: "Source mix",
      sourceMixTitle: "Where today's burn comes from",
      sessionsLabel: (count, mix) => `${count} sessions · ${mix}`,
      costPending: "cost pending",
      selectedRange: "Selected range",
      selectedRangeDetail: (delta) => delta,
      averagePerPeriod: (granularity) => `Average per ${granularity.toLowerCase()}`,
      averagePerPeriodDetail: (granularity) =>
        `Across the visible ${granularity.toLowerCase()} window`,
      peakPeriod: "Peak slice",
      peakPeriodDetail: (label) => `${label} was the highest point in view`,
      scaleNote: (max) => `Chart scale: 0 to ${max} tokens`,
      granularity: {
        day: "Day",
        week: "Week",
        month: "Month"
      },
      weekSpan: "Mon-Sun",
      dayCount: (count) => `${count} days`
    },
    sources: {
      kicker: "Today's mix",
      title: "Separate today's sources from the history window",
      summary:
        "This section is always about today only, so it does not drift when you page through older history above or below it.",
      todayScope: "Today only"
    },
    sessions: {
      kicker: "Connector sessions",
      title: "Review sessions underneath each connector",
      summary:
        "Titles and previews stay source-native, so you can quickly spot what actually burned the tokens.",
      indexedCount: (count) => `${count} indexed sessions`,
      empty: "No sessions indexed yet for this connector."
    },
    connectors: {
      lastSeen: "Last seen",
    },
    common: {
      ready: "ready",
      partial: "partial",
      missing: "missing",
      native: "native",
      derived: "derived",
      estimated: "estimated",
      mixed: "mixed",
      indexed: "indexed",
      recomputed: "recomputed",
      pending: "pending",
      unknown: "unknown"
    }
  },
  "zh-CN": {
    app: {
      eyebrow: "Burned / 桌面端烧量面板",
      title: "更清楚地看见各个 AI 工具的消耗，而不是被它们各自的存储细节淹没。",
      description: (date) =>
        `Burned 保留各个来源自己的原始数据结构，但会为你整理出 ${date} 的可读时间线。你可以按天、按周、按月切换，向前翻看更久的历史，并按 connector 下钻到具体 session。`,
      historyChip: "180 天滚动历史",
      observedToday: "今日观测",
      visibleConnectors: "可见连接器",
      noActiveSource: "当前还没有活跃来源",
      moreSources: (count) => ` +${count} 个`,
      refreshNow: "立即刷新",
      refreshing: "刷新中...",
      autoRefresh: "窗口可见时每分钟执行一次全量重扫",
      lastScanned: (time) => `上次全量扫描：${time}`,
      lastScannedPending: "正在等待第一次全量扫描",
      refreshError: "本次刷新失败，Burned 会继续自动重试。",
      metrics: {
        today: "今日 Token",
        todayDetail: (exact) => `完整数值 ${exact} tokens`,
        sevenDayAverage: "7 日日均",
        sevenDayAverageDetail: (exact) => `完整数值 ${exact} tokens / 天`,
        activeConnectors: "活跃连接器",
        activeConnectorsDetail: (active, connected) =>
          `今天有 ${active}/${connected} 个连接器写出了 usage`,
        nativeCoverage: "原生覆盖率",
        nativeCoverageDetail: "今天有多少 token 直接来自来源软件原生 usage"
      },
      locale: {
        label: "语言",
        english: "EN",
        chinese: "中文"
      }
    },
    trend: {
      kicker: "烧量时间线",
      title: "按天、按周、按月查看 burn 变化",
      emptyTitle: "还没有索引到 usage 历史。",
      tabAriaLabel: "时间粒度",
      earlier: "更早",
      later: "更近",
      totalBurn: "总烧量",
      confidence: "原生覆盖率",
      peakConnectors: "峰值连接器数",
      sessionActivity: "窗口内 Session 数",
      confidenceDetail: (granularity) => `按当前${granularity}窗口加权计算`,
      peakConnectorsDetail: "所选切片中同时活跃的最高连接器数量",
      sessionActivityDetail: "所选时间窗口内汇总的活跃 session 数",
      noPriorSlice: (granularity) => `还没有更早的${granularity}窗口`,
      previousWindowDelta: (delta, granularity) => `较上一${granularity}窗口 ${delta}`,
      sourceMixKicker: "来源构成",
      sourceMixTitle: "今天的 burn 来自哪里",
      sessionsLabel: (count, mix) => `${count} 个 session · ${mix}`,
      costPending: "成本待接入",
      selectedRange: "当前时间窗",
      selectedRangeDetail: (delta) => delta,
      averagePerPeriod: (granularity) => `每${granularity}平均`,
      averagePerPeriodDetail: (granularity) => `按当前可见${granularity}窗口计算`,
      peakPeriod: "峰值切片",
      peakPeriodDetail: (label) => `${label} 是当前视图里的最高点`,
      scaleNote: (max) => `图表刻度：0 到 ${max} tokens`,
      granularity: {
        day: "天",
        week: "周",
        month: "月"
      },
      weekSpan: "周一到周日",
      dayCount: (count) => `${count} 天`
    },
    sources: {
      kicker: "今日来源",
      title: "把今天的数据和历史窗口拆开看",
      summary:
        "这里永远只统计今天，所以不会因为你翻看更早的历史窗口而和趋势图口径混在一起。",
      todayScope: "仅今天"
    },
    sessions: {
      kicker: "Connector Session",
      title: "按 connector 查看下面的 session",
      summary:
        "标题和预览保持来源原样，这样你可以更快判断到底是哪一类对话烧掉了 token。",
      indexedCount: (count) => `已索引 ${count} 个 session`,
      empty: "这个 connector 下面还没有索引到 session。"
    },
    connectors: {
      lastSeen: "最近看到",
    },
    common: {
      ready: "就绪",
      partial: "部分可用",
      missing: "缺失",
      native: "原生",
      derived: "推导",
      estimated: "估算",
      mixed: "混合",
      indexed: "已索引",
      recomputed: "已重算",
      pending: "待定",
      unknown: "未知"
    }
  }
};

const englishCopy = copies["en-US"];
if (englishCopy) {
  copies["ja-JP"] = englishCopy;
  copies["ko-KR"] = englishCopy;
  copies["de-DE"] = englishCopy;
  copies["fr-FR"] = englishCopy;
  copies["es-ES"] = englishCopy;
}

export const supportedLocales: Locale[] = [
  "en-US",
  "zh-CN",
  "ja-JP",
  "ko-KR",
  "de-DE",
  "fr-FR",
  "es-ES"
];

const localeLabels: Record<Locale, string> = {
  "en-US": "English",
  "zh-CN": "中文",
  "ja-JP": "日本語",
  "ko-KR": "한국어",
  "de-DE": "Deutsch",
  "fr-FR": "Français",
  "es-ES": "Español"
};

const localeFamilyFallbacks: Record<string, Locale> = {
  en: "en-US",
  zh: "zh-CN",
  ja: "ja-JP",
  ko: "ko-KR",
  de: "de-DE",
  fr: "fr-FR",
  es: "es-ES"
};

function isSupportedLocale(value: string | null | undefined): value is Locale {
  return value != null && supportedLocales.includes(value as Locale);
}

function resolvePreferredLocale(
  stored: string | null | undefined,
  systemLocales: readonly string[],
): Locale {
  if (isSupportedLocale(stored)) {
    return stored;
  }

  for (const candidate of systemLocales) {
    if (isSupportedLocale(candidate)) {
      return candidate;
    }
  }

  for (const candidate of systemLocales) {
    const family = candidate.toLowerCase().split("-")[0];
    const fallback = localeFamilyFallbacks[family];
    if (fallback) {
      return fallback;
    }
  }

  return "en-US";
}

export function detectInitialLocale(): Locale {
  if (typeof window !== "undefined") {
    return resolvePreferredLocale(
      window.localStorage.getItem("burned.locale"),
      window.navigator.languages.length > 0
        ? window.navigator.languages
        : [window.navigator.language],
    );
  }

  return "en-US";
}

export function getCopy(locale: Locale) {
  return copies[locale] ?? copies["en-US"]!;
}

export function getLocaleLabel(locale: Locale) {
  return localeLabels[locale];
}

export function localeTag(locale: Locale) {
  return locale;
}

export function formatCompactNumber(
  value: number,
  locale: Locale,
  maximumFractionDigits = 1
) {
  return new Intl.NumberFormat(localeTag(locale), {
    maximumFractionDigits,
    notation: "compact"
  }).format(value);
}

export function formatFriendlyNumber(
  value: number,
  locale: Locale,
  maximumFractionDigits = 1,
  compactThreshold = 10_000
) {
  if (Math.abs(value) >= compactThreshold) {
    return formatCompactNumber(value, locale, maximumFractionDigits);
  }

  return formatNumber(value, locale);
}

export function formatNumber(
  value: number,
  locale: Locale,
  maximumFractionDigits = 0
) {
  return new Intl.NumberFormat(localeTag(locale), {
    maximumFractionDigits
  }).format(value);
}

export function formatPercent(value: number, locale: Locale) {
  return new Intl.NumberFormat(localeTag(locale), {
    style: "percent",
    maximumFractionDigits: 0
  }).format(value);
}

function parseDateInput(value: string) {
  const parsed = /^\d{4}-\d{2}-\d{2}$/.test(value)
    ? new Date(`${value}T12:00:00`)
    : new Date(value);

  if (Number.isNaN(parsed.getTime())) {
    return null;
  }

  return parsed;
}

export function formatLocalizedDateTime(isoDateTime: string | undefined, locale: Locale) {
  if (!isoDateTime) {
    return null;
  }

  const parsed = parseDateInput(isoDateTime);
  if (!parsed) {
    return null;
  }

  return new Intl.DateTimeFormat(localeTag(locale), {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  }).format(parsed);
}

export function sourceStateLabel(locale: Locale, state: SourceState) {
  return getCopy(locale).common[state];
}

export function calculationLabel(
  locale: Locale,
  method: CalculationMethod | "mixed"
) {
  if (method === "mixed") {
    return getCopy(locale).common.mixed;
  }

  return getCopy(locale).common[method];
}

export function sessionStatusLabel(
  locale: Locale,
  status: SessionSummary["status"]
) {
  return getCopy(locale).common[status];
}
