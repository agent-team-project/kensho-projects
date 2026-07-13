import { useEffect, useMemo, useState, type SyntheticEvent } from "react";

import {
  WorkplaneClient,
  WorkplaneProblem,
  type Activity,
  type CreateExplorationProject,
  type Decision,
  type Project,
  type RecordDecision,
  type Session,
} from "./api/client.gen";
import { continueExploration, createExploration, readExploration } from "./workflow";

const defaultOrganization = "00000000-0000-4000-8000-000000000010";

const initialProject: CreateExplorationProject = {
  title: "M1 human walking slice",
  outcome: "Prove one complete actor-neutral project transaction.",
  hypothesis: "Humans and agents can use one public command plane.",
  falsifier: "Either actor requires a private mutation or produces different domain history.",
  decision_criteria: ["Both actor kinds commit contiguous attributable events"],
  experiment_bound: "One serialized Track A transaction",
};

const initialDecision: RecordDecision = {
  kind: "continue",
  question: "Should the exploration continue?",
  choice: "Continue through exact-commit verification.",
  alternatives: ["Stop the study"],
  rationale: "The walking-slice transaction produced committed evidence.",
  evidence: ["Immutable project activity"],
  consequences: ["Reprice contracts before Track B/C fan-out"],
};

export function App() {
  const client = useMemo(() => new WorkplaneClient(""), []);
  const [email, setEmail] = useState("human@workplane.local");
  const [password, setPassword] = useState("walking-slice-password");
  const [organizationID, setOrganizationID] = useState(defaultOrganization);
  const [session, setSession] = useState<Session>();
  const [projectInput, setProjectInput] = useState(initialProject);
  const [project, setProject] = useState<Project>();
  const [decision, setDecision] = useState<Decision>();
  const [activity, setActivity] = useState<Activity[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("Authenticate to begin the public M1 transaction.");

  useEffect(() => {
    const projectID = new URLSearchParams(window.location.search).get("project");
    if (!projectID) return;
    void readExploration(client, projectID)
      .then(({ project: loaded, activity: events }) => {
        setProject(loaded);
        setActivity(events);
        setMessage("Committed project restored from the server.");
      })
      .catch(() => { setMessage("Log in to restore this committed project."); });
  }, [client]);

  async function submitLogin(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    await act(async () => {
      const authenticated = await client.login({}, { email, password });
      setSession(authenticated);
      setMessage("Human session established. CSRF protection is active.");
    });
  }

  async function submitProject(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!session) return;
    await act(async () => {
      const created = await createExploration(client, organizationID, session.csrf_token, projectInput);
      setProject(created);
      setDecision(undefined);
      const loaded = await readExploration(client, created.id);
      setActivity(loaded.activity);
      window.history.replaceState(null, "", `?project=${encodeURIComponent(created.id)}`);
      setMessage("Exploration project and project.created committed atomically.");
    });
  }

  async function submitDecision(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!session || !project) return;
    await act(async () => {
      const result = await continueExploration(client, project, session.csrf_token, initialDecision);
      setDecision(result.decision);
      setProject(result.project);
      setActivity(result.activity);
      setMessage(`Continue decision committed at ${result.version}.`);
    });
  }

  async function act(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } catch (error) {
      if (error instanceof WorkplaneProblem) {
        const payload = error.payload as { title?: string; code?: string };
        setMessage(`${payload.title ?? "Request failed"} (${String(payload.code ?? error.status)})`);
      } else {
        const reason = error instanceof Error ? error.message : "unknown transport error";
        setMessage(`The public API request failed: ${reason}. No browser-local mutation was kept.`);
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <main>
      <header className="hero">
        <p className="eyebrow">Workplane · Track A · M1</p>
        <h1>One project plane for human and agent work.</h1>
        <p className="lede">
          This production path uses the generated public client. PostgreSQL—not the browser—is authoritative.
        </p>
        <p className="status" role="status" aria-live="polite">{busy ? "Working…" : message}</p>
      </header>

      <section aria-labelledby="login-heading">
        <div className="section-heading"><span>01</span><h2 id="login-heading">Human session</h2></div>
        <form onSubmit={(event) => { void submitLogin(event); }}>
          <label>Email<input type="email" value={email} onChange={(event) => { setEmail(event.target.value); }} required /></label>
          <label>Password<input type="password" value={password} onChange={(event) => { setPassword(event.target.value); }} required /></label>
          <button disabled={busy || Boolean(session)}>{session ? "Authenticated" : "Log in"}</button>
        </form>
      </section>

      <section aria-labelledby="project-heading">
        <div className="section-heading"><span>02</span><h2 id="project-heading">Exploration contract</h2></div>
        <form onSubmit={(event) => { void submitProject(event); }}>
          <label>Organization ID<input value={organizationID} onChange={(event) => { setOrganizationID(event.target.value); }} required /></label>
          <label>Title<input value={projectInput.title} onChange={(event) => { setProjectInput({ ...projectInput, title: event.target.value }); }} required /></label>
          <label className="wide">Outcome<textarea value={projectInput.outcome} onChange={(event) => { setProjectInput({ ...projectInput, outcome: event.target.value }); }} required /></label>
          <label className="wide">Hypothesis<textarea value={projectInput.hypothesis} onChange={(event) => { setProjectInput({ ...projectInput, hypothesis: event.target.value }); }} required /></label>
          <label className="wide">Falsifier<textarea value={projectInput.falsifier} onChange={(event) => { setProjectInput({ ...projectInput, falsifier: event.target.value }); }} required /></label>
          <button disabled={busy || !session}>Create through public API</button>
        </form>
      </section>

      {project && <section aria-labelledby="decision-heading">
        <div className="section-heading"><span>03</span><h2 id="decision-heading">Committed projection</h2></div>
        <dl className="project-card">
          <div><dt>Project</dt><dd>{project.title}</dd></div>
          <div><dt>Mode / state</dt><dd>{project.mode} / {project.state}</dd></div>
          <div><dt>Version</dt><dd>{project.version}</dd></div>
          <div><dt>Actor result</dt><dd>{decision ? `${decision.actor_kind} · ${decision.actor_id}` : "Awaiting decision"}</dd></div>
        </dl>
        <form onSubmit={(event) => { void submitDecision(event); }}><button disabled={busy}>Record continue decision</button></form>
      </section>}

      <section aria-labelledby="activity-heading">
        <div className="section-heading"><span>04</span><h2 id="activity-heading">Immutable activity</h2></div>
        {activity.length === 0 ? <p className="empty">No committed events yet.</p> : <ol className="activity">
          {activity.map((event) => <li key={event.event_id}>
            <strong>{event.event_type}</strong>
            <span>v{event.aggregate_version} · {event.actor_kind} · {event.actor_id}</span>
            {event.principal_id && <small>delegated principal {event.principal_id}</small>}
          </li>)}
        </ol>}
      </section>
    </main>
  );
}
