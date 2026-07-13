import { describe, expect, it } from "vitest";

import {
  WorkplaneClient,
  WorkplaneContractError,
  requestBodyMaxBytes,
  type CreateExplorationProject,
  type RecordDecision,
} from "./client.gen";

declare const process: { env: Record<string, string | undefined> };

const baseUrl = process.env["WORKPLANE_LIVE_API_URL"] ?? "";
const liveDescribe = baseUrl === "" ? describe.skip : describe;
const organizationId = "00000000-0000-4000-8000-000000000010";
const agentToken = "wpa_local_walking_slice_agent_token_00000000000000000001";
const security = { kind: "agent-bearer" as const, bearerToken: agentToken };

liveDescribe("generated client and live API request boundaries", () => {
  it("accepts every declared maximum and rejects every over-boundary value", async () => {
    const client = new WorkplaneClient(baseUrl);
    const emailSuffix = "@example.test";
    const loginBoundary = {
      email: `${"e".repeat(320 - emailSuffix.length)}${emailSuffix}`,
      password: "p".repeat(1024),
    };
    await expect(client.login({}, loginBoundary)).rejects.toMatchObject({ status: 401 });
    for (const [index, body] of [
      { ...loginBoundary, email: `${loginBoundary.email}e` },
      { ...loginBoundary, password: `${loginBoundary.password}p` },
    ].entries()) {
      await expect(client.login({}, body)).rejects.toBeInstanceOf(WorkplaneContractError);
      const response = await fetch(`${baseUrl}/api/v1/session/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      expect(response.status, `login over-boundary case ${String(index)}`).toBe(400);
    }

    const projectBoundary: CreateExplorationProject = {
      title: "t".repeat(200),
      outcome: "o".repeat(2000),
      hypothesis: "h".repeat(2000),
      falsifier: "f".repeat(2000),
      decision_criteria: ["c".repeat(500), ...Array.from({ length: 31 }, () => "c")],
      experiment_bound: "b".repeat(1000),
    };
    const project = await client.createProject(
      { org_id: organizationId },
      projectBoundary,
      { idempotencyKey: "k".repeat(128), security },
    );
    expect(project).toMatchObject(projectBoundary);
    expect(project.version).toBe(1);
    const minimumKeyProject = await client.createProject(
      { org_id: organizationId },
      projectBoundary,
      { idempotencyKey: "m".repeat(16), security },
    );
    expect(minimumKeyProject.version).toBe(1);
    for (const [index, key] of ["s".repeat(15), "l".repeat(129)].entries()) {
      await expect(
        client.createProject({ org_id: organizationId }, projectBoundary, { idempotencyKey: key, security }),
      ).rejects.toBeInstanceOf(WorkplaneContractError);
      const response = await fetch(`${baseUrl}/api/v1/orgs/${organizationId}/projects`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${agentToken}`,
          "Content-Type": "application/json",
          "Idempotency-Key": key,
        },
        body: JSON.stringify(projectBoundary),
      });
      expect(response.status, `idempotency-key over-boundary case ${String(index)}`).toBe(400);
    }

    const decisionBoundary: RecordDecision = {
      kind: "continue",
      question: "q".repeat(2000),
      choice: "c".repeat(2000),
      alternatives: ["a".repeat(1000), ...Array.from({ length: 31 }, () => "a")],
      rationale: "r".repeat(4000),
      evidence: ["e".repeat(2000), ...Array.from({ length: 63 }, () => "e")],
      consequences: ["c".repeat(2000), ...Array.from({ length: 31 }, () => "c")],
    };
    const recorded = await client.recordDecision(
      { project_id: project.id },
      decisionBoundary,
      {
        idempotencyKey: "boundary-valid-decision-001",
        expectedVersion: '"1"',
        security,
      },
    );
    expect(recorded.body).toMatchObject(decisionBoundary);
    expect(recorded.version).toBe('"2"');

    const projectOverBoundary: Array<CreateExplorationProject> = [
      { ...projectBoundary, title: "t".repeat(201) },
      { ...projectBoundary, outcome: "o".repeat(2001) },
      { ...projectBoundary, hypothesis: "h".repeat(2001) },
      { ...projectBoundary, falsifier: "f".repeat(2001) },
      { ...projectBoundary, decision_criteria: Array.from({ length: 33 }, () => "c") },
      { ...projectBoundary, decision_criteria: ["c".repeat(501)] },
      { ...projectBoundary, decision_criteria: ["   "] },
      { ...projectBoundary, experiment_bound: "b".repeat(1001) },
    ];
    for (const [index, body] of projectOverBoundary.entries()) {
      const key = `boundary-invalid-project-${String(index).padStart(4, "0")}`;
      await expect(
        client.createProject({ org_id: organizationId }, body, { idempotencyKey: key, security }),
      ).rejects.toBeInstanceOf(WorkplaneContractError);
      const response = await fetch(`${baseUrl}/api/v1/orgs/${organizationId}/projects`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${agentToken}`,
          "Content-Type": "application/json",
          "Idempotency-Key": key,
        },
        body: JSON.stringify(body),
      });
      expect(response.status, `project over-boundary case ${String(index)}`).toBe(400);
    }

    const decisionOverBoundary: Array<RecordDecision> = [
      { ...decisionBoundary, kind: "pivot" } as unknown as RecordDecision,
      { ...decisionBoundary, question: "q".repeat(2001) },
      { ...decisionBoundary, question: "   " },
      { ...decisionBoundary, choice: "c".repeat(2001) },
      { ...decisionBoundary, alternatives: Array.from({ length: 33 }, () => "a") },
      { ...decisionBoundary, alternatives: ["a".repeat(1001)] },
      { ...decisionBoundary, rationale: "r".repeat(4001) },
      { ...decisionBoundary, evidence: Array.from({ length: 65 }, () => "e") },
      { ...decisionBoundary, evidence: ["e".repeat(2001)] },
      { ...decisionBoundary, consequences: Array.from({ length: 33 }, () => "c") },
      { ...decisionBoundary, consequences: ["c".repeat(2001)] },
    ];
    for (const [index, body] of decisionOverBoundary.entries()) {
      const key = `boundary-invalid-decision-${String(index).padStart(4, "0")}`;
      await expect(
        client.recordDecision(
          { project_id: project.id },
          body,
          { idempotencyKey: key, expectedVersion: '"2"', security },
        ),
      ).rejects.toBeInstanceOf(WorkplaneContractError);
      const response = await fetch(`${baseUrl}/api/v1/projects/${project.id}/decisions`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${agentToken}`,
          "Content-Type": "application/json",
          "Idempotency-Key": key,
          "If-Match": '"2"',
        },
        body: JSON.stringify(body),
      });
      expect(response.status, `decision over-boundary case ${String(index)}`).toBe(400);
    }

    const bodyOverByteLimit: RecordDecision = {
      kind: "continue",
      question: "q",
      choice: "c",
      alternatives: [],
      rationale: "r",
      evidence: Array.from({ length: 64 }, () => "e".repeat(1024)),
      consequences: [],
    };
    expect(new TextEncoder().encode(JSON.stringify(bodyOverByteLimit)).byteLength).toBeGreaterThan(
      requestBodyMaxBytes,
    );
    await expect(
      client.recordDecision(
        { project_id: project.id },
        bodyOverByteLimit,
        {
          idempotencyKey: "boundary-invalid-body-bytes-01",
          expectedVersion: '"2"',
          security,
        },
      ),
    ).rejects.toBeInstanceOf(WorkplaneContractError);
    const oversizedResponse = await fetch(`${baseUrl}/api/v1/projects/${project.id}/decisions`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${agentToken}`,
        "Content-Type": "application/json",
        "Idempotency-Key": "boundary-invalid-body-bytes-01",
        "If-Match": '"2"',
      },
      body: JSON.stringify(bodyOverByteLimit),
    });
    expect(oversizedResponse.status).toBe(413);
  });
});
