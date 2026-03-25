import fs from "node:fs";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const RELEASE_TYPES = ["patch", "minor", "major"];
const DEFAULT_REMOTE = "origin";
const FALLBACK_CLAUDE_TIMEOUT_MS = 120_000;
const MAX_PROMPT_DIFF_CHARS = 24_000;

export function parseReleaseType(rawValue) {
  if (RELEASE_TYPES.includes(rawValue)) {
    return rawValue;
  }

  throw new Error(`Expected one of patch, minor, major. Received: ${rawValue ?? "(missing)"}`);
}

export function resolveReleaseType(rawValue) {
  return parseReleaseType(rawValue ?? "patch");
}

function parseVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version ?? "");
  if (!match) {
    throw new Error(`Unsupported version format: ${version ?? "(missing)"}`);
  }

  return match.slice(1).map((part) => Number.parseInt(part, 10));
}

function compareVersions(leftVersion, rightVersion) {
  const left = parseVersion(leftVersion);
  const right = parseVersion(rightVersion);

  for (let index = 0; index < left.length; index += 1) {
    if (left[index] > right[index]) {
      return 1;
    }

    if (left[index] < right[index]) {
      return -1;
    }
  }

  return 0;
}

export function selectReleaseBaseVersion(localVersion, publishedVersion) {
  if (!publishedVersion) {
    return localVersion;
  }

  return compareVersions(localVersion, publishedVersion) >= 0 ? localVersion : publishedVersion;
}

export function bumpVersion(version, releaseType) {
  const [major, minor, patch] = parseVersion(version);

  if (releaseType === "patch") {
    return `${major}.${minor}.${patch + 1}`;
  }

  if (releaseType === "minor") {
    return `${major}.${minor + 1}.0`;
  }

  return `${major + 1}.0.0`;
}

export function buildPushCommandArgs({
  branch,
  hasUpstream,
  remote = DEFAULT_REMOTE
}) {
  if (hasUpstream) {
    return ["push", remote, branch];
  }

  return ["push", "-u", remote, branch];
}

export function buildStageAllArgs() {
  return ["add", "-A"];
}

export function fallbackReleaseCommitMessage(version) {
  return `chore: release v${version}`;
}

function sanitizeCommitMessage(message) {
  if (!message) {
    return "";
  }

  return (
    message
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find(Boolean) ?? ""
  );
}

export function resolveCommitMessage({ claudeMessage, version }) {
  const sanitized = sanitizeCommitMessage(claudeMessage);
  return sanitized || fallbackReleaseCommitMessage(version);
}

export function buildClaudeCommitPrompt(version, diffSummary = "") {
  return [
    "Write a git commit message for the staged git diff in this repository.",
    `The release version is ${version}.`,
    "Return exactly one single line in Conventional Commit format.",
    "Do not use quotes, bullets, code fences, or explanation.",
    "Prefer a concise message that reflects the staged code changes, not generic release commentary.",
    diffSummary ? `Staged git diff:\n${diffSummary}` : ""
  ]
    .filter(Boolean)
    .join("\n\n");
}

function runCommand(command, args, { cwd, env = process.env, stdio = "inherit" } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      env,
      stdio,
      shell: process.platform === "win32"
    });

    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`${command} exited with signal ${signal}`));
        return;
      }

      if (code !== 0) {
        reject(new Error(`${command} exited with code ${code ?? "unknown"}`));
        return;
      }

      resolve();
    });
  });
}

function captureCommand(command, args, { cwd, env = process.env, allowFailure = false } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    env,
    encoding: "utf8",
    shell: process.platform === "win32"
  });

  if (result.error) {
    throw result.error;
  }

  if (!allowFailure && result.status !== 0) {
    const stderr = result.stderr?.trim();
    throw new Error(stderr || `${command} exited with code ${result.status ?? "unknown"}`);
  }

  return {
    status: result.status ?? 0,
    stdout: result.stdout?.trim() ?? "",
    stderr: result.stderr?.trim() ?? ""
  };
}

function readPackageJson(rootDir) {
  const packageJsonPath = path.join(rootDir, "package.json");
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  return { packageJsonPath, packageJson };
}

function writePackageJson(packageJsonPath, packageJson) {
  fs.writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

function resolveCurrentBranch(rootDir) {
  const { stdout } = captureCommand("git", ["branch", "--show-current"], { cwd: rootDir });
  if (!stdout) {
    throw new Error("Release must run from a named git branch.");
  }

  return stdout;
}

function hasUpstream(rootDir) {
  const result = captureCommand(
    "git",
    ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
    { cwd: rootDir, allowFailure: true }
  );

  return result.status === 0;
}

function ensureTagIsAvailable(rootDir, tagName) {
  const result = captureCommand(
    "git",
    ["rev-parse", "-q", "--verify", `refs/tags/${tagName}`],
    { cwd: rootDir, allowFailure: true }
  );

  if (result.status === 0) {
    throw new Error(`Git tag ${tagName} already exists.`);
  }
}

async function resolvePublishedVersion(packageName, rootDir) {
  const result = captureCommand("npm", ["view", packageName, "version"], {
    cwd: rootDir,
    allowFailure: true
  });

  if (result.status !== 0 || !result.stdout) {
    return null;
  }

  return result.stdout.split(/\s+/).pop() ?? null;
}

function truncateForPrompt(text, maxChars = MAX_PROMPT_DIFF_CHARS) {
  if (text.length <= maxChars) {
    return text;
  }

  return `${text.slice(0, maxChars)}\n\n[diff truncated]`;
}

function readStagedDiffSummary(rootDir) {
  const stat = captureCommand("git", ["diff", "--cached", "--stat", "--no-ext-diff"], {
    cwd: rootDir
  }).stdout;
  const diff = captureCommand("git", ["diff", "--cached", "--no-ext-diff", "--unified=0"], {
    cwd: rootDir
  }).stdout;

  return truncateForPrompt([stat, diff].filter(Boolean).join("\n\n"));
}

function tryGenerateClaudeCommitMessage(rootDir, version, env) {
  const prompt = buildClaudeCommitPrompt(version, readStagedDiffSummary(rootDir));

  try {
    const result = spawnSync("claude", ["-p", prompt], {
      cwd: rootDir,
      env,
      encoding: "utf8",
      timeout: FALLBACK_CLAUDE_TIMEOUT_MS,
      maxBuffer: 4 * 1024 * 1024
    });

    if (result.error || result.status !== 0) {
      return null;
    }

    return sanitizeCommitMessage(result.stdout);
  } catch {
    return null;
  }
}

export async function runRelease({
  rootDir,
  argv = process.argv.slice(2),
  env = process.env
}) {
  const releaseType = resolveReleaseType(argv[0]);

  const branch = resolveCurrentBranch(rootDir);
  const upstreamPresent = hasUpstream(rootDir);
  const { packageJsonPath, packageJson } = readPackageJson(rootDir);
  const publishedVersion = await resolvePublishedVersion(packageJson.name, rootDir);
  const baseVersion = selectReleaseBaseVersion(packageJson.version, publishedVersion);
  const nextVersion = bumpVersion(baseVersion, releaseType);
  const tagName = `v${nextVersion}`;

  ensureTagIsAvailable(rootDir, tagName);

  console.log(`Preparing ${packageJson.name}@${nextVersion} from ${branch}`);
  if (publishedVersion && publishedVersion !== packageJson.version) {
    console.log(`npm latest is ${publishedVersion}; release will bump from ${baseVersion}`);
  }

  await runCommand("pnpm", ["test"], { cwd: rootDir, env });
  packageJson.version = nextVersion;
  writePackageJson(packageJsonPath, packageJson);
  await runCommand("git", buildStageAllArgs(), { cwd: rootDir, env });

  const commitMessage = resolveCommitMessage({
    claudeMessage: tryGenerateClaudeCommitMessage(rootDir, nextVersion, env),
    version: nextVersion
  });

  if (commitMessage === fallbackReleaseCommitMessage(nextVersion)) {
    console.log(`Using fallback commit message: ${commitMessage}`);
  } else {
    console.log(`Using Claude-generated commit message: ${commitMessage}`);
  }

  await runCommand("git", ["commit", "-m", commitMessage], { cwd: rootDir, env });
  await runCommand("git", ["tag", tagName], { cwd: rootDir, env });
  await runCommand("npm", ["publish"], { cwd: rootDir, env });
  await runCommand("git", buildPushCommandArgs({ branch, hasUpstream: upstreamPresent }), {
    cwd: rootDir,
    env
  });
  await runCommand("git", ["push", DEFAULT_REMOTE, tagName], { cwd: rootDir, env });
}

const currentFilePath = fileURLToPath(import.meta.url);
const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : "";

if (currentFilePath === invokedPath) {
  const rootDir = path.resolve(path.dirname(currentFilePath), "..");

  runRelease({ rootDir }).catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exit(1);
  });
}
