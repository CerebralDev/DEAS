# Neural interaction primitives

## Compact visual contract

The AiO installer is the first small, left-starting bubble. Standard bubbles are 144px wide with an 82px baseline height, showing only the heading and a mini status. Hover, keyboard focus or first touch reveals the description inside pre-reserved, naturally wrapping space in the existing bubble/choice button. Width, height and surrounding geometry do not change. There are no separate help bubbles, popups or portals. Following choices stay in place. Shared ResizeObserver measurements still update endpoints on actual viewport/content changes. Selection remains a single click/tap. Selecting dismisses help, and a stationary pointer must not automatically disclose a future decision that moves beneath it.

Active nodes follow short responsive snake rows. Single- and multi-choice decisions share one staircase-shaped group rather than adding a separate full-size bubble per alternative. Earlier selected decisions recede into a semitransparent overlapping history rail. Each heading remains keyboard/mouse accessible and returns to that decision. Reopening alone preserves downstream state; only actual changed values invalidate it.

Keep the language selector and chrome small. Do not make the map a long scrolling form. Technical JSON, steps and logs use optional disclosures; actual native request/hardware summaries, blockers and install consent must remain visible.

## Atomic ownership

- `NeuralBlob`: measured non-button shell, selectable heading/status and enlarged contextual help.
- `NeuralHelp`: description text inside the existing clickable shape, with equivalent focus/touch disclosure and no additional bubble.
- `NeuralChoiceGroup` / `NeuralChoiceItem`: one reusable staircase decision with separate heading and option buttons; no nested buttons.
- `NeuralHistory`: bounded semitransparent returnable selection stack, using the same blob atom.
- `NeuralPath`: measured hierarchy road endpoints and directional light/paw/off motion.
- `NeuralIncludedFeatures`: passive caller-translated microbubbles for the included parts of any parent node/choice; no controls, callbacks or extra graph nodes.
- `NeuralGraph`: responsive snake packing, stable shell/option measurements and composition, without domain transitions.
- Automatic setup renders its own compact review controls. The retired guided `setupPresentation` projection is no longer used; the shared neural primitives remain available to their actual consumers.
- `AiOSetup`: actual state changes, explicit native IPC and inline controls, not connector geometry.

`NeuralNode` supports `choices?: NeuralChoice[]` and `selectionMode?: 'single'|'multi'`. Each choice carries its stable ID, heading, optional description/status, selected/disabled state and optional `suggests` target IDs. `NeuralGraph` accepts `history?: NeuralNode[]`; one `onSelect(id)` handles heading, choice and history actions.

## References and geometry

Measured shells expose `data-node`; group options expose `data-choice`, not a second top-level node. Roads expose `data-edge="from->to"`, `data-from` and `data-to`. Internal SVG IDs are independently unique per mount. History does not participate in active-road geometry. Hidden descriptions and badge slots retain their layout footprint. Hover/focus/selection only change visibility and colour; history no longer uses transform/recede animation. Actual viewport changes refresh road bounds through ResizeObserver. Suggested metadata renders badges only. No recommendation SVG, road legend or animation is rendered; hierarchy roads remain.

## Motion and access

Light, paws and off remain shared road modes. Honour reduced motion for roads, help and history stacking. Do not impersonate installation progress. Buttons have stable heading-only accessible names and `aria-describedby` help; labels/errors remain readable without colour/motion. Closing an inline studio panel returns focus to its node. Road SVGs are noninteractive and hidden from accessibility APIs.

## Suggested and detected metadata

Choices and nodes carry independent `suggested?: boolean` and `detected?: boolean` flags. `NeuralFlags` renders localized Empfohlen/Suggested and Erkannt/Detected badges inside existing buttons. Recommendations never draw connection lines or auto-select their targets.

`native_detect_hardware()` performs bounded read-only discovery. Genuine NVIDIA discovery marks that backend detected/recommended and may preselect it only when applicable and untouched. Manual/restored choices, prepared plans and running jobs cannot be overridden. Detected metadata persists in returnable history. The model supports detected feature choices too, but this slice does not scan existing ComfyUI installations for installed feature packs; no offered feature is falsely labelled detected. AMD/Intel discovery is not implemented. Discovery is not installation, compatibility or generation acceptance.

## Explicit confirmation and included packages

`SetupState.confirmed` records confirmed decisions separately from selected values. `select` stores a choice; `confirm` alone progresses. The compatibility-only `choose` event is not used by the UI or discovery. One persistent green progression button uses `setup.confirmStep` (Bestätigen & weiter / Confirm & continue) in a normal-flow footer at the lower-right edge of the current max-width selection/control area, never at the viewport corner, including inputs and the architecture step with extra options. Input forms connect to the same external button via the HTML form attribute, preserving validation and Enter submission. A detected backend is prefilled, not confirmed. History reopening preserves values; confirming an unchanged reopened step moves to its natural next step. Actual edits invalidate affected confirmations and native consent. Drafts persist unfinished selections; old v2 drafts infer their historical confirmations.

Director is a single selectable package including Forge, RefMods, Continuity and Output. `included?: {id,label}[]` can be supplied to any NeuralNode/NeuralChoice/NeuralBlob; labels are caller-translated, and items expose `data-included-feature` without buttons or graph node IDs. The studio map retains Engine, Director and Plugins instead of duplicating included parts as main nodes. Effective provisioning includes director/forge/refmods/continuity; Output is presentation-only. Native planning expands incomplete Director requests before installation consent. Package inclusion is not runtime readiness.

## Native directory selection

The native picker chooses an existing parent directory. The user then names an explicit NEW child environment; native provisioning rejects existing targets. Cancellation preserves the draft. Browser mode truthfully disables the native chooser while allowing manual absolute paths. Choosing a folder never creates files, downloads packages or starts installation.

## Safety and proof

The native installer is Rust, not an external Go sidecar. Local planning, native plan confirmation and actual provisioned status remain separate. Preserve native immutable plan/consent invalidation and owned-job safeguards during layout changes.

Run unit/model tests plus `tools/verify_ui.py` at 1440/1024/768/390/320. Verify stable geometry on real hover/focus/selection, constant bubble/node count and contained text, grouped stairs, every prior decision's return path, unchanged plan roundtrips, no implicit feature activation and no backend requests. Browser tests do not substitute for native WebKit rendering or native dialog runtime tests.
