import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.join(dirname, "..");
const releaseScriptPath = path.join(dirname, "release.mjs");
const legacyReleaseShellPath = path.join(dirname, "release.sh");
const releaseShellPath = path.join(rootDir, "release.sh");
const burnedControlPath = path.join(rootDir, "burned.sh");

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

test("runRelease stops before mutating package.json when npm auth is invalid", async () => {
  const { runRelease } = await loadReleaseModule();
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "burned-release-auth-"));
  const packageJsonPath = path.join(tempRoot, "package.json");
  const initialPackageJson = `${JSON.stringify({ name: "burned", version: "0.2.4" }, null, 2)}\n`;
  const commandLog = [];

  fs.writeFileSync(packageJsonPath, initialPackageJson);

  const captureCommandImpl = (command, args) => {
    commandLog.push(["capture", command, args]);

    if (command === "git" && args[0] === "branch") {
      return { status: 0, stdout: "main", stderr: "" };
    }

    if (command === "git" && args[0] === "rev-parse" && args.includes("@{upstream}")) {
      return { status: 0, stdout: "origin/main", stderr: "" };
    }

    if (command === "npm" && args[0] === "view") {
      return { status: 0, stdout: "0.2.3", stderr: "" };
    }

    if (command === "git" && args[0] === "rev-parse" && args.at(-1) === "refs/tags/v0.2.5") {
      return { status: 1, stdout: "", stderr: "" };
    }

    if (command === "npm" && args[0] === "whoami") {
      return { status: 1, stdout: "", stderr: "npm error code E401" };
    }

    throw new Error(`Unexpected command: ${command} ${args.join(" ")}`);
  };

  const runCommandImpl = async () => {
    throw new Error("runCommand should not be called when npm auth preflight fails");
  };

  await assert.rejects(
    runRelease({
      rootDir: tempRoot,
      argv: ["patch"],
      captureCommandImpl,
      runCommandImpl,
      log: () => {}
    }),
    /npm authentication failed.*npm login/i
  );

  assert.equal(fs.readFileSync(packageJsonPath, "utf8"), initialPackageJson);
  assert.equal(
    commandLog.some(([kind, command]) => kind === "capture" && command === "npm"),
    true
  );
});

test("runRelease stops before mutating package.json when the npm user does not own the package", async () => {
  const { runRelease } = await loadReleaseModule();
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "burned-release-owner-"));
  const packageJsonPath = path.join(tempRoot, "package.json");
  const initialPackageJson = `${JSON.stringify({ name: "burned", version: "0.2.4" }, null, 2)}\n`;

  fs.writeFileSync(packageJsonPath, initialPackageJson);

  const captureCommandImpl = (command, args) => {
    if (command === "git" && args[0] === "branch") {
      return { status: 0, stdout: "main", stderr: "" };
    }

    if (command === "git" && args[0] === "rev-parse" && args.includes("@{upstream}")) {
      return { status: 0, stdout: "origin/main", stderr: "" };
    }

    if (command === "npm" && args[0] === "view") {
      return { status: 0, stdout: "0.2.3", stderr: "" };
    }

    if (command === "git" && args[0] === "rev-parse" && args.at(-1) === "refs/tags/v0.2.5") {
      return { status: 1, stdout: "", stderr: "" };
    }

    if (command === "npm" && args[0] === "whoami") {
      return { status: 0, stdout: "kbaicai", stderr: "" };
    }

    if (command === "npm" && args[0] === "owner" && args[1] === "ls") {
      return { status: 0, stdout: "ifastcc <ifastcc2025@gmail.com>", stderr: "" };
    }

    throw new Error(`Unexpected command: ${command} ${args.join(" ")}`);
  };

  const runCommandImpl = async () => {
    throw new Error("runCommand should not be called when owner preflight fails");
  };

  await assert.rejects(
    runRelease({
      rootDir: tempRoot,
      argv: ["patch"],
      captureCommandImpl,
      runCommandImpl,
      log: () => {}
    }),
    /not listed as an owner/i
  );

  assert.equal(fs.readFileSync(packageJsonPath, "utf8"), initialPackageJson);
});

test("describeNpmPublishFailure turns registry 404s into a publish-permission hint", async () => {
  const { describeNpmPublishFailure } = await loadReleaseModule();

  const error = describeNpmPublishFailure({
    packageName: "burned",
    publishedVersion: "0.2.3",
    npmUser: "kbaicai",
    packageOwners: ["ifastcc"],
    stderr: [
      "npm error code E404",
      "npm error 404 Not Found - PUT https://registry.npmjs.org/burned - Not found"
    ].join("\n")
  });

  assert.match(error.message, /cannot publish the existing npm package/i);
  assert.match(error.message, /Current npm user: kbaicai/);
  assert.match(error.message, /Known owners: ifastcc/);
});

test("release.sh exists as an executable wrapper around the node release flow", () => {
  assert.equal(fs.existsSync(legacyReleaseShellPath), false);
  assert.equal(fs.existsSync(releaseShellPath), true);

  const shellSource = fs.readFileSync(releaseShellPath, "utf8");
  const mode = fs.statSync(releaseShellPath).mode;

  assert.match(shellSource, /^#!\/usr\/bin\/env sh/m);
  assert.match(shellSource, /exec node "\$ROOT_DIR\/scripts\/release\.mjs" "\$@"/);
  assert.doesNotMatch(shellSource, /Usage: \.\/scripts\/release\.sh/);
  assert.notEqual(mode & 0o111, 0);
});

test("burned.sh exists at the project root as a quick control wrapper", () => {
  assert.equal(fs.existsSync(burnedControlPath), true);

  const shellSource = fs.readFileSync(burnedControlPath, "utf8");
  const mode = fs.statSync(burnedControlPath).mode;

  assert.match(shellSource, /^#!\/usr\/bin\/env sh/m);
  assert.match(shellSource, /case "\$\{1:-restart\}" in/);
  assert.match(shellSource, /start\)/);
  assert.match(shellSource, /stop\)/);
  assert.match(shellSource, /restart\)/);
  assert.match(shellSource, /status\)/);
  assert.match(shellSource, /"\$ROOT_DIR\/burned"/);
  assert.notEqual(mode & 0o111, 0);
});

test("burned.sh keeps tracking the spawned burned-web service after the launcher exits", () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "burned-control-"));
  const tempControlPath = path.join(tempRoot, "burned.sh");
  const tempLauncherPath = path.join(tempRoot, "burned");
  const tempServicePath = path.join(tempRoot, "burned-web");
  const pidFilePath = path.join(tempRoot, ".burned.pid");
  const logFilePath = path.join(tempRoot, ".burned.log");

  fs.copyFileSync(burnedControlPath, tempControlPath);
  fs.writeFileSync(
    tempLauncherPath,
    `#!/usr/bin/env sh
set -eu
ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
"$ROOT_DIR/burned-web" &
echo "Burned dashboard is running at http://127.0.0.1:47831/"
sleep 2
`,
  );
  fs.writeFileSync(
    tempServicePath,
    `#!/usr/bin/env sh
trap 'exit 0' TERM INT
while :; do
  sleep 1
done
`,
  );

  fs.chmodSync(tempControlPath, 0o755);
  fs.chmodSync(tempLauncherPath, 0o755);
  fs.chmodSync(tempServicePath, 0o755);

  let trackedPid = null;

  try {
    const startOutput = execFileSync(tempControlPath, ["start"], {
      cwd: tempRoot,
      encoding: "utf8",
    });

    assert.match(startOutput, /Burned started \(PID \d+\)\./);
    assert.equal(fs.existsSync(pidFilePath), true);

    trackedPid = fs.readFileSync(pidFilePath, "utf8").trim();
    assert.match(trackedPid, /^\d+$/);

    const trackedCommand = execFileSync("ps", ["-p", trackedPid, "-o", "command="], {
      encoding: "utf8",
    }).trim();
    assert.match(trackedCommand, /burned-web/);

    execFileSync("sleep", ["3"]);

    const statusOutput = execFileSync(tempControlPath, ["status"], {
      cwd: tempRoot,
      encoding: "utf8",
    });
    assert.match(statusOutput, new RegExp(`Burned is running \\(PID ${trackedPid}\\)\\.`));

    const logSource = fs.readFileSync(logFilePath, "utf8");
    assert.equal(logSource.length >= 0, true);
  } finally {
    try {
      execFileSync(tempControlPath, ["stop"], {
        cwd: tempRoot,
        encoding: "utf8",
      });
    } catch {}

    if (trackedPid) {
      try {
        execFileSync("kill", ["-KILL", trackedPid], { stdio: "ignore" });
      } catch {}
    }
  }
});
