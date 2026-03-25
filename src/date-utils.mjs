const formatterCache = new Map();
const DAY_IN_MS = 24 * 60 * 60 * 1000;

function getFormatter(timeZone) {
  const cacheKey = timeZone ?? "__local__";
  const cached = formatterCache.get(cacheKey);

  if (cached) {
    return cached;
  }

  const formatter = new Intl.DateTimeFormat("en-US", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit"
  });

  formatterCache.set(cacheKey, formatter);
  return formatter;
}

function assertValidDate(date) {
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) {
    throw new TypeError("Expected a valid Date instance.");
  }
}

export function toIsoDateInTimeZone(date, timeZone) {
  assertValidDate(date);

  const formatter = getFormatter(timeZone);
  const parts = formatter.formatToParts(date);
  const year = parts.find((part) => part.type === "year")?.value;
  const month = parts.find((part) => part.type === "month")?.value;
  const day = parts.find((part) => part.type === "day")?.value;

  if (!year || !month || !day) {
    throw new Error("Unable to format ISO date parts.");
  }

  return `${year}-${month}-${day}`;
}

export function toLocalIsoDate(date = new Date()) {
  return toIsoDateInTimeZone(date);
}

function parseIsoDate(isoDate) {
  const [year, month, day] = isoDate.split("-").map(Number);

  if (!year || !month || !day) {
    throw new TypeError(`Expected YYYY-MM-DD date string, got "${isoDate}".`);
  }

  return new Date(Date.UTC(year, month - 1, day));
}

function parseDisplayDate(isoDate) {
  const [year, month, day] = isoDate.split("-").map(Number);

  if (!year || !month || !day) {
    throw new TypeError(`Expected YYYY-MM-DD date string, got "${isoDate}".`);
  }

  return new Date(year, month - 1, day, 12, 0, 0);
}

function formatUtcIsoDate(date) {
  return `${date.getUTCFullYear()}-${String(date.getUTCMonth() + 1).padStart(2, "0")}-${String(
    date.getUTCDate()
  ).padStart(2, "0")}`;
}

function shiftIsoDate(isoDate, deltaDays) {
  const date = parseIsoDate(isoDate);
  date.setUTCDate(date.getUTCDate() + deltaDays);
  return formatUtcIsoDate(date);
}

function dayDistance(referenceDate, targetDate) {
  return Math.round((parseIsoDate(referenceDate).getTime() - parseIsoDate(targetDate).getTime()) / DAY_IN_MS);
}

function formatWeekday(isoDate, locale) {
  return new Intl.DateTimeFormat(locale, { weekday: "short" }).format(parseDisplayDate(isoDate));
}

function formatMonthDay(isoDate, locale) {
  return new Intl.DateTimeFormat(locale, {
    month: locale === "zh-CN" ? "numeric" : "short",
    day: "numeric"
  }).format(parseDisplayDate(isoDate));
}

function formatChineseAbsoluteDate(isoDate, includeYear) {
  const [year, month, day] = isoDate.split("-").map(Number);

  if (includeYear) {
    return `${year}年${month}月${day}日`;
  }

  return `${month}月${day}日`;
}

export function getDefaultSelectedDate({ availableDates, todayDate }) {
  if (!Array.isArray(availableDates) || availableDates.length === 0) {
    return todayDate ?? "";
  }

  const sortedDates = [...availableDates].sort();
  const yesterdayDate = todayDate ? shiftIsoDate(todayDate, -1) : null;

  if (yesterdayDate && sortedDates.includes(yesterdayDate)) {
    return yesterdayDate;
  }

  return sortedDates[sortedDates.length - 1];
}

export function buildWeeklyBurnCopy({ date, todayDate, locale }) {
  const daysAgo = dayDistance(todayDate, date);
  const useAbsoluteDate = daysAgo > 2 || daysAgo < 0;
  const sameYear = todayDate.slice(0, 4) === date.slice(0, 4);
  const metaDate = useAbsoluteDate
    ? formatWeekday(date, locale)
    : `${formatWeekday(date, locale)} ${new Intl.DateTimeFormat(locale, {
        month: "numeric",
        day: "numeric"
      }).format(parseDisplayDate(date))}`;

  if (locale === "zh-CN") {
    if (daysAgo === 0) {
      return { title: "今天消耗", metaDate };
    }

    if (daysAgo === 1) {
      return { title: "昨天消耗", metaDate };
    }

    if (daysAgo === 2) {
      return { title: "前天消耗", metaDate };
    }

    return {
      title: `${formatChineseAbsoluteDate(date, !sameYear)}消耗`,
      metaDate
    };
  }

  if (daysAgo === 0) {
    return { title: "Today's Burn", metaDate };
  }

  if (daysAgo === 1) {
    return { title: "Yesterday's Burn", metaDate };
  }

  if (daysAgo === 2) {
    return { title: "Burn 2 Days Ago", metaDate };
  }

  return {
    title: `Burn on ${formatMonthDay(date, locale)}${sameYear ? "" : `, ${date.slice(0, 4)}`}`,
    metaDate
  };
}

export function resolveSelectedDateAfterRefresh({
  currentDate,
  previousDefaultDate,
  nextDefaultDate,
  availableDates,
}) {
  if (!nextDefaultDate) {
    return currentDate;
  }

  if (!currentDate || !availableDates.includes(currentDate)) {
    return nextDefaultDate;
  }

  if (
    previousDefaultDate &&
    currentDate === previousDefaultDate &&
    nextDefaultDate !== previousDefaultDate
  ) {
    return nextDefaultDate;
  }

  return currentDate;
}
