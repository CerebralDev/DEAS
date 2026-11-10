# Apply & return

The H3 Forge workspace offers Apply & return immediately beside Apply. Both use the same validation path. Apply stays in Forge; Apply & return returns to Studio only after successful application. Neither queues work. Stale/busy results stay blocked, and existing history and Undo remain available.

Integration: optional ForgeButton.onReturn and Workbench.onReturnStudio callbacks; onApply may return false to reject application. No provider, library, or contextual Chat dependency. No persisted schema changes. Focused validation: src/features/forge/Forge.test.tsx, plus the shared layout gate.
