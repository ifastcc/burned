import assert from "node:assert/strict";
import test from "node:test";

import { resolveBurnedBinaryPath, resolveCargoTargetDir } from "./burned-cli.mjs";

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
