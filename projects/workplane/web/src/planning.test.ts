import { describe, expect, it, vi } from "vitest";

import type { ForecastInput, PromoteProjectRequest } from "./api/client.gen";
import { runAgentPlanningSlice, type PlanningClient } from "./planning";

const forecast: ForecastInput = {
  p50_at: "2026-07-14T00:00:00Z",
  p90_at: "2026-07-15T00:00:00Z",
  review_after: "2026-07-13T18:00:00Z",
  basis: "Measured throughput",
  assumptions: ["No scope expansion"],
  reason_codes: ["new-evidence"],
  impact: "Quality gates are unchanged",
};

const promotion: PromoteProjectRequest = {
  decision: {
    question: "Promote?", choice: "Promote", alternatives: ["Continue"],
    rationale: "The path is known", evidence: ["decision criteria"], consequences: ["Track B delivery"],
  },
  residual_uncertainty: "Integration risk remains visible",
  priority_rationale: "The outcome is the admitted next unit",
  deliverables: [{
    title: "M2C", description: "Planning contract spine", required: true, weight: 1000,
    state: "ready", acceptance_criteria: ["Exact-head smoke passes"],
  }],
};

describe("standalone M2C planning flow", () => {
  it("uses the same generated versioned operations for an agent", async () => {
    const deliverable = { id: "deliverable-id" };
    const client: PlanningClient = {
      reforecastProject: vi.fn().mockResolvedValue({ body: {}, version: '"2"' }),
      activateProject: vi.fn().mockResolvedValue({ body: {}, version: '"3"' }),
      promoteProject: vi.fn().mockResolvedValue({ body: { deliverables: [deliverable] }, version: '"6"' }),
      reforecastDeliverable: vi.fn().mockResolvedValue({ body: {}, version: '"7"' }),
      listDeliverables: vi.fn().mockResolvedValue([deliverable]),
      listProjectForecasts: vi.fn().mockResolvedValue([]),
    };

    const result = await runAgentPlanningSlice(client, "project-id", '"1"', "agent-token", forecast, promotion, forecast);

    expect(result.scopedForecast.version).toBe('"7"');
    expect(client.reforecastProject).toHaveBeenCalledWith(
      expect.anything(), expect.anything(),
      expect.objectContaining({ security: { kind: "agent-bearer", bearerToken: "agent-token" } }),
    );
    expect(client.promoteProject).toHaveBeenCalledOnce();
    expect(client.reforecastDeliverable).toHaveBeenCalledWith(
      { deliverable_id: "deliverable-id" }, expect.anything(), expect.anything(),
    );
  });
});
