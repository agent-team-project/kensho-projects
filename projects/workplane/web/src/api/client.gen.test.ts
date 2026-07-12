import { describe, expect, it } from "vitest";

import {
  WorkplaneClient,
  WorkplaneContractError,
  type VersionETag,
} from "./client.gen";

const decisionBody = {
  kind: "continue",
  question: "Continue?",
  choice: "yes",
  alternatives: [],
  rationale: "evidence supports it",
  evidence: [],
  consequences: [],
};

describe("generated response headers", () => {
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
});
