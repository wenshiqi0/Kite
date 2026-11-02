import { createSign } from "node:crypto";
import { array, check, integer, type Json, type ObjectValue, object, string } from "@kite/protocol";
import { atomic, bounded, readJson } from "@kite/runtime";

const TOKEN_URL = "https://oauth2.googleapis.com/token",
  SCOPE = "https://www.googleapis.com/auth/firebase.messaging";
export class ProviderResponseError extends Error {
  constructor(readonly data: Json) {
    super("push provider rejected request");
  }
}
async function post(url: string, body: string, headers: Record<string, string>): Promise<Json> {
  const response = await fetch(url, {
    method: "POST",
    body,
    headers,
    signal: AbortSignal.timeout(8000),
    redirect: "error",
  });
  check(response.body, "provider response missing");
  const raw = (await bounded(response.body, 65536)).toString();
  let data: Json;
  try {
    data = JSON.parse(raw) as Json;
  } catch {
    throw new Error("invalid push provider response");
  }
  if (!response.ok) throw new ProviderResponseError(data);
  return data;
}
export type PushTransport = (
  url: string,
  body: string,
  headers: Record<string, string>,
) => Promise<Json>;
export async function accessToken(
  account: ObjectValue,
  cache: string,
  transport: PushTransport = post,
): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  try {
    const saved = object(readJson(cache, 65536, true));
    if (saved.email === account.client_email && Number(saved.expires) > now + 60)
      return string(saved.token);
  } catch {
    /* renew token */
  }
  const encode = (v: unknown) => Buffer.from(JSON.stringify(v)).toString("base64url"),
    unsigned = `${encode({ alg: "RS256", typ: "JWT" })}.${encode({ iss: account.client_email, scope: SCOPE, aud: TOKEN_URL, iat: now, exp: now + 3600 })}`,
    signature = createSign("RSA-SHA256")
      .update(unsigned)
      .sign(string(account.private_key))
      .toString("base64url"),
    result = object(
      await transport(
        TOKEN_URL,
        new URLSearchParams({
          grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
          assertion: `${unsigned}.${signature}`,
        }).toString(),
        { "Content-Type": "application/x-www-form-urlencoded" },
      ),
    ),
    token = string(result.access_token);
  check(token.length > 0 && token.length <= 8192, "invalid access token");
  atomic(cache, {
    email: account.client_email,
    token,
    expires: now + Math.min(3600, integer(result.expires_in)),
  });
  return token;
}
export function payload(subscription: ObjectValue, event: ObjectValue): Json {
  check(
    /^[a-f0-9]{64}$/.test(string(event.id)) &&
      ["completed", "attention", "question", "failed"].includes(string(event.kind)),
    "invalid event",
  );
  check(/^[a-zA-Z0-9_-]{1,128}$/.test(string(subscription.host_id)), "invalid host identity");
  return {
    message: {
      token: subscription.token,
      data: {
        kite_notice_id: `${subscription.host_id}/${event.id}`,
        kite_kind: event.kind,
        kite_session_id: event.session_id,
        kite_created_ms: String(event.created_ms),
      },
      android: {
        priority: "HIGH",
        ttl: "3600s",
        restricted_package_name: "io.github.wenshiqi0.kite",
      },
    },
  };
}
export async function deliver(
  accountFile: string,
  cache: string,
  job: ObjectValue,
  transport: PushTransport = post,
): Promise<Json> {
  const account = object(readJson(accountFile, 65536, true)),
    project = string(account.project_id),
    subscription = object(job.subscription),
    events = array(job.events);
  check(
    /^[a-z][a-z0-9-]{4,62}$/.test(project) && subscription.project_id === project,
    "Firebase project mismatch",
  );
  check(events.length <= 16, "push batch too large");
  const token = await accessToken(account, cache, transport),
    results: ObjectValue[] = [];
  for (const value of events) {
    const event = object(value);
    let status = "sent";
    try {
      await transport(
        `https://fcm.googleapis.com/v1/projects/${project}/messages:send`,
        JSON.stringify(payload(subscription, event)),
        { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
      );
    } catch (error) {
      let removed = false;
      try {
        if (error instanceof ProviderResponseError)
          removed = array(object(object(error.data).error).details ?? [])
            .map(object)
            .some((r) => r.errorCode === "UNREGISTERED");
      } catch {
        /* return no provider details */
      }
      status = removed ? "unregistered" : "retry";
    }
    results.push({ id: event.id, status });
    if (status !== "sent") break;
  }
  return { results };
}
