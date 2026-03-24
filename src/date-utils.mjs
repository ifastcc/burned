const formatterCache = new Map();

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
