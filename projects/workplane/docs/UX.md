# User Experience Contract

## 1. Experience goals

Workplane is a work surface, not a marketing site. It should feel quiet, dense,
predictable, and fast under repeated use. The interface optimizes for:

- scanning and comparing projects;
- understanding one project's contract without opening several panels;
- moving between brief, deliverables, work, evidence, and decisions;
- recognizing forecast uncertainty and material change;
- acting through keyboard or pointer with equal completeness; and
- making human and agent actions equally legible.

The application opens directly to the last portfolio, not a landing page.

## 2. Navigation

### 2.1 Global shell

Desktop layout:

```text
+---------------------------------------------------------------+
| Org switcher | Search | Create | Inbox | Help | Actor menu    |
+---------------+-----------------------------------------------+
| Portfolio     | Main route                                    |
| Judgment      |                                               |
| Projects      |                                               |
| My work       |                                               |
| Reviews       |                                               |
| Decisions     |                                               |
| Automations*  |                                               |
| Settings      |                                               |
+---------------+-----------------------------------------------+
```

The sidebar is resizable and collapses to icons. It does not contain nested
card surfaces. The header remains one stable height. Global search and command
palette share a consistent result model.

`Automations` exists only after the ECA exploration is promoted. Core renders
no disabled entry, route, or feature flag for it.

### 2.2 Project navigation

Project routes are tabs below a compact project header:

- Overview
- Brief
- Deliverables
- Work
- Timeline
- Experiments (exploration mode only)
- Decisions
- Evidence
- Activity

Tabs are real routes with stable URLs. The project header shows key, title,
mode, state, owner, P50/P90 forecast, forecast freshness, and health signals. It never grows
into a hero or pushes the primary work below the first viewport.

## 3. Portfolio experience

### 3.1 Default table

The default portfolio view is a virtualized table with stable columns:

- rank;
- project/outcome;
- mode and state;
- owner;
- health;
- accepted deliverables;
- intent target;
- P50/P90 forecast;
- forecast freshness;
- blocking dependency; and
- last material change.

Users can resize/reorder optional columns and save personal views. Required
status columns cannot be removed from the canonical view.

Grouping and filters use menus/chips only while active. Common filters have
keyboard shortcuts through the command palette, not visible instructional text.

### 3.2 Priority review

Priority review uses a split comparison surface:

- left: ranked projects and structured inputs;
- right: selected project's outcome, rationale, dependencies, opportunity cost,
  and recent decisions.

Reordering requires a rationale for changed relative priority. Bulk reorder
previews the affected range and commits atomically. Suggested rank is visibly
advisory and exposes its formula/inputs.

### 3.3 Alternate views

- Board groups by health, state, mode, or owner.
- Timeline plots P50/P90 bands, intent targets, and typed external deadlines as
  distinct markers.
- Dependency view shows only cross-project blocking dependencies by default.

All views use the same query/filter state and preserve selection when switching.

## 4. Project overview

The overview is an unframed grid of full-width regions, not nested cards:

1. outcome and current mode/state;
2. P50/P90 forecast, freshness, target/deadline gaps, and last reforecast;
3. required deliverables and gate status;
4. exploration readiness or exploitation progress;
5. open blockers, risks, findings, and decisions due;
6. recent material change; and
7. owner/participants and next review.

Activity volume is secondary. A project with many task updates but no accepted
deliverable must look stalled, not busy.

## 5. Project creation

Creation is a focused full-page flow with two steps.

### Step 1: intent

- title;
- outcome or decision question;
- mode segmented control;
- owner;
- primary portfolio;
- priority rationale and inputs; and
- visibility.

### Step 2A: exploration contract

- hypothesis/unknown;
- falsifier;
- evidence sought;
- decision criteria;
- bounds;
- intent target; and
- P50/P90 forecast, review date, and basis.

### Step 2B: exploitation contract

- initial required deliverables;
- acceptance criteria;
- intent target;
- P50/P90 forecast, review date, and basis; and
- known dependencies.

Save as proposed is always available. Activate is enabled only when domain
requirements pass and clearly lists missing requirements.

## 6. Exploration workflow

### 6.1 Exploration overview

The main visualization is an evidence map:

- hypotheses/unknowns;
- experiments;
- observations/evidence;
- decision criteria; and
- current coverage/unresolved uncertainty.

It does not display percent complete.

### 6.2 Experiment execution

An experiment page shows question, method, evidence criteria, bounds, assigned
work, observations, and bound consumption. Crossing a threshold creates an
attention banner with actions `Review`, `Continue`, `Pivot`, or `Stop`; it never
silently disables data entry.

### 6.3 Decision checkpoint

The decision surface presents:

- question;
- active hypothesis;
- supporting and contradicting evidence;
- unresolved uncertainty;
- expected value of more information;
- alternatives; and
- consequences.

The command is a segmented choice: Continue, Pivot, Stop, Promote. Each choice
reveals only its required fields. Promotion requires initial deliverables and
accepted residual uncertainty before confirmation.

## 7. Exploitation workflow

### 7.1 Deliverables

The deliverables route uses a dense list with state, required flag, weight,
criteria coverage, gates, owner, and forecast impact. Selecting a deliverable
opens an unframed detail pane.

### 7.2 Review

Review places four regions in one view:

1. acceptance criteria;
2. submitted evidence with provenance;
3. open/prior findings and resolutions; and
4. verdict controls.

Approve is disabled if hard evidence requirements or independence policy fail.
Bounce requires at least one actionable finding linked to a criterion or marked
as contract-independent. Findings include severity, location/reference,
explanation, and required evidence.

### 7.3 Work

Table is the default. Board supports drag between allowed states and shows a
preview before committing batch moves. Timeline visualizes work dependencies but
does not replace project forecast.

Assignee controls support human and agent identities without separate lanes.
Agent identities display a small service badge and token/scoped-authority link.

## 8. Forecast workflow

Forecast is always a band, never a single unlabeled due date.

The reforecast dialog requires:

- P50/P90 dates;
- next review date;
- basis;
- assumptions;
- reason codes;
- impact; and
- optional linked scope decision.

The dialog displays the current forecast beside the proposal. After commit, the
timeline animates only if reduced motion is not requested and announces the
change to assistive technology.

Forecast history plots P50/P90 predictions, stale intervals, holds, target and
deadline changes, and actual completion. It provides raw values in an accessible
table.

## 9. Brief editor

Core briefs use versioned sections. Separate sections can be edited
concurrently; a stale same-section write opens a conflict surface with current
and attempted content. Every save produces an immutable revision.

The editor supports:

- headings 1-3;
- paragraphs;
- ordered/unordered lists;
- checklists;
- quotes;
- fenced code with language;
- simple tables;
- links;
- mentions;
- entity references; and
- inline comments.

No arbitrary embeds, HTML, executable code, layout columns, databases inside
documents, or plugin blocks in v1.

The toolbar uses familiar icons with tooltips. Keyboard shortcuts work but are
not advertised as persistent explanatory copy. Slash command inserts allowed
blocks. If the collaboration exploration is promoted, presence uses restrained
cursor/selection colors and actor names and section locks are replaced by CRDT
co-editing without changing structured project commands.

Structured project fields appear in a compact read-only inspector beside the
brief. Editing them opens normal domain command controls; direct text edits
cannot alter them.

Offline/disconnected brief state is explicit: `Reconnecting`, `Unsynced local
edits`, or `Synced`. Closing with unsynced edits prompts the user. Structured
project commands are disabled offline with last-confirmed time shown.

## 10. Decisions and evidence

Decisions use a chronological ledger with filters for kind, actor, and evidence.
Each entry shows choice first, then rationale, alternatives, evidence,
consequences, and supersession.

Evidence creation supports text observations, test/report metadata, links, and
small images. It captures producer, time, digest where applicable, and source.
The UI never lets a user overwrite evidence; correction creates a superseding
record and displays both.

## 11. Inbox and reviews

Inbox groups actionable items, not every event:

- assigned work;
- mention;
- review request;
- forecast/target attention;
- blocked dependency;
- experiment bound attention;
- automation failure; and
- permission/token event requiring action.

Items link to the exact action surface. Batch acknowledgement is allowed;
approvals and verdicts are never batch-acked.

### 11.1 Judgment queue

The judgment queue is the fleet-director surface. It contains only work waiting
for authorized judgment: promotion/pivot/stop, priority conflict, scope cut,
stale forecast, independent review, soft-gate or deliverable waiver, cancellation, token grant, and
other human-required decisions. It shows impact of waiting, age, project
outcome, recommendation/evidence, and the exact decision command. Activity noise
and ordinary assigned work do not appear here.

## 12. Search and command palette

Global search opens with one command and supports projects, work, deliverables,
decisions, evidence, comments, and brief text. Results show entity type,
project, safe snippet, and last change. Filters can be typed or selected.

The command palette includes navigation and commands the actor can perform. It
does not reveal unauthorized command names for hidden resources.

## 13. Empty, loading, and failure states

- Empty portfolio: primary action `Create project`; no marketing illustration.
- Empty project mode view: describe the missing contract in one concise line and
  provide the correct command.
- Loading: stable skeleton dimensions matching final layout.
- Permission denied: no leaked title or metadata.
- Version conflict: attempted/current values and `Reload`, `Copy my change`, or
  `Reapply` where semantically safe.
- Realtime disconnected: persistent status, last confirmed time, and retry.
- Automation failed: reason, triggering event, attempted command, retry policy.
- Search stale: indexed source time/version; direct resource state remains
  authoritative.

## 14. Responsive behavior

### Wide: 1280 px and above

Sidebar, main content, and optional detail pane can coexist. Tables retain
critical columns and horizontal scroll only inside the table region.

### Medium: 768-1279 px

Sidebar collapses; detail pane becomes a route/modal sheet. Tables hide optional
columns through explicit priority order.

### Narrow: below 768 px

The app supports portfolio review, project overview, inbox, comments, evidence,
review verdict, and reforecast. Complex board/timeline/editor surfaces use
purpose-built narrow layouts and horizontal panning where unavoidable. No text
or command is clipped. Full brief table editing may be read-only on narrow
screens, but text/comment editing remains available.

## 15. Accessibility

- WCAG 2.2 AA target.
- Semantic landmarks, headings, tables, grids, dialogs, tabs, and status.
- Complete keyboard operation and visible focus.
- Drag operations have menu/keyboard alternatives.
- Realtime and validation changes use appropriately scoped live regions.
- Color is never the only mode/state/health signal.
- Charts have accessible tables and do not require hover.
- Touch targets meet 44x44 CSS px where the narrow layout applies.
- Reduced motion and high-contrast preferences are honored.

## 16. Visual system

- Neutral light and dark surfaces with semantic green, amber, red, blue, and
  violet used sparingly across distinct meanings.
- No one-hue palette, gradients, decorative orbs, or marketing hero.
- Cards only for repeated entities or genuine framed tools; page sections remain
  unframed.
- Border radius at or below 8 px.
- Stable toolbar, row, board column, and timeline dimensions.
- Typography uses normal letter spacing and fixed responsive steps, not viewport
  font scaling.
- Icons come from Lucide with tooltips for unfamiliar actions.

## 17. Computer verification

Every release runs real rendered-browser verification against a populated
deployment:

- desktop 1440x900;
- compact desktop 1024x768;
- mobile 390x844;
- light/dark mode;
- 200% browser zoom for core workflows; and
- two simultaneous browser contexts for structured realtime, section conflicts,
  and promoted collaboration behavior.

Evidence includes screenshots, video for realtime flows, DOM accessibility
snapshot, console errors/warnings, failed requests, layout geometry, and exact
interaction assertions. Static/unit tests cannot substitute for this gate.

## 18. Golden paths

All screens serve three primary end-to-end paths:

1. **Human triage:** open judgment queue, compare evidence/impact, record a
   decision, and observe dependent work unblock.
2. **Agent execution:** receive assigned work through event stream, recover
   project context, update through public commands, submit evidence, and receive
   review without a private project plane.
3. **Promotion decision:** create exploration, preregister criteria, run
   experiments, inspect evidence, promote with residual uncertainty and initial
   deliverables, then see portfolio allocation change.

Release visual review must complete all three paths at desktop and narrow
viewports.
