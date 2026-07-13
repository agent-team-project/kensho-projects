import { describe, expect, it, vi } from "vitest";

import type { Project, RecordDecision } from "./api/client.gen";
import { runAgentWalkingSlice, type WalkingSliceClient } from "./workflow";

const project: Project = {
  id: "project-id", organization_id: "org-id", title: "Agent slice", outcome: "Parity",
  mode: "exploration", state: "proposed", version: 1, hypothesis: "Shared plane",
  falsifier: "Private mutation", decision_criteria: ["Equivalent history"], experiment_bound: "M1",
};
const decision: RecordDecision = {
  kind: "continue", question: "Continue?", choice: "Yes", alternatives: [],
  rationale: "Evidence", evidence: [], consequences: [],
};

describe("standalone agent flow", () => {
  it("uses the same generated operations and agent security for the full transaction", async () => {
    const client: WalkingSliceClient = {
      createProject: vi.fn().mockResolvedValue(project),
      recordDecision: vi.fn().mockResolvedValue({ body: { ...decision, id: "decision-id", project_id: project.id, actor_id: "agent-id", actor_kind: "agent", principal_id: "human-id", recorded_at: "2026-07-13T00:00:00Z" }, version: '"2"' }),
      getProject: vi.fn().mockResolvedValue({ ...project, version: 2 }),
      listProjectActivity: vi.fn().mockResolvedValue([{ event_id: "event-id" }]),
    };

    const result = await runAgentWalkingSlice(client, "org-id", "agent-token", project, decision);

    expect(result.version).toBe('"2"');
    expect(client.createProject).toHaveBeenCalledWith(expect.anything(), expect.anything(), expect.objectContaining({ security: { kind: "agent-bearer", bearerToken: "agent-token" } }));
    expect(client.recordDecision).toHaveBeenCalledOnce();
    expect(client.getProject).toHaveBeenCalledOnce();
    expect(client.listProjectActivity).toHaveBeenCalledOnce();
  });
});
