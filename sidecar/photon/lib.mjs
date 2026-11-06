// The Photon sidecar's logic, kept apart from send.mjs so tests can drive it
// with a fake SDK. send.mjs describes the contract with hackathon-radar.

/** The oldest Node the SDK's dependencies support. */
export const MIN_NODE_VERSION = "20.18.1";

/** The environment variables the project credentials come from. */
export const PROJECT_ID_VAR = "PHOTON_PROJECT_ID";
export const PROJECT_SECRET_VAR = "PHOTON_PROJECT_SECRET";

/** How long one send may take before the sidecar gives up. */
export const SEND_TIMEOUT_MS = 45_000;

// A phone number in E.164 format; anything else is taken as a Photon space ID.
const E164 = /^\+[1-9]\d{6,14}$/;

/** A failure with one of the result codes hackathon-radar knows. */
export class SidecarError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "SidecarError";
    this.code = code;
  }
}

/** Whether `version` (such as "22.1.0") is at least MIN_NODE_VERSION. */
export function nodeIsRecentEnough(version) {
  const have = String(version).split(".").map(Number);
  const need = MIN_NODE_VERSION.split(".").map(Number);
  for (let i = 0; i < need.length; i++) {
    const h = have[i] || 0;
    if (h !== need[i]) return h > need[i];
  }
  return true;
}

/** Reads `{"to": ..., "text": ...}` from the text on stdin. */
export function parseRequest(input) {
  let request;
  try {
    request = JSON.parse(input);
  } catch {
    throw new SidecarError("bad_request", "stdin is not JSON");
  }
  const to = typeof request?.to === "string" ? request.to.trim() : "";
  const text = typeof request?.text === "string" ? request.text : "";
  if (!to) throw new SidecarError("bad_request", "the request has no `to`");
  if (!text.trim()) throw new SidecarError("bad_request", "the request has no `text`");
  return { to, text };
}

/** The project ID and secret from the environment. */
export function readCredentials(env) {
  const projectId = (env[PROJECT_ID_VAR] || "").trim();
  const projectSecret = (env[PROJECT_SECRET_VAR] || "").trim();
  const missing = [
    projectId ? null : PROJECT_ID_VAR,
    projectSecret ? null : PROJECT_SECRET_VAR,
  ].filter(Boolean);
  if (missing.length > 0) {
    throw new SidecarError("missing_credentials", `${missing.join(" and ")} not set`);
  }
  return { projectId, projectSecret };
}

/** Loads the parts of Photon's SDK the sidecar uses. */
export async function loadSdk() {
  try {
    const core = await import("@spectrum-ts/core");
    const { imessage } = await import("@spectrum-ts/imessage");
    return { Spectrum: core.Spectrum, text: core.text, imessage };
  } catch (error) {
    if (error?.code === "ERR_MODULE_NOT_FOUND") {
      throw new SidecarError("sdk_missing", "the Photon SDK is not installed; run `npm ci` here");
    }
    throw error;
  }
}

/**
 * Sends `request.text` to `request.to` from the project's iMessage line and
 * resolves to the sent message's ID. The SDK connection is always closed.
 */
export async function send(request, credentials, sdk) {
  const app = await sdk.Spectrum({
    projectId: credentials.projectId,
    projectSecret: credentials.projectSecret,
    providers: [sdk.imessage.config()],
    telemetry: false,
    options: { logLevel: "warn" },
  });
  try {
    const im = sdk.imessage(app);
    // A phone number names a direct conversation; anything else is a space
    // that already exists, such as a group chat.
    const space = E164.test(request.to)
      ? await im.space.create(request.to)
      : await im.space.get(request.to);
    const message = await space.send(sdk.text(request.text));
    return message?.id ?? null;
  } finally {
    await Promise.race([
      Promise.resolve(app.stop()).catch(() => {}),
      new Promise((resolve) => setTimeout(resolve, 3000).unref()),
    ]);
  }
}

/** The result code and message for an error from `send` or the SDK. */
export function classify(error) {
  const message = String(error?.message ?? error ?? "unknown error");
  if (error instanceof SidecarError) return { error: error.code, message };
  const lowered = message.toLowerCase();
  // A shared line refuses to start a conversation with a number that hasn't
  // texted it first; the SDK raises AuthenticationError("Target not allowed
  // for this project") from space.send.
  if (lowered.includes("target not allowed")) return { error: "not_texted_yet", message };
  // Spectrum() checks the ID and secret against Spectrum Cloud first and
  // throws SpectrumCloudError with the HTTP status when they're refused.
  if (error?.name === "SpectrumCloudError" && [401, 403, 404].includes(error.status)) {
    return { error: "bad_credentials", message };
  }
  if (error?.name === "AuthenticationError") return { error: "bad_credentials", message };
  if (error?.name === "RateLimitError") return { error: "rate_limited", message };
  return { error: "send_failed", message };
}

/** The text with every copy of each secret taken out. */
export function scrub(text, secrets) {
  let out = String(text);
  for (const secret of secrets) {
    if (secret) out = out.split(secret).join("[redacted]");
  }
  return out;
}

/**
 * One whole sidecar run: the result to print as one line of JSON, and the
 * exit code. Never throws.
 */
export async function main({ input, env, nodeVersion, load = loadSdk, timeoutMs = SEND_TIMEOUT_MS }) {
  const secrets = [env[PROJECT_SECRET_VAR]];
  let timer;
  try {
    if (!nodeIsRecentEnough(nodeVersion)) {
      throw new SidecarError(
        "node_too_old",
        `Node ${nodeVersion} is older than ${MIN_NODE_VERSION}`,
      );
    }
    const request = parseRequest(input);
    const credentials = readCredentials(env);
    const sdk = await load();
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(
        () => reject(new SidecarError("timeout", `no answer from Photon after ${timeoutMs / 1000} seconds`)),
        timeoutMs,
      );
    });
    const messageId = await Promise.race([send(request, credentials, sdk), timeout]);
    return { result: { ok: true, message_id: messageId }, code: 0 };
  } catch (error) {
    const { error: code, message } = classify(error);
    return { result: { ok: false, error: code, message: scrub(message, secrets) }, code: 1 };
  } finally {
    clearTimeout(timer);
  }
}
