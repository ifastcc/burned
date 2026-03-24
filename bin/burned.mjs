#!/usr/bin/env node

import path from "node:path";
import { fileURLToPath } from "node:url";

import { runBurnedCli } from "../scripts/burned-cli.mjs";

const binDir = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(binDir, "..");

runBurnedCli({ rootDir }).catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
