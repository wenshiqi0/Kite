import { afterAll, expect, test } from "bun:test";
import { generateKeyPairSync, verify } from "node:crypto";
import { mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Json, object, parse, string } from "@kite/protocol";
import { accessToken, deliver, ProviderResponseError, type PushTransport } from "../src/push";

const root = mkdtempSync(join(tmpdir(), "kite-push-test-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
test("signs RS256 service assertions in memory and reuses a private token cache", async () => {
  const pair = generateKeyPairSync("rsa", { modulusLength: 2048 }),
    account = {
      client_email: "owned@example.test",
      private_key: pair.privateKey.export({ type: "pkcs8", format: "pem" }).toString(),
    },
    cache = join(root, "cache.json");
  let requests = 0;
  const transport: PushTransport = async (url, body) => {
    requests++;
    expect(url).toBe("https://oauth2.googleapis.com/token");
    const assertion = new URLSearchParams(body).get("assertion") || "",
      [header, claims, signature] = assertion.split(".");
    expect(object(parse(Buffer.from(string(header), "base64url").toString())).alg).toBe("RS256");
    expect(object(parse(Buffer.from(string(claims), "base64url").toString())).iss).toBe(
      account.client_email,
    );
    expect(
      verify(
        "RSA-SHA256",
        Buffer.from(`${header}.${claims}`),
        pair.publicKey,
        Buffer.from(string(signature), "base64url"),
      ),
    ).toBe(true);
    return { access_token: "owned-token", expires_in: 3600 };
  };
  expect(await accessToken(account, cache, transport)).toBe("owned-token");
  expect(await accessToken(account, cache, transport)).toBe("owned-token");
  expect(requests).toBe(1);
  expect(statSync(cache).mode & 0o777).toBe(0o600);
});
test("push stops on an invalidated device without returning provider secrets", async () => {
  const file = join(root, "account.json"),
    cache = join(root, "delivery-cache.json"),
    account = { project_id: "owned-project", client_email: "owned@example.test" };
  writeFileSync(file, JSON.stringify(account), { mode: 0o600 });
  writeFileSync(
    cache,
    JSON.stringify({
      email: account.client_email,
      expires: Math.floor(Date.now() / 1000) + 3600,
      token: "owned-token",
    }),
    { mode: 0o600 },
  );
  let requests = 0;
  const transport: PushTransport = async (url, _body, headers) => {
    requests++;
    expect(url).toBe("https://fcm.googleapis.com/v1/projects/owned-project/messages:send");
    expect(headers.Authorization).toBe("Bearer owned-token");
    throw new ProviderResponseError({
      error: { details: [{ errorCode: "UNREGISTERED", token: "provider-secret" }] },
    });
  };
  const event = {
    id: "a".repeat(64),
    kind: "completed",
    session_id: "owned-session",
    created_ms: 123,
  };
  const value: Json = await deliver(
    file,
    cache,
    {
      subscription: { host_id: "owned", token: "owned-device", project_id: "owned-project" },
      events: [event, { ...event, id: "b".repeat(64) }],
    },
    transport,
  );
  expect(value).toEqual({ results: [{ id: event.id, status: "unregistered" }] });
  expect(requests).toBe(1);
  expect(JSON.stringify(value)).not.toContain("provider-secret");
});
