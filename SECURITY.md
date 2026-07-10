# Security policy

## Supported surface

Security fixes are considered for the current default branch. The projects are
local-first demonstrations and are not currently distributed as supported
production services.

## Reporting a vulnerability

Do not disclose a suspected vulnerability in a public issue. Use GitHub's
private vulnerability reporting flow from the repository Security tab. Include
the affected project, revision, reproduction steps, expected impact, and any
suggested mitigation.

Never include real credentials, private workbook data, agent transcripts, or
other sensitive user data in a report.

## Secrets and generated state

The repository publication check rejects common credential formats, personal
absolute paths, dangerous Codex sandbox bypass flags, generated agent runtime
state, and oversized artifacts. This check is defense in depth, not a guarantee;
contributors remain responsible for reviewing every proposed public diff.
