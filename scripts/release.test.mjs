import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));
const releaseScriptPath = path.join(dirname, "release.mjs");
const releaseShellPath = path.join(dirname, "release.sh");

async function loadReleaseModule() {
  assert.equal(fs.existsSync(releaseScriptPath), true);
  return import(pathToFileURL(releaseScriptPath).href);
}

test("bumpVersion increments patch, minor, and major releases", async () => {
  const { bumpVersion } = await loadReleaseModule();

  assert.equal(bumpVersion("0.2.2", "patch"), "0.2.3");
  assert.equal(bumpVersion("0.2.2", "minor"), "0.3.0");
  assert.equal(bumpVersion("0.2.2", "major"), "1.0.0");
});

test("selectReleaseBaseVersion never bumps from a version older than npm", async () => {
  const { selectReleaseBaseVersion } = await loadReleaseModule();

  assert.equal(selectReleaseBaseVersion("0.2.2", "0.2.1"), "0.2.2");
  assert.equal(selectReleaseBaseVersion("0.2.2", "0.2.9"), "0.2.9");
  assert.equal(selectReleaseBaseVersion("0.2.2", null), "0.2.2");
});

test("parseReleaseType rejects unsupported release kinds", async () => {
  const { parseReleaseType } = await loadReleaseModule();

  assert.equal(parseReleaseType("patch"), "patch");
  assert.throws(() => parseReleaseType("beta"), /Expected one of patch, minor, major/);
});

test("resolveReleaseType defaults missing input to patch", async () => {
  const { resolveReleaseType } = await loadReleaseModule();

  assert.equal(resolveReleaseType(undefined), "patch");
  assert.equal(resolveReleaseType("minor"), "minor");
});

test("buildPushCommandArgs adds upstream wiring only when needed", async () => {
  const { buildPushCommandArgs } = await loadReleaseModule();

  assert.deepEqual(buildPushCommandArgs({ branch: "main", hasUpstream: true }), [
    "push",
    "origin",
    "main"
  ]);
  assert.deepEqual(buildPushCommandArgs({ branch: "codex/release", hasUpstream: false }), [
    "push",
    "-u",
    "origin",
    "codex/release"
  ]);
});

test("buildStageAllArgs stages the whole release snapshot", async () => {
  const { buildStageAllArgs } = await loadReleaseModule();

  assert.deepEqual(buildStageAllArgs(), ["add", "-A"]);
});

test("fallbackReleaseCommitMessage stays deterministic for release commits", async () => {
  const { fallbackReleaseCommitMessage } = await loadReleaseModule();

  assert.equal(fallbackReleaseCommitMessage("0.2.3"), "chore: release v0.2.3");
});

test("resolveCommitMessage prefers claude output and falls back when it is blank", async () => {
  const { resolveCommitMessage } = await loadReleaseModule();

  assert.equal(
    resolveCommitMessage({
      claudeMessage: "fix: streamline release flow\n\nextra detail",
      version: "0.2.3"
    }),
    "fix: streamline release flow"
  );
  assert.equal(
    resolveCommitMessage({
      claudeMessage: "   ",
      version: "0.2.3"
    }),
    "chore: release v0.2.3"
  );
  assert.equal(
    resolveCommitMessage({
      claudeMessage: null,
      version: "0.2.3"
    }),
    "chore: release v0.2.3"
  );
});

test("buildClaudeCommitPrompt asks for one conventional commit line from the staged diff", async () => {
  const { buildClaudeCommitPrompt } = await loadReleaseModule();

  const prompt = buildClaudeCommitPrompt("0.2.3");

  assert.match(prompt, /staged git diff/i);
  assert.match(prompt, /single line/i);
  assert.match(prompt, /Conventional Commit/i);
  assert.match(prompt, /0\.2\.3/);
});

test("release.sh exists as an executable wrapper around the node release flow", () => {
  assert.equal(fs.existsSync(releaseShellPath), true);

  const shellSource = fs.readFileSync(releaseShellPath, "utf8");
  const mode = fs.statSync(releaseShellPath).mode;

  assert.match(shellSource, /^#!\/usr\/bin\/env sh/m);
  assert.match(shellSource, /exec node "\$ROOT_DIR\/scripts\/release\.mjs" "\$@"/);
  assert.doesNotMatch(shellSource, /Usage: \.\/scripts\/release\.sh/);
  assert.notEqual(mode & 0o111, 0);
});
