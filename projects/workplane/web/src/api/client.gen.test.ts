import { describe, expect, it } from "vitest";

import {
  WorkplaneClient,
  WorkplaneContractError,
  type RecordDecision,
  type VersionETag,
} from "./client.gen";

const decisionBody: RecordDecision = {
  kind: "continue",
  question: "Continue?",
  choice: "yes",
  alternatives: [],
  rationale: "evidence supports it",
  evidence: [],
  consequences: [],
};

describe("generated contract headers", () => {
  it("returns the committed ETag from recordDecision", async () => {
    let requestHeaders: Headers | undefined;
    const fetcher: typeof fetch = (_input, init) => {
      requestHeaders = new Headers(init?.headers);
      return Promise.resolve(
        new Response(JSON.stringify({ id: "decision-id" }), {
          status: 201,
          headers: { "Content-Type": "application/json", ETag: '"8"' },
        }),
      );
    };
    const client = new WorkplaneClient("http://workplane.test", fetcher);

    const response = await client.recordDecision(
      { project_id: "project-id" },
      decisionBody,
      {
        idempotencyKey: "decision-request-0001",
        expectedVersion: '"7"',
        security: { kind: "agent-bearer", bearerToken: "agent-token" },
      },
    );

    expect(response.version).toBe<VersionETag>('"8"');
    expect(requestHeaders?.get("If-Match")).toBe('"7"');
  });

  it("fails closed when recordDecision drops ETag", async () => {
    const fetcher: typeof fetch = () =>
      Promise.resolve(
        new Response(JSON.stringify({ id: "decision-id" }), {
          status: 201,
          headers: { "Content-Type": "application/json" },
        }),
      );
    const client = new WorkplaneClient("http://workplane.test", fetcher);

    await expect(
      client.recordDecision({ project_id: "project-id" }, decisionBody, {
        idempotencyKey: "decision-request-0002",
        expectedVersion: '"7"',
        security: { kind: "agent-bearer", bearerToken: "agent-token" },
      }),
    ).rejects.toThrow(WorkplaneContractError);
  });

  it("keeps typed agent mutation headers authoritative over mixed-case extensions", async () => {
    let requestHeaders: Headers | undefined;
    const fetcher: typeof fetch = (_input, init) => {
      requestHeaders = new Headers(init?.headers);
      return Promise.resolve(
        new Response(JSON.stringify({ id: "decision-id" }), {
          status: 201,
          headers: { "Content-Type": "application/json", ETag: '"8"' },
        }),
      );
    };
    const client = new WorkplaneClient("http://workplane.test", fetcher);

    await client.recordDecision(
      { project_id: "project-id" },
      decisionBody,
      {
        idempotencyKey: "typed-request-key",
        expectedVersion: '"7"',
        security: { kind: "agent-bearer", bearerToken: "typed-agent-token" },
        headers: {
          "iF-mAtCh": "extension-version",
          "aUtHoRiZaTiOn": "Bearer extension-token",
          "iDeMpOtEnCy-KeY": "extension-request-key",
          "x-CsRf-ToKeN": "extension-csrf-token",
          "X-Trace-ID": "trace-0001",
        },
      },
    );

    expect(requestHeaders?.get("If-Match")).toBe('"7"');
    expect(requestHeaders?.get("Authorization")).toBe("Bearer typed-agent-token");
    expect(requestHeaders?.get("Idempotency-Key")).toBe("typed-request-key");
    expect(requestHeaders?.get("X-CSRF-Token")).toBeNull();
    expect(requestHeaders?.get("X-Trace-ID")).toBe("trace-0001");
  });

  it("keeps typed human mutation headers authoritative over mixed-case extensions", async () => {
    let requestHeaders: Headers | undefined;
    const fetcher: typeof fetch = (_input, init) => {
      requestHeaders = new Headers(init?.headers);
      return Promise.resolve(
        new Response(JSON.stringify({ id: "decision-id" }), {
          status: 201,
          headers: { "Content-Type": "application/json", ETag: '"8"' },
        }),
      );
    };
    const client = new WorkplaneClient("http://workplane.test", fetcher);

    await client.recordDecision(
      { project_id: "project-id" },
      decisionBody,
      {
        idempotencyKey: "typed-human-request-key",
        expectedVersion: '"7"',
        security: { kind: "human-session", csrfToken: "typed-csrf-token" },
        headers: {
          "If-mAtCh": "extension-version",
          "AUTHorization": "Bearer extension-token",
          "IdEmPoTeNcY-kEy": "extension-request-key",
          "X-cSrF-tOkEn": "extension-csrf-token",
        },
      },
    );

    expect(requestHeaders?.get("If-Match")).toBe('"7"');
    expect(requestHeaders?.get("Authorization")).toBeNull();
    expect(requestHeaders?.get("Idempotency-Key")).toBe("typed-human-request-key");
    expect(requestHeaders?.get("X-CSRF-Token")).toBe("typed-csrf-token");
  });
});
