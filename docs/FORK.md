# CerebralDev DEAS fork

This is an independent public fork of [darksidewalker/DEAS](https://github.com/darksidewalker/DEAS), not an official release. Upstream authorship, GPL-3.0-or-later licensing and third-party credits are retained. No upstream PR or author notification was sent as part of establishing this fork.

## Branch policy

- `main` mirrors upstream without local changes. The verified preparation baseline is `0c2b1083d47ecc7c210b09f75ebedf90fbba1414`; the prior local baseline was v0.2.22 (`88fc7bca19ed2ef8ca8098091aac49019fd3fe3a`). The intervening upstream change only updates verification documentation.
- `codex/workspace-integration` is the default development branch. It combines reviewed modules and separate fork-maintenance commits.
- Feature branches and exact adoption ranges are in [UPSTREAM-MODULES.md](UPSTREAM-MODULES.md). Dependent modules are explicitly based on their prerequisite branch.
- Small shared workspace diagnostics/selection and browser-fixture commits are bundled where needed. They are not dependencies on the Chat feature. Cherry-pick a shared commit once; merging branch history recognizes it automatically.
- Do not force-push published branches, overwrite upstream tags or label a fork binary as an official DEAS release. Use `fork-*` tags if later authorized. Binary releases and scheduled data publication are disabled initially; verification CI remains enabled.
- Publish explicit reviewed branch refs only, never `git push --mirror`; private stashes and preservation refs must remain local.

## Portable setup and development

```sh
git clone https://github.com/CerebralDev/DEAS.git
cd DEAS
git remote add upstream https://github.com/darksidewalker/DEAS.git
git remote set-url --push upstream DISABLED
git switch codex/workspace-integration
npm ci
npm run typecheck
npm run test:core
npm run test:ui
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo build --manifest-path src-tauri/Cargo.toml --locked --release
```

Read AGENTS.md and docs/PROJECT-RULES.md first. Use the documented Windows Rust/WebView2 or Linux GTK/WebKit prerequisites in [GETTING-STARTED.md](GETTING-STARTED.md). Layout changes also require `npm run test:layout` and the [paired-output gate](WORKSPACE-VERIFICATION.md). Keep platform-native runtime evidence separate from Chromium tests. Credentials belong only in protected application storage; no real API calls or rendering are required for fixture tests.

## Manual upstream synchronization

Start with a clean tree, preserve uncommitted work privately and verify upstream's current SHA. Never infer the installed build from Git HEAD.

```sh
git fetch upstream --prune
git switch main
git merge --ff-only upstream/main
git push origin main
git switch codex/workspace-integration
git merge main
```

If the mirror cannot fast-forward, stop and inspect rather than reset. Resolve and test integration conflicts, then publish. Keep feature branches stable for review; port fixes deliberately or create a new revision branch. A source push never authorizes a deploy, restart, inference, download or publication. Upstream PR descriptions are prepared in [upstream-proposals](upstream-proposals/); submitting them needs a separate instruction.

## Installed-build provenance and rollback

The installation was deliberately left on the pre-fork custom v0.2.22 build. Its recorded executable SHA-256 is `b8bdf7acb7a4ee11b13f4486a20a0d958b93dd675278633d12e3d2069b67e63d`. The source packaging is not a new installed release or a claim of binary reproducibility.

Package versions and the built-in upstream update behavior are preserved. Fork-specific versioning, signing and update-feed policy must be reviewed before any future fork binary release; this setup does not redirect an installed updater.

The operator retains a private pre-fork source snapshot, tracked patch, per-file hashes, installed-build receipt and local working-tree preservation record. Their machine-specific paths, credentials, media and operational logs are excluded from this repository. Fork verification outcomes are in [FORK-VERIFICATION.md](FORK-VERIFICATION.md).

Rolling back source does not roll back application data. Preserve newer organization records, conversation history, settings and media. If an older executable cannot understand an added provider, restore only its backed-up provider configuration after retaining the newer copy. Do not restore an entire profile or delete new data. Repository setup leaves running services and model files untouched.
