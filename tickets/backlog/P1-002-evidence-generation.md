ID:
P1-002

TITLE:
Automate Evidence Generation

STATUS:
BACKLOG

OWNER:
UNASSIGNED

PROBLEM:

Evidence records currently require manual collection
and manual copy/paste operations.

This creates duplicated effort and increases the
risk of inconsistency between project state and
recorded evidence.

DELIVERABLES:

Evidence generation command

Automated stage inventory generation

Automated project snapshot generation

Automated ticket snapshot generation

ACCEPTANCE CRITERIA:

Command exists:

.\tools\op.ps1 evidence

Generates:

evidence/source/current-stage-inventory.md

Generates stage inventory directly from:

src/stages/

No manual file listing required

Generated evidence is reproducible from
current project state

FILES:

tools/op.ps1

tools/commands/

evidence/source/

DEPENDENCIES:

NONE

NOTES:

Source of truth must remain the filesystem.

Humans should not manually duplicate
information that can be derived from:

- tickets
- status
- findings
- decisions
- project structure

DECISION REQUIRED:

NO