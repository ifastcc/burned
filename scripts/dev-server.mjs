import { spawn } from "node:child_process";

import { DEFAULT_DEV_PORT, findAvailablePort, parsePortArg } from "./dev-ports.mjs";

async function main() {
  const args = process.argv.slice(2);
  const requestedPort = parsePortArg(args);
  const strictPort = args.includes("--strict-port");
  const port = requestedPort ?? (await findAvailablePort(DEFAULT_DEV_PORT));
  const viteArgs = ["exec", "vite", "--port", String(port)];

  if (process.env.TAURI_DEV_HOST) {
    viteArgs.push("--host", process.env.TAURI_DEV_HOST);
  }

  if (strictPort || requestedPort != null) {
    viteArgs.push("--strictPort");
  }

  const child = spawn("pnpm", viteArgs, {
    stdio: "inherit",
    shell: process.platform === "win32"
  });

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
