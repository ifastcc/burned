function isSubagentSession(session) {
  return session?.sessionRole === "subagent" && typeof session.parentSessionId === "string";
}

function resolveRootSession(session, sessionsById) {
  let current = session;
  const seen = new Set([session.id]);

  while (isSubagentSession(current)) {
    const parentSession = sessionsById.get(current.parentSessionId);
    if (!parentSession || seen.has(parentSession.id)) {
      return current;
    }
    current = parentSession;
    seen.add(current.id);
  }

  return current;
}

function derivePricingCoverage(sessions) {
  if (sessions.every((session) => session.pricingCoverage === "actual")) {
    return "actual";
  }

  if (sessions.every((session) => session.pricingCoverage === "pending")) {
    return "pending";
  }

  return "partial";
}

function buildDisplaySession(session, children) {
  if (children.length === 0) {
    return session;
  }

  const sessions = [session, ...children];
  const pricingCoverage = derivePricingCoverage(sessions);

  return {
    ...session,
    totalTokens: sessions.reduce((sum, item) => sum + item.totalTokens, 0),
    costUsd: sessions.reduce((sum, item) => sum + item.costUsd, 0),
    pricedSessions: sessions.reduce((sum, item) => sum + item.pricedSessions, 0),
    pendingPricingSessions: sessions.reduce(
      (sum, item) => sum + item.pendingPricingSessions,
      0,
    ),
    pricingCoverage,
    pricingState: pricingCoverage === "actual" ? "actual" : "pending",
  };
}

export function shouldShowSourceLabel({ nested = false, sourceScoped = false } = {}) {
  return !nested && !sourceScoped;
}

export function buildSessionThreadGroups(sessions) {
  const allSessions = Array.isArray(sessions) ? sessions : [];
  const sessionsById = new Map(allSessions.map((session) => [session.id, session]));
  const groups = [];
  const groupsByRootId = new Map();

  for (const session of allSessions) {
    const rootSession = resolveRootSession(session, sessionsById);
    let group = groupsByRootId.get(rootSession.id);

    if (!group) {
      group = {
        id: rootSession.id,
        session: rootSession,
        children: [],
        displaySession: rootSession,
      };
      groupsByRootId.set(rootSession.id, group);
      groups.push(group);
    }

    if (session.id !== rootSession.id) {
      group.children.push(session);
    }
  }

  return groups.map((group) => ({
    ...group,
    displaySession: buildDisplaySession(group.session, group.children),
  }));
}

export function sortSessionThreadGroups(groups, sort = "recent") {
  if (sort === "recent") {
    return groups;
  }

  const sorted = [...groups];
  if (sort === "tokens") {
    sorted.sort(
      (left, right) =>
        right.displaySession.totalTokens - left.displaySession.totalTokens ||
        right.displaySession.costUsd - left.displaySession.costUsd ||
        left.displaySession.title.localeCompare(right.displaySession.title),
    );
    return sorted;
  }

  sorted.sort(
    (left, right) =>
      right.displaySession.costUsd - left.displaySession.costUsd ||
      right.displaySession.totalTokens - left.displaySession.totalTokens ||
      left.displaySession.title.localeCompare(right.displaySession.title),
  );
  return sorted;
}
