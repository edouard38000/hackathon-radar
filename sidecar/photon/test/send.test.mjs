// The sidecar against a fake Photon SDK: nothing here reaches the network.
// Its results are compared with the fixtures in tests/fixtures/photon/, the
// same files the Rust Photon channel's tests read, so both sides of the
// contract are held to one recording.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { classify, main, nodeIsRecentEnough, parseRequest, scrub } from "../lib.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const sidecarDir = join(here, "..");
const fixtures = join(here, "../../../tests/fixtures/photon");
const fixture = (name) => readFileSync(join(fixtures, name), "utf8").trimEnd();

const ENV = {
  PHOTON_PROJECT_ID: "proj_fixture",
  PHOTON_PROJECT_SECRET: "secret_fixture_not_real",
};
const NODE = "22.11.0";

// Errors shaped like the SDK's own.
class SpectrumCloudError extends Error {
  constructor(status, code, message) {
    super(message);
    this.name = "SpectrumCloudError";
    this.status = status;
    this.code = code;
  }
}
class AuthenticationError extends Error {
  constructor(message) {
    super(message);
    this.name = "AuthenticationError";
  }
}
class RateLimitError extends Error {
  constructor(message) {
    super(message);
    this.name = "RateLimitError";
  }
}

/** A fake SDK that records every call. */
function fakeSdk({ startError, sendError, sendNever = false } = {}) {
  const calls = { spectrum: [], create: [], get: [], sent: [], stopped: 0 };
  const space = {
    async send(content) {
      calls.sent.push(content);
      if (sendError) throw sendError;
      if (sendNever) return new Promise(() => {});
      return { id: "fixture-message-1" };
    },
  };
  const app = {
    async stop() {
      calls.stopped += 1;
    },
  };
  const imessage = (instance) => {
    assert.equal(instance, app);
    return {
      space: {
        async create(to) {
          calls.create.push(to);
          return space;
        },
        async get(id) {
          calls.get.push(id);
          return space;
        },
      },
    };
  };
  imessage.config = () => ({ provider: "imessage" });
  const sdk = {
    async Spectrum(options) {
      calls.spectrum.push(options);
      if (startError) throw startError;
      return app;
    },
    text: (text) => ({ type: "text", text }),
    imessage,
  };
  return { sdk, calls };
}

async function run(fake, { input = fixture("request.json"), env = ENV, timeoutMs } = {}) {
  const { result, code } = await main({
    input,
    env,
    nodeVersion: NODE,
    load: async () => fake.sdk,
    timeoutMs,
  });
  return { line: JSON.stringify(result), code };
}

test("sends the request's text to the phone number from the project's line", async () => {
  const fake = fakeSdk();
  const { line, code } = await run(fake);

  assert.equal(line, fixture("sent.json"));
  assert.equal(code, 0);
  assert.deepEqual(fake.calls.spectrum, [
    {
      projectId: "proj_fixture",
      projectSecret: "secret_fixture_not_real",
      providers: [{ provider: "imessage" }],
      telemetry: false,
      options: { logLevel: "warn" },
    },
  ]);
  assert.deepEqual(fake.calls.create, ["+32470123456"]);
  assert.deepEqual(fake.calls.get, []);
  assert.deepEqual(fake.calls.sent, [
    { type: "text", text: "hackathon-radar: 1 Hackathon newly fits you\n\nMystery Hack\n1 Dec 2026" },
  ]);
  assert.equal(fake.calls.stopped, 1, "the connection is closed");
});

test("a space ID is looked up instead of created", async () => {
  const fake = fakeSdk();
  const { code } = await run(fake, {
    input: JSON.stringify({ to: "iMessage;+;chat123456", text: "hello" }),
  });

  assert.equal(code, 0);
  assert.deepEqual(fake.calls.create, []);
  assert.deepEqual(fake.calls.get, ["iMessage;+;chat123456"]);
});

test("a number that hasn't texted the line yet is reported as such", async () => {
  const fake = fakeSdk({ sendError: new AuthenticationError("Target not allowed for this project") });
  const { line, code } = await run(fake);

  assert.equal(line, fixture("not-texted-yet.json"));
  assert.equal(code, 1);
  assert.equal(fake.calls.stopped, 1, "the connection is closed after a failure too");
});

test("refused credentials are reported as bad credentials", async () => {
  for (const status of [401, 403, 404]) {
    const fake = fakeSdk({
      startError: new SpectrumCloudError(status, "UNAUTHORIZED", "Invalid credentials"),
    });
    const { line, code } = await run(fake);
    assert.equal(line, fixture("bad-credentials.json"), `status ${status}`);
    assert.equal(code, 1);
  }
});

test("rate limits are reported as such", async () => {
  const fake = fakeSdk({ sendError: new RateLimitError("Daily message limit exceeded") });
  const { line } = await run(fake);
  assert.equal(line, fixture("rate-limited.json"));
});

test("the secret never appears in a result", async () => {
  const fake = fakeSdk({
    startError: new Error(`request with secret_fixture_not_real failed`),
  });
  const { line, code } = await run(fake);

  assert.equal(code, 1);
  assert.equal(
    line,
    JSON.stringify({ ok: false, error: "send_failed", message: "request with [redacted] failed" }),
  );
});

test("a send that hangs gives up", async () => {
  const fake = fakeSdk({ sendNever: true });
  const { line, code } = await run(fake, { timeoutMs: 20 });

  assert.equal(code, 1);
  assert.equal(JSON.parse(line).error, "timeout");
});

test("missing credentials and bad requests are refused before the SDK loads", async () => {
  let loaded = false;
  const load = async () => {
    loaded = true;
    return fakeSdk().sdk;
  };

  const noSecret = await main({
    input: fixture("request.json"),
    env: { PHOTON_PROJECT_ID: "proj_fixture" },
    nodeVersion: NODE,
    load,
  });
  assert.deepEqual(noSecret.result, {
    ok: false,
    error: "missing_credentials",
    message: "PHOTON_PROJECT_SECRET not set",
  });

  const notJson = await main({ input: "to=+3247", env: ENV, nodeVersion: NODE, load });
  assert.equal(notJson.result.error, "bad_request");

  const tooOld = await main({ input: fixture("request.json"), env: ENV, nodeVersion: "18.17.0", load });
  assert.deepEqual(tooOld.result, {
    ok: false,
    error: "node_too_old",
    message: "Node 18.17.0 is older than 20.18.1",
  });
  assert.equal(loaded, false);
});

test("requests need a recipient and some text", () => {
  assert.deepEqual(parseRequest(fixture("request.json")).to, "+32470123456");
  assert.throws(() => parseRequest('{"text":"hi"}'), { code: "bad_request" });
  assert.throws(() => parseRequest('{"to":"+32470123456","text":"  "}'), { code: "bad_request" });
});

test("Node versions are compared numerically", () => {
  assert.equal(nodeIsRecentEnough("20.18.1"), true);
  assert.equal(nodeIsRecentEnough("20.19.0"), true);
  assert.equal(nodeIsRecentEnough("26.8.2"), true);
  assert.equal(nodeIsRecentEnough("20.18.0"), false);
  assert.equal(nodeIsRecentEnough("20.9.9"), false);
  assert.equal(nodeIsRecentEnough("18.20.4"), false);
});

test("errors the SDK doesn't name are send failures", () => {
  assert.deepEqual(classify(new Error("socket hang up")), {
    error: "send_failed",
    message: "socket hang up",
  });
  assert.equal(scrub("a s3 b s3", ["s3", undefined]), "a [redacted] b [redacted]");
});

// The entry point itself, run by Node as hackathon-radar runs it.
function sidecar(script, env) {
  return spawnSync(process.execPath, [script], {
    input: fixture("request.json"),
    env: { PATH: process.env.PATH, ...env },
    encoding: "utf8",
  });
}

test("the entry point prints exactly one line of JSON", () => {
  const run = sidecar(join(sidecarDir, "send.mjs"), { PHOTON_PROJECT_ID: "proj_fixture" });

  assert.equal(run.status, 1);
  assert.equal(
    run.stdout,
    '{"ok":false,"error":"missing_credentials","message":"PHOTON_PROJECT_SECRET not set"}\n',
  );
});

test("without its dependencies installed, the sidecar says to run npm ci", () => {
  // A copy of the sidecar with no node_modules anywhere above it.
  const dir = mkdtempSync(join(tmpdir(), "photon-sidecar-"));
  try {
    for (const file of ["send.mjs", "lib.mjs", "package.json"]) {
      copyFileSync(join(sidecarDir, file), join(dir, file));
    }
    const run = sidecar(join(dir, "send.mjs"), ENV);

    assert.equal(run.status, 1);
    assert.equal(run.stdout, `${fixture("sdk-missing.json")}\n`);
    assert.ok(!run.stderr.includes("secret_fixture_not_real"));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
