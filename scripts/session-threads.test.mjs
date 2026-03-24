import assert from "node:assert/strict";
import test from "node:test";

import {
  buildSessionThreadGroups,
  shouldShowSourceLabel,
} from "../src/session-threads.mjs";

function makeSession(overrides = {}) {
  return {
    id: "session",
    sourceId: "codex",
    title: "Session",
    preview: "",
    source: "Codex",
    workspace: "burned",
    model: "gpt-5.4",
    startedAt: "Mar 24 06:29",
    totalTokens: 100,
    costUsd: 1,
    pricedSessions: 1,
    pendingPricingSessions: 0,
    pricingCoverage: "actual",
    pricingState: "actual",
    calculationMethod: "native",
    status: "indexed",
    parentSessionId: null,
    sessionRole: "primary",
    agentLabel: null,
    ...overrides,
  };
}

test("buildSessionThreadGroups nests codex subagent sessions under their parent thread", () => {
  const parent = makeSession({ id: "parent", title: "Parent thread" });
  const childA = makeSession({
    id: "child-a",
    title: "Parent thread",
    parentSessionId: "parent",
    sessionRole: "subagent",
    agentLabel: "Euler",
    totalTokens: 80,
  });
  const childB = makeSession({
    id: "child-b",
    title: "Parent thread",
    parentSessionId: "parent",
    sessionRole: "subagent",
    agentLabel: "Noether",
    totalTokens: 70,
  });

  const groups = buildSessionThreadGroups([childA, parent, childB]);

  assert.equal(groups.length, 1);
  assert.equal(groups[0].session.id, "parent");
  assert.equal(groups[0].children.length, 2);
  assert.equal(groups[0].displaySession.totalTokens, 250);
  assert.equal(groups[0].displaySession.costUsd, 3);
  assert.deepEqual(
    groups[0].children.map((session) => session.id),
    ["child-a", "child-b"],
  );
});

test("buildSessionThreadGroups keeps orphan subagent sessions visible when parent is absent", () => {
  const orphan = makeSession({
    id: "child-a",
    title: "Detached subagent",
    parentSessionId: "missing-parent",
    sessionRole: "subagent",
    agentLabel: "Ptolemy",
  });

  const groups = buildSessionThreadGroups([orphan]);

  assert.equal(groups.length, 1);
  assert.equal(groups[0].session.id, "child-a");
  assert.equal(groups[0].displaySession.id, "child-a");
  assert.equal(groups[0].children.length, 0);
});

test("shouldShowSourceLabel only keeps source labels on top-level mixed-source feeds", () => {
  assert.equal(shouldShowSourceLabel({ nested: false, sourceScoped: false }), true);
  assert.equal(shouldShowSourceLabel({ nested: false, sourceScoped: true }), false);
  assert.equal(shouldShowSourceLabel({ nested: true, sourceScoped: false }), false);
});
