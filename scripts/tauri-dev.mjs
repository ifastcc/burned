import { spawn } from "node:child_process";

import { buildTauriDevOverride, DEFAULT_DEV_PORT, findAvailablePort } from "./dev-ports.mjs";

async function main() {
  const port = await findAvailablePort(DEFAULT_DEV_PORT);
  const override = JSON.stringify(buildTauriDevOverride(port));
  const child = spawn(
    "pnpm",
    ["tauri", "dev", "--config", override, ...process.argv.slice(2)],
    {
      stdio: "inherit",
      shell: process.platform === "win32"
    }
  );

  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }

    process.exit(code ?? 0);
  });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
