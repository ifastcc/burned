import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  needsRustRebuild,
  resolveBurnedBinaryPath,
  resolveCargoTargetDir
} from "./burned-cli.mjs";

test("resolveCargoTargetDir honors explicit override", () => {
  const targetDir = resolveCargoTargetDir({
    env: { BURNED_CARGO_TARGET_DIR: "/tmp/burned-target" },
    homeDir: "/Users/example"
  });

  assert.equal(targetDir, "/tmp/burned-target");
});

test("resolveCargoTargetDir falls back to a stable home cache", () => {
  const targetDir = resolveCargoTargetDir({
    env: {},
    homeDir: "/Users/example"
  });

  assert.equal(targetDir, "/Users/example/.burned/cargo-target");
});

test("resolveBurnedBinaryPath uses platform-specific executable names", () => {
  assert.equal(
    resolveBurnedBinaryPath("/tmp/burned-target", "darwin"),
    "/tmp/burned-target/release/burned-web"
  );
  assert.equal(
    resolveBurnedBinaryPath("/tmp/burned-target", "win32"),
    "/tmp/burned-target/release/burned-web.exe"
  );
});

test("needsRustRebuild returns true when the binary is missing", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "burned-cli-"));
  const binaryPath = path.join(tmpDir, "burned-web");
  const sourcePaths = [path.join(tmpDir, "src-tauri", "src", "bin", "burned-web.rs")];

  fs.mkdirSync(path.dirname(sourcePaths[0]), { recursive: true });
  fs.writeFileSync(sourcePaths[0], "// source");

  assert.equal(needsRustRebuild(binaryPath, sourcePaths), true);
});

test("needsRustRebuild returns true when a source file is newer than the binary", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "burned-cli-"));
  const binaryPath = path.join(tmpDir, "burned-web");
  const sourcePath = path.join(tmpDir, "src-tauri", "src", "bin", "burned-web.rs");

  fs.mkdirSync(path.dirname(sourcePath), { recursive: true });
  fs.writeFileSync(binaryPath, "binary");
  fs.writeFileSync(sourcePath, "// source");

  const now = Date.now() / 1000;
  fs.utimesSync(binaryPath, now - 120, now - 120);
  fs.utimesSync(sourcePath, now, now);

  assert.equal(needsRustRebuild(binaryPath, [sourcePath]), true);
});

test("needsRustRebuild returns false when the binary is newer than all watched sources", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "burned-cli-"));
  const binaryPath = path.join(tmpDir, "burned-web");
  const sourcePath = path.join(tmpDir, "src-tauri", "src", "bin", "burned-web.rs");

  fs.mkdirSync(path.dirname(sourcePath), { recursive: true });
  fs.writeFileSync(binaryPath, "binary");
  fs.writeFileSync(sourcePath, "// source");

  const now = Date.now() / 1000;
  fs.utimesSync(sourcePath, now - 120, now - 120);
  fs.utimesSync(binaryPath, now, now);

  assert.equal(needsRustRebuild(binaryPath, [sourcePath]), false);
});
