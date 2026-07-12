# Public Workplane topology

This directory contains the minimal repository-relative definitions needed to
review and reproduce the serialized `research_slice`: four role prompts, one
pipeline, its budgets/WIP, manual integration seam, exact smoke command, and
source provenance.

It is intentionally not a copy of the operator deployment. There is no bundled
template lock because these definitions are a project-specific public subset,
not an upgradeable initialized runtime. Credentials, tokens, daemon state,
jobs, events, mailboxes, worktrees, outcomes, and machine-local paths are never
published. A local operator supplies those outside this package.
