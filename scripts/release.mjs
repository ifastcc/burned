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
    const captureOutput = stdio === "capture";
    let stdout = "";
    let stderr = "";
    const child = spawn(command, args, {
      cwd,
      env,
      stdio: captureOutput ? ["inherit", "pipe", "pipe"] : stdio,
      shell: process.platform === "win32"
    });

    if (captureOutput) {
      child.stdout?.on("data", (chunk) => {
        stdout += chunk.toString();
        process.stdout.write(chunk);
      });

      child.stderr?.on("data", (chunk) => {
        stderr += chunk.toString();
        process.stderr.write(chunk);
      });
    }

    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`${command} exited with signal ${signal}`));
        return;
      }

      if (code !== 0) {
        const details = [stderr.trim(), stdout.trim()].filter(Boolean).join("\n");
        const error = new Error(details || `${command} exited with code ${code ?? "unknown"}`);
        error.stdout = stdout.trim();
        error.stderr = stderr.trim();
        error.exitCode = code ?? null;
        reject(error);
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

function resolveCurrentBranch(rootDir, captureCommandImpl = captureCommand) {
  const { stdout } = captureCommandImpl("git", ["branch", "--show-current"], { cwd: rootDir });
  if (!stdout) {
    throw new Error("Release must run from a named git branch.");
  }

  return stdout;
}

function hasUpstream(rootDir, captureCommandImpl = captureCommand) {
  const result = captureCommandImpl(
    "git",
    ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
    { cwd: rootDir, allowFailure: true }
  );

  return result.status === 0;
}

function ensureTagIsAvailable(rootDir, tagName, captureCommandImpl = captureCommand) {
  const result = captureCommandImpl(
    "git",
    ["rev-parse", "-q", "--verify", `refs/tags/${tagName}`],
    { cwd: rootDir, allowFailure: true }
  );

  if (result.status === 0) {
    throw new Error(`Git tag ${tagName} already exists.`);
  }
}

async function resolvePublishedVersion(packageName, rootDir, captureCommandImpl = captureCommand) {
  const result = captureCommandImpl("npm", ["view", packageName, "version"], {
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

function readStagedDiffSummary(rootDir, captureCommandImpl = captureCommand) {
  const stat = captureCommandImpl("git", ["diff", "--cached", "--stat", "--no-ext-diff"], {
    cwd: rootDir
  }).stdout;
  const diff = captureCommandImpl("git", ["diff", "--cached", "--no-ext-diff", "--unified=0"], {
    cwd: rootDir
  }).stdout;

  return truncateForPrompt([stat, diff].filter(Boolean).join("\n\n"));
}

function tryGenerateClaudeCommitMessage(rootDir, version, env, captureCommandImpl = captureCommand) {
  const prompt = buildClaudeCommitPrompt(version, readStagedDiffSummary(rootDir, captureCommandImpl));

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

export function parseNpmOwnerUsernames(ownerListing = "") {
  return ownerListing
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => line.split(/\s+/)[0]);
}

export function ensureNpmPublishPreflight({
  packageName,
  publishedVersion,
  rootDir,
  captureCommandImpl = captureCommand
}) {
  const whoamiResult = captureCommandImpl("npm", ["whoami"], {
    cwd: rootDir,
    allowFailure: true
  });

  if (whoamiResult.status !== 0 || !whoamiResult.stdout) {
    throw new Error(
      `npm authentication failed for ${packageName}. Run 'npm login' or refresh the token in ~/.npmrc before releasing.`
    );
  }

  const npmUser = whoamiResult.stdout.split(/\s+/)[0];

  if (!publishedVersion) {
    return { npmUser, packageOwners: [] };
  }

  const ownerResult = captureCommandImpl("npm", ["owner", "ls", packageName], {
    cwd: rootDir,
    allowFailure: true
  });

  if (ownerResult.status !== 0 || !ownerResult.stdout) {
    throw new Error(
      `Unable to verify npm ownership for ${packageName}. Run 'npm owner ls ${packageName}' and confirm that ${npmUser} can publish it before releasing.`
    );
  }

  const packageOwners = parseNpmOwnerUsernames(ownerResult.stdout);

  if (!packageOwners.includes(npmUser)) {
    throw new Error(
      `npm user ${npmUser} is not listed as an owner for ${packageName}. Known owners: ${packageOwners.join(", ")}. Publish with an authorized npm account or add this user as an owner first.`
    );
  }

  return { npmUser, packageOwners };
}

export function describeNpmPublishFailure({
  packageName,
  publishedVersion,
  npmUser,
  packageOwners = [],
  stderr = "",
  message = ""
}) {
  const combined = [message, stderr].filter(Boolean).join("\n");

  if (/E401|401 Unauthorized/i.test(combined)) {
    return new Error(
      `npm publish failed for ${packageName}: registry authentication was rejected. Run 'npm login' or refresh the token in ~/.npmrc, then retry.`
    );
  }

  if (publishedVersion && /E403|403 Forbidden|E404|404 Not Found/i.test(combined)) {
    const ownerSummary = packageOwners.length > 0 ? packageOwners.join(", ") : "(unknown)";
    return new Error(
      `npm publish failed for ${packageName}: the current account cannot publish the existing npm package. Current npm user: ${npmUser ?? "(unknown)"}. Known owners: ${ownerSummary}.`
    );
  }

  return new Error(combined || `npm publish failed for ${packageName}.`);
}

export async function runRelease({
  rootDir,
  argv = process.argv.slice(2),
  env = process.env,
  captureCommandImpl = captureCommand,
  runCommandImpl = runCommand,
  log = console.log
}) {
  const releaseType = resolveReleaseType(argv[0]);

  const branch = resolveCurrentBranch(rootDir, captureCommandImpl);
  const upstreamPresent = hasUpstream(rootDir, captureCommandImpl);
  const { packageJsonPath, packageJson } = readPackageJson(rootDir);
  const publishedVersion = await resolvePublishedVersion(
    packageJson.name,
    rootDir,
    captureCommandImpl
  );
  const { npmUser, packageOwners } = ensureNpmPublishPreflight({
    packageName: packageJson.name,
    publishedVersion,
    rootDir,
    captureCommandImpl
  });
  const baseVersion = selectReleaseBaseVersion(packageJson.version, publishedVersion);
  const nextVersion = bumpVersion(baseVersion, releaseType);
  const tagName = `v${nextVersion}`;

  ensureTagIsAvailable(rootDir, tagName, captureCommandImpl);

  log(`Preparing ${packageJson.name}@${nextVersion} from ${branch}`);
  if (publishedVersion && publishedVersion !== packageJson.version) {
    log(`npm latest is ${publishedVersion}; release will bump from ${baseVersion}`);
  }

  await runCommandImpl("pnpm", ["test"], { cwd: rootDir, env });
  packageJson.version = nextVersion;
  writePackageJson(packageJsonPath, packageJson);
  await runCommandImpl("git", buildStageAllArgs(), { cwd: rootDir, env });

  const commitMessage = resolveCommitMessage({
    claudeMessage: tryGenerateClaudeCommitMessage(rootDir, nextVersion, env, captureCommandImpl),
    version: nextVersion
  });

  if (commitMessage === fallbackReleaseCommitMessage(nextVersion)) {
    log(`Using fallback commit message: ${commitMessage}`);
  } else {
    log(`Using Claude-generated commit message: ${commitMessage}`);
  }

  await runCommandImpl("git", ["commit", "-m", commitMessage], { cwd: rootDir, env });
  await runCommandImpl("git", ["tag", tagName], { cwd: rootDir, env });

  try {
    await runCommandImpl("npm", ["publish"], { cwd: rootDir, env, stdio: "capture" });
  } catch (error) {
    throw describeNpmPublishFailure({
      packageName: packageJson.name,
      publishedVersion,
      npmUser,
      packageOwners,
      stderr: error?.stderr ?? "",
      message: error instanceof Error ? error.message : String(error)
    });
  }

  await runCommandImpl("git", buildPushCommandArgs({ branch, hasUpstream: upstreamPresent }), {
    cwd: rootDir,
    env
  });
  await runCommandImpl("git", ["push", DEFAULT_REMOTE, tagName], { cwd: rootDir, env });
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
