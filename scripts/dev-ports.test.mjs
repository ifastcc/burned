import assert from "node:assert/strict";
import net from "node:net";
import test from "node:test";

import { buildTauriDevOverride, findAvailablePort } from "./dev-ports.mjs";

function listen(server, port) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => {
      server.removeListener("error", reject);
      resolve();
    });
  });
}

function close(server) {
  return new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

test("findAvailablePort skips occupied ports", async () => {
  const occupied = net.createServer();
  await listen(occupied, 43120);

  try {
    const nextPort = await findAvailablePort(43120, 5);
    assert.equal(nextPort, 43121);
  } finally {
    await close(occupied);
  }
});

test("buildTauriDevOverride keeps beforeDevCommand and devUrl in sync", () => {
  const override = buildTauriDevOverride(1432);

  assert.deepEqual(override, {
    build: {
      beforeDevCommand: "node ./scripts/dev-server.mjs --port 1432 --strict-port",
      devUrl: "http://127.0.0.1:1432"
    }
  });
});
