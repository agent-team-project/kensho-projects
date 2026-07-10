# Kensho Feedback Route

This public snapshot keeps agent feedback local to the project runtime. Private
feedback records are not published; durable findings are summarized in the
evaluation documents.

Use this command from the repo root:

```sh
agent-team feedback submit \
  --route local \
  --category friction \
  "One sentence describing the Kensho issue"
```

Categories are `friction`, `bug`, `idea`, `docs`, and `incident`.

Feedback should be concrete and candid. Include what felt confusing, brittle,
slow, emotionally uncomfortable, surprisingly smooth, or trust-building, and
name the command, prompt, pipeline step, or file involved. Keep the submitted
item to one dense sentence so it can be clustered and triaged upstream.

The daemon is not required for feedback submission; the route writes ignored,
file-backed feedback state under this project.
