import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";

export function resolveCargoTargetDir({
  env = process.env,
  homeDir = os.homedir()
} = {}) {
  return env.BURNED_CARGO_TARGET_DIR || path.join(homeDir, ".burned", "cargo-target");
}

export function resolveBurnedBinaryPath(targetDir, platform = process.platform) {
  const fileName = platform === "win32" ? "burned-web.exe" : "burned-web";
  return path.join(targetDir, "release", fileName);
}

function latestModifiedMs(targetPath) {
  if (!fs.existsSync(targetPath)) {
    return 0;
  }

  const stat = fs.statSync(targetPath);
  if (!stat.isDirectory()) {
    return stat.mtimeMs;
  }

  return fs.readdirSync(targetPath).reduce((latest, entry) => {
    return Math.max(latest, latestModifiedMs(path.join(targetPath, entry)));
  }, stat.mtimeMs);
}

export function needsRustRebuild(binaryPath, watchedPaths) {
  if (!fs.existsSync(binaryPath)) {
    return true;
  }

  const binaryModifiedMs = fs.statSync(binaryPath).mtimeMs;
  const latestSourceMs = watchedPaths.reduce((latest, watchedPath) => {
    return Math.max(latest, latestModifiedMs(watchedPath));
  }, 0);

  return latestSourceMs > binaryModifiedMs;
}

function rustWatchPaths(rootDir) {
  return [
    path.join(rootDir, "src-tauri", "Cargo.toml"),
    path.join(rootDir, "src-tauri", "Cargo.lock"),
    path.join(rootDir, "src-tauri", "build.rs"),
    path.join(rootDir, "src-tauri", "src"),
    path.join(rootDir, "src-tauri", "capabilities"),
    path.join(rootDir, "src-tauri", "tauri.conf.json")
  ];
}

function spawnCommand(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: "inherit",
      shell: process.platform === "win32",
      ...options
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

async function ensureFrontendDist(rootDir) {
  const indexHtml = path.join(rootDir, "dist", "index.html");
  if (fs.existsSync(indexHtml)) {
    return;
  }

  await spawnCommand("pnpm", ["build"], { cwd: rootDir });
}

async function ensureRustBinary(rootDir, targetDir) {
  const binaryPath = resolveBurnedBinaryPath(targetDir);
  if (!needsRustRebuild(binaryPath, rustWatchPaths(rootDir))) {
    return binaryPath;
  }

  await spawnCommand(
    "cargo",
    [
      "build",
      "--release",
      "--manifest-path",
      path.join(rootDir, "src-tauri", "Cargo.toml"),
      "--bin",
      "burned-web"
    ],
    {
      cwd: rootDir,
      env: {
        ...process.env,
        CARGO_TARGET_DIR: targetDir
      }
    }
  );

  return binaryPath;
}

export async function runBurnedCli({
  rootDir,
  argv = process.argv.slice(2)
}) {
  const targetDir = resolveCargoTargetDir();

  try {
    await ensureFrontendDist(rootDir);
    const binaryPath = await ensureRustBinary(rootDir, targetDir);

    const child = spawn(binaryPath, argv, {
      cwd: rootDir,
      stdio: "inherit",
      env: {
        ...process.env,
        CARGO_TARGET_DIR: targetDir
      }
    });

    child.on("exit", (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
        return;
      }

      process.exit(code ?? 0);
    });
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("spawn cargo")) {
      console.error("Burned requires a Rust toolchain on PATH the first time it runs.");
    }

    throw error;
  }
}
