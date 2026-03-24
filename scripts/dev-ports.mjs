import net from "node:net";

export const DEFAULT_DEV_PORT = 1420;
const DEFAULT_HOST = "127.0.0.1";

function canBind(port, host = DEFAULT_HOST) {
  return new Promise((resolve) => {
    const server = net.createServer();

    server.once("error", () => {
      resolve(false);
    });

    server.listen(port, host, () => {
      server.close(() => resolve(true));
    });
  });
}

export async function findAvailablePort(startPort = DEFAULT_DEV_PORT, maxAttempts = 20) {
  for (let offset = 0; offset < maxAttempts; offset += 1) {
    const port = startPort + offset;
    if (await canBind(port)) {
      return port;
    }
  }

  throw new Error(
    `Could not find a free dev port between ${startPort} and ${startPort + maxAttempts - 1}.`
  );
}

export function parsePortArg(argv = []) {
  const portFlagIndex = argv.indexOf("--port");
  if (portFlagIndex === -1) {
    return null;
  }

  const rawValue = argv[portFlagIndex + 1];
  const port = Number.parseInt(rawValue ?? "", 10);
  if (!Number.isInteger(port) || port <= 0) {
    throw new Error(`Invalid --port value: ${rawValue ?? "(missing)"}`);
  }

  return port;
}

export function buildTauriDevOverride(port) {
  return {
    build: {
      beforeDevCommand: `node ./scripts/dev-server.mjs --port ${port} --strict-port`,
      devUrl: `http://${DEFAULT_HOST}:${port}`
    }
  };
}
