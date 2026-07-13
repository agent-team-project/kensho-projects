import {
  type Activity,
  type CreateExplorationProject,
  type Decision,
  type Project,
  type RecordDecision,
  type VersionETag,
  type WorkplaneClient,
} from "./api/client.gen";

export type WalkingSliceClient = Pick<
  WorkplaneClient,
  "createProject" | "getProject" | "listProjectActivity" | "recordDecision"
>;

const key = (prefix: string) => `${prefix}-${crypto.randomUUID()}`;
const versionETag = (version: number): VersionETag => {
  if (!Number.isSafeInteger(version) || version < 1) throw new Error("Invalid aggregate version");
  return (`"${version.toFixed(0)}"`) as VersionETag;
};

export async function createExploration(
  client: WalkingSliceClient,
  organizationID: string,
  csrfToken: string,
  input: CreateExplorationProject,
): Promise<Project> {
  return client.createProject({ org_id: organizationID }, input, {
    idempotencyKey: key("human-create"),
    security: { kind: "human-session", csrfToken },
  });
}

export async function readExploration(client: WalkingSliceClient, projectID: string) {
  const security = { kind: "human-session" } as const;
  const [project, activity] = await Promise.all([
    client.getProject({ project_id: projectID }, { security }),
    client.listProjectActivity({ project_id: projectID }, { security }),
  ]);
  return { project, activity };
}

export async function continueExploration(
  client: WalkingSliceClient,
  project: Project,
  csrfToken: string,
  input: RecordDecision,
): Promise<{ project: Project; decision: Decision; activity: Activity[]; version: VersionETag }> {
  const committed = await client.recordDecision({ project_id: project.id }, input, {
    idempotencyKey: key("human-decision"),
    expectedVersion: versionETag(project.version),
    security: { kind: "human-session", csrfToken },
  });
  const [updated, activity] = await Promise.all([
    client.getProject({ project_id: project.id }, { security: { kind: "human-session" } }),
    client.listProjectActivity({ project_id: project.id }, { security: { kind: "human-session" } }),
  ]);
  return { project: updated, decision: committed.body, activity, version: committed.version };
}

export async function runAgentWalkingSlice(
  client: WalkingSliceClient,
  organizationID: string,
  bearerToken: string,
  projectInput: CreateExplorationProject,
  decisionInput: RecordDecision,
) {
  const security = { kind: "agent-bearer", bearerToken } as const;
  const project = await client.createProject({ org_id: organizationID }, projectInput, {
    idempotencyKey: key("agent-create"), security,
  });
  const decision = await client.recordDecision({ project_id: project.id }, decisionInput, {
    idempotencyKey: key("agent-decision"), expectedVersion: versionETag(project.version), security,
  });
  const [projection, activity] = await Promise.all([
    client.getProject({ project_id: project.id }, { security }),
    client.listProjectActivity({ project_id: project.id }, { security }),
  ]);
  return { project: projection, decision: decision.body, activity, version: decision.version };
}
