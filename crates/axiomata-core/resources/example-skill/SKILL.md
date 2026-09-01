---
name: example-skill
description: Built-in smoke-test skill. Reports the working directory and lists the top-level entries of the Second-Brain workspace, then stops.
backend: claude-code
---

# Example Skill

This is the example skill that ships with Axiomata-OS. Its only job is to prove
the skills runner works end to end — from the registry, through the agent
backend, to the run log.

When invoked, do exactly this and nothing more:

1. State the current working directory.
2. List the top-level files and folders in it, one per line.
3. Stop. Do not make any changes, do not run further commands.
