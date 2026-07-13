import {
  type DeliverableInput,
  type ForecastInput,
  type AgentBearerSecurity,
  type HumanMutationSecurity,
  type PromoteProjectRequest,
  type VersionETag,
  type WorkplaneClient,
} from "./api/client.gen";

export type PlanningClient = Pick<
  WorkplaneClient,
  | "reforecastProject"
  | "activateProject"
  | "promoteProject"
  | "reforecastDeliverable"
  | "listDeliverables"
  | "listProjectForecasts"
>;

const key = (prefix: string) => `${prefix}-${crypto.randomUUID()}`;

export async function runPlanningSlice(
  client: PlanningClient,
  projectID: string,
  version: VersionETag,
  security: AgentBearerSecurity | HumanMutationSecurity,
  forecast: ForecastInput,
  promotion: PromoteProjectRequest,
  deliverableForecast: ForecastInput,
) {
  const projectForecast = await client.reforecastProject(
    { project_id: projectID },
    forecast,
    { idempotencyKey: key("planning-project-forecast"), expectedVersion: version, security },
  );
  const activated = await client.activateProject(
    { project_id: projectID },
    { reason: "The exploration activation contract is complete." },
    { idempotencyKey: key("planning-activate"), expectedVersion: projectForecast.version, security },
  );
  const promoted = await client.promoteProject(
    { project_id: projectID },
    promotion,
    { idempotencyKey: key("planning-promote"), expectedVersion: activated.version, security },
  );
  const firstDeliverable = promoted.body.deliverables[0];
  if (firstDeliverable === undefined) throw new Error("Promotion returned no deliverable");
  const scopedForecast = await client.reforecastDeliverable(
    { deliverable_id: firstDeliverable.id },
    deliverableForecast,
    { idempotencyKey: key("planning-deliverable-forecast"), expectedVersion: promoted.version, security },
  );
  const [deliverables, history] = await Promise.all([
    client.listDeliverables({ project_id: projectID }, { security }),
    client.listProjectForecasts({ project_id: projectID }, { security }),
  ]);
  return { projectForecast, activated, promoted, scopedForecast, deliverables, history };
}

export async function runAgentPlanningSlice(
  client: PlanningClient,
  projectID: string,
  version: VersionETag,
  bearerToken: string,
  forecast: ForecastInput,
  promotion: PromoteProjectRequest,
  deliverableForecast: ForecastInput,
) {
  return runPlanningSlice(
    client,
    projectID,
    version,
    { kind: "agent-bearer", bearerToken },
    forecast,
    promotion,
    deliverableForecast,
  );
}

export type { DeliverableInput };
