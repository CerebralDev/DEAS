# Workspace browser fixtures

The shared viewport gate retains upstream checks and its negative control. It updates the expected navigation for the already-existing Analysis tab and supplies read-only IPC fixtures. Module-specific dock and 600-resource hierarchy assertions run when those modules are present in the checkout. Missing controls then fail rather than silently skip.

Run npm run test:layout and uv run --no-project --with playwright==1.64.1 python tools/verify_workspace_paired.py. The paired gate checks mixed, empty and rooted media collections with pointer interaction. No real backend, media, models or inference is used. This fixture repair is bundled with each layout module; it is not a product dependency.
