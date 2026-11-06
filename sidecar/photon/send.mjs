#!/usr/bin/env node
// Sends one hackathon-radar Alert through Photon and exits (ADR 0005).
//
//   stdin:  {"to": "+32470123456", "text": "..."}
//   env:    PHOTON_PROJECT_ID, PHOTON_PROJECT_SECRET
//   stdout: one line of JSON, {"ok": true, "message_id": "..."} or
//           {"ok": false, "error": "<code>", "message": "..."}
//   exit:   0 when sent, 1 otherwise
//
// The SDK logs to the console; all of that goes to stderr so stdout holds
// only the result.

import { main } from "./lib.mjs";

for (const level of ["log", "info", "debug", "warn"]) {
  console[level] = (...args) => console.error(...args);
}

let input = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) input += chunk;

const { result, code } = await main({
  input,
  env: process.env,
  nodeVersion: process.versions.node,
});

// The SDK may leave connections open, so exit once the line is written.
process.stdout.write(`${JSON.stringify(result)}\n`, () => process.exit(code));
