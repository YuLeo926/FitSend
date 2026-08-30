# FitSend 0.3.0 Verified Send Plans Design

**Status:** Approved in conversation on 2026-08-30

**Scope:** Replace FitSend's single per-file destination ceiling with explicit send-plan rules that can constrain each file or the accepted batch total, while preserving the existing local-only quality strategies and sequential media processor.

## Product Promise

FitSend 0.3.0 does not merely make each selected file smaller. It applies the rule of the place where the files are going and proves the accepted result against that rule.

The user-facing promise becomes:

> Choose a destination. FitSend gives every file a fair budget, protects the selected quality floor, and verifies either every file or the whole accepted batch before it says the files are ready.

## Goals

- Represent upload rules explicitly as either a per-file ceiling or an aggregate batch ceiling.
- Make Discord, Gmail, Outlook, web upload, and custom destinations honest about what is being verified.
- Allocate an aggregate ceiling fairly across mixed image/video batches without using a naive equal-size split.
- Reallocate unused bytes from completed items to later items so discrete encoders do not cause avoidable compression.
- Preserve the three existing strategies: Precise Fit, Balanced, and Smallest Acceptable.
- Preserve every existing quality floor, media-integrity check, cancellation guarantee, and collision-safe output rule.
- Let users save named custom plans locally without an account or network request.
- Show the rule source and verification date for every built-in provider plan.
- Keep the default workflow non-technical and no more complicated than choosing a destination, choosing a quality guardrail, and starting the batch.

## Non-goals

- Downloading destination rules from the network or silently changing a rule after installation.
- Globally re-encoding completed files to solve a mathematically optimal whole-batch quality problem.
- Moving processing orchestration completely out of the existing frontend queue.
- Per-item destinations or compression strategies inside one selected batch.
- Parallel video encoding.
- PDF, audio-only, archive, or Office-document support.
- Trimming, cropping, editing, codec controls, quality sliders, bitrate controls, or resolution controls.
- Cloud uploads, accounts, telemetry, or shared saved-plan synchronization.
- GPU encoder selection.

## Terminology

- **Published limit:** The provider's documented or observed limit shown for context.
- **Working ceiling:** The conservative byte count FitSend actually enforces.
- **Limit scope:** Whether the working ceiling applies to each file or the sum of all accepted files.
- **Allocation:** The maximum bytes assigned to one item in an aggregate plan.
- **Accepted bytes:** The real measured size of a created output or an unchanged source that FitSend has accepted.
- **Send plan:** A destination rule plus the selected compression strategy.

## Destination Rule Model

The frontend domain adds:

```ts
type LimitScope = "perFile" | "batchTotal";

type DestinationRule = {
  id: string;
  family: "discord" | "email" | "web" | "custom" | "saved";
  name: string;
  shortLabel: string;
  description: string;
  scope: LimitScope;
  publishedLimitLabel: string;
  maxBytes: number;
  acceptedKinds: MediaKind[];
  sourceLabel: string;
  sourceUrl: string | null;
  verifiedOn: string | null;
  ruleNote: string;
  accent: "coral" | "blue" | "ink";
  builtIn: boolean;
};
```

`maxBytes` is always the working ceiling. Processing and verification never use the human-readable published label.

The Rust core adds matching serialized concepts for `LimitScope`, `BatchBudgetRequest`, `BatchBudget`, and `ItemAllocation`. Provider names and source URLs remain frontend data; the Rust core receives only the rule semantics and analyzed media required for deterministic allocation.

## Built-in Plans

All limits below use the app's existing binary size units (`1 MB = 1,048,576 bytes`) for working ceilings.

| ID | UI name | Scope | Published context | Working ceiling | Source checked 2026-08-30 |
|---|---|---|---|---:|---|
| `discord-safe` | Discord Free — Safe | Per file | Discord documentation currently exposes both 10 MB account tables and a 20 MB rollout FAQ | floor(9.8 MiB) | Discord File Attachments FAQ and Account Caps pages |
| `discord-basic` | Discord Nitro Basic | Per file | 50 MB | 49 MiB | Discord File Attachments FAQ |
| `discord-nitro` | Discord Nitro | Per file | 500 MB | 490 MiB | Discord File Attachments FAQ |
| `gmail-personal` | Gmail personal | Batch total | 25 MB total attachments | 24 MiB | Gmail Help: Send attachments with your Gmail message |
| `outlook-internet` | Outlook internet email | Batch total | 20 MB message limit, including the message | 18 MiB | Microsoft Support: Reduce attachment size to send large files with Outlook |
| `web-5mb` | Web upload | Per file | Product default rather than a provider guarantee | 5 MiB | No external source; labelled as a conservative generic form limit |
| `custom` | Custom plan | User selected | Exact user-entered rule | Default 10 MiB | User supplied |

The source URLs are:

- `https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ`
- `https://support.discord.com/hc/en-us/articles/33694251638295-Discord-Account-Caps-Server-Caps-and-More`
- `https://support.google.com/mail/answer/6584`
- `https://support.microsoft.com/en-US/Outlook/reduce-attachment-size-to-send-large-files-with-outlook`

The Discord Free plan is intentionally named **Safe**. FitSend must not claim that 10 MB is the only current free-account limit while Discord documents and experiments with different values. Users who know their account accepts a different ceiling can use a custom per-file plan.

Gmail Workspace and Exchange administrators can set different limits. The built-ins are explicitly named for the documented personal/internet cases; organizational users are directed to Custom rather than given a false guarantee.

## Saved Custom Plans

The Custom editor contains:

- A numeric limit and KB/MB unit selector.
- A two-option scope control: **Each file** or **All files together**.
- An optional plan name used only when the user chooses **Save plan**.

Validation rules are explicit:

- Working ceilings must be between 8 KiB and 10 GiB inclusive.
- Saved names are trimmed, must contain 1–40 visible characters, and are compared case-insensitively for uniqueness.
- Saving a duplicate name does not overwrite silently; the UI offers an explicit **Replace saved plan** action.
- At most 20 saved plans are retained. FitSend never silently evicts an older plan.

Saved plans use local WebView storage under the versioned key `fitsend.sendPlans.v1`. The stored object contains `schemaVersion: 1`, a stable UUID, name, scope, max bytes, creation time, and update time. Loading validates every entry independently; a malformed entry is ignored without discarding valid siblings. Nothing is synced or uploaded.

Deleting a saved plan is allowed only while configuration is unlocked. Deleting it does not alter an already planned or completed batch.

## Batch Budget Semantics

### Per-file scope

Every runnable item receives the destination's complete `maxBytes` as its target. This preserves the 0.2.x behavior and all existing media-processing tests.

The final proof is valid only when every accepted item individually measures at or below `maxBytes`.

### Batch-total scope when the source batch already fits

If the sum of all valid source sizes is at or below the working ceiling, each item's initial allocation equals its source size.

- Precise Fit returns unchanged sources.
- Balanced may create a result only under its existing meaningful-savings and quality rules.
- Smallest Acceptable may continue to seek a smaller acceptable result.

Because every accepted result is no larger than its allocation, the aggregate proof remains valid.

### Batch-total scope when compression is required

The allocator uses deterministic capped proportional water-filling:

1. Reserve `min(sourceBytes, 8 KiB)` for every valid item. A source smaller than 8 KiB is capped at its source size and never enlarged.
2. If the sum of those per-item reserves exceeds the working ceiling, reject the batch plan before processing.
3. Distribute the remaining bytes in proportion to each item's reducible source bytes: `max(sourceBytes - min(sourceBytes, 8 KiB), 1)`.
4. Cap an item's allocation at its source size; redistribute bytes released by a cap among uncapped items using the same weights.
5. Resolve fractional bytes with the largest-remainder method. Ties follow the user's selection order, making output deterministic.

This produces approximately the same required reduction ratio across the batch while avoiding the worst failure of an equal split, where a small image and a long video receive identical budgets. Source byte size already incorporates duration, resolution, frame rate, and source bitrate for video, so it is the primary demand signal. Media-specific quality and feasibility remain the responsibility of the existing strategy planner and processor.

The allocator is not allowed to assign more than the source size, less than `min(sourceBytes, 8 KiB)`, or a set of allocations whose sum exceeds the aggregate working ceiling.

### Forward reallocation

After each terminal item, the queue asks the Rust budget engine to recalculate allocations for waiting items:

- Created output consumes its actual measured output bytes.
- No-change output consumes its unchanged source bytes.
- Failed or cancelled items consume zero because they are not part of the accepted sendable subset.
- The remaining working budget is redistributed across waiting items with the same capped proportional algorithm.

Because encoders often land materially below a target, later files can receive a larger allocation and preserve more quality. A recalculation may increase a waiting allocation but must never reduce it below the allocation already displayed for that item.

FitSend 0.3.0 does not go backwards and re-encode a completed file, and it does not retry an earlier quality-floor failure after later files free more space. This keeps work bounded and predictable. A future global optimizer may add backward improvement passes only as a separate design.

### Partial success

Every plan proves only the accepted subset. If any selected item fails or is cancelled:

- Successful rows remain usable.
- The summary uses an amber partial-result state, not the green whole-batch-ready state.
- An aggregate proof says, for example, `4 accepted files total 18.7 MB — verified under 24 MB; 1 file needs attention`.
- A per-file proof says, for example, `4 accepted files are each under 9.8 MB; 1 file needs attention`.
- Failed input bytes are not included in the accepted total.

FitSend never implies that all selected files can be sent when one or more files failed, regardless of limit scope.

## Architecture

### Frontend destination registry

`src/domain/profiles.ts` becomes the source of built-in presentation data and custom-plan conversion. It does not allocate batch bytes.

Responsibilities:

- Return a validated `DestinationRule` by ID.
- Group rules by Discord, Email, Web, Custom, and Saved families.
- Expose source and verification metadata.
- Validate, serialize, and deserialize saved custom plans through a separate focused persistence module.

### Rust batch budget engine

A new focused core module, `src-tauri/core/src/batch_budget.rs`, owns aggregate allocation and reallocation.

Public operations:

- `build_budget(request) -> BatchBudget`
- `rebalance_budget(request) -> BatchBudget`

The module depends only on analyzed source sizes, stable item IDs/order, scope, ceiling, and accepted actual sizes. It does not know provider names, UI state, filesystem paths beyond stable identifiers, or encoding details.

`BatchBudget` returns:

- Rule scope and working ceiling.
- Accepted bytes already consumed.
- Remaining bytes.
- Ordered item allocations.
- Feasibility and a plain-language blocking reason when infeasible.

### Tauri command boundary

The desktop shell adds `build_batch_budget` and `rebalance_batch_budget` commands. Existing `build_plan`, `process_media`, progress events, and cancellation remain intact.

The new command boundary keeps allocation policy in Rust while allowing the existing React hook to retain scheduling and row-level progress.

### Frontend queue

`useBatchQueue` receives a complete destination rule rather than a single `targetBytes` number.

The queue flow becomes:

1. Analyze every selected path as today.
2. Wait until every selected item is analyzed or terminal before starting.
3. Request an initial batch budget for all valid waiting items.
4. Store `allocationBytes` on each waiting `BatchItem`.
5. Pass the current allocation to the existing single-file plan and processor.
6. Accept only verified processor results.
7. Rebalance remaining allocations after each created, unchanged, failed, or cancelled item.
8. Continue sequentially until all items are terminal.

The selected destination rule and strategy are locked as soon as an initial batch budget or media plan exists, matching the existing immutable-proof behavior.

### Batch item and totals

`BatchItem` adds `allocationBytes: number | null`. The UI may display this as `Budget ≤ 6.4 MB` after the initial budget is built.

`BatchTotals` adds:

- `acceptedFiles`
- `acceptedBytes`
- `limitScope`
- `workingCeilingBytes`
- `allSelectedAccepted`
- `proofValid`

`proofValid` means:

- Per-file: every accepted result is individually within the ceiling.
- Batch-total: the sum of accepted bytes is within the ceiling.

It does not mean every selected item succeeded; `allSelectedAccepted` carries that separate fact.

## User Experience

The existing warm editorial visual direction, asymmetric main/aside layout, typography, coral action color, and local-privacy promise remain. This is an information-architecture refinement rather than a redesign.

### Destination selection

The right rail continues to use Step 1, but avoids seven equally prominent cards:

- Top-level choices are Discord, Email, Web upload, Custom, and Saved.
- Discord expands an inline tier selector for Free — Safe, Nitro Basic, and Nitro.
- Email expands an inline provider selector for Gmail personal and Outlook internet email.
- Saved appears only when at least one custom plan exists.

Every active rule shows a compact receipt containing:

- `Each file` or `All files together`.
- The working ceiling.
- Published-limit context.
- `Checked 30 Aug 2026` for built-ins with sources.
- A source link or an explicit `FitSend generic default` label.

The strategy remains Step 2 and retains the current three options.

### Send plan panel

Before processing, the main plan route reads differently by scope:

- Per file: `Discord Safe → each file ≤ 9.8 MB → Balanced`.
- Batch total: `Gmail personal → all files ≤ 24 MB → Balanced`.

Once the budget is available, aggregate-plan rows show their assigned maximum. Reallocations update waiting rows without animation that implies completed files are changing.

### Result proof

Per-file success:

> 5 files ready for Discord Safe. Every accepted file is verified under 9.8 MB.

Aggregate success:

> 5 files ready for Gmail. 23.6 MB total — verified under 24 MB.

Partial aggregate result:

> 4 accepted files total 18.7 MB — verified under 24 MB. 1 file needs attention.

The summary retains original total, accepted total, saved bytes, new outputs, kept originals, and failures. It must not say `Every file` when failed or cancelled rows exist.

## Error Handling

- Invalid built-in rule data is a development error covered by tests; the production fallback is Discord Safe.
- Invalid saved entries are ignored individually and never crash startup.
- A custom ceiling outside 8 KiB–10 GiB disables the primary action with a direct validation message.
- An aggregate ceiling smaller than the sum of `min(sourceBytes, 8 KiB)` across valid items is rejected before processing.
- Unsupported, corrupt, unreadable, or missing files keep the existing row-local failure behavior.
- A strategy that cannot meet one item's allocation without crossing its quality floor fails that row and continues; it never borrows by violating another waiting allocation.
- Real output over its assigned target is rejected by the existing processor and never counted as accepted.
- A rebalanced budget that cannot be calculated is a batch-level stop: completed outputs remain on disk, waiting items become failed with the same clear reason, and no invalid proof is shown.
- Cancellation stops the active process, removes temporary output, marks remaining rows cancelled, and proves only the already accepted subset.
- Saved-plan storage write failure leaves the unsaved custom settings active for the current batch and reports that the plan could not be saved.

## Testing

### Frontend domain tests

- Built-in IDs are unique and every rule has valid scope, ceiling, source label, and accepted media kinds.
- Provider families resolve the correct tier/provider child.
- Custom limits enforce the 8 KiB–10 GiB range.
- Saved-plan serialization round-trips valid entries.
- Malformed saved entries are ignored independently.
- Duplicate names and the 20-plan maximum are enforced without silent replacement or eviction.
- Summary copy distinguishes per-file, aggregate-complete, and aggregate-partial proofs.
- Per-file partial results also avoid whole-batch-ready wording.
- Configuration locks after a budget exists.

### Rust budget tests

- Per-file scope assigns the full ceiling independently.
- Aggregate allocations never exceed the ceiling and never fall below `min(sourceBytes, 8 KiB)` when feasible.
- A source batch already under the ceiling receives source-sized allocations.
- Capped water-filling redistributes bytes from small files.
- Largest-remainder ties are deterministic by selection order.
- Mixed small images and large videos receive proportional rather than equal allocations.
- Rebalancing consumes actual accepted bytes and forwards unused bytes.
- Waiting allocations do not shrink after successful forward reallocation.
- Failed and cancelled items consume no accepted budget.
- An impossible minimum aggregate fails before processing.
- Allocation multiplication and proportional division use `u128` intermediates, so integer arithmetic cannot overflow at the supported 10 GiB ceiling; selected file count itself remains bounded only by available memory and the existing picker.

### Existing media regression

All 12 frontend tests, 30 core tests, and 56 real-media acceptance scenarios from 0.2.1 remain mandatory. The known 2.1 MB-to-2 MB image regression and all quality-floor thresholds remain unchanged.

### New batch acceptance scenarios

Add at least these twelve cases:

1. Multiple images already under a Gmail aggregate ceiling.
2. Multiple images requiring aggregate compression.
3. Multiple videos requiring aggregate compression.
4. Mixed image/video aggregate compression.
5. Small file capped at source size with its unused allocation redistributed.
6. Encoder result below allocation increasing later allocations.
7. Aggregate target below the sum of the per-item `min(sourceBytes, 8 KiB)` reserves.
8. One quality-floor failure among otherwise successful items.
9. One corrupt file among otherwise successful aggregate items.
10. Cancellation during an aggregate batch.
11. Partial-result proof excluding failed bytes.
12. Per-file Discord behavior remaining identical to 0.2.1.

Every aggregate success must assert the sum of actual accepted file sizes, not the sum of estimates or allocations.

## Migration and Compatibility

- Existing users have no persisted destination state to migrate.
- The old `discord`, `gmail`, `web`, and `custom` IDs are mapped respectively to `discord-safe`, `gmail-personal`, `web-5mb`, and `custom` when encountered in development state.
- Single-file use remains a one-item batch. For a one-item aggregate plan, batch-total and per-file verification are mathematically equivalent, but the UI still uses the selected provider's correct wording.
- Output filenames, source preservation, FFmpeg discovery, bundled Windows packaging, and cancellation cleanup remain unchanged.

## Release Boundary

This ships as FitSend `0.3.0` because it changes destination semantics and the public proof contract.

The release is complete only when:

- Package, Tauri, and Cargo versions agree on `0.3.0`.
- README destination and batch descriptions no longer claim every rule is per-file.
- Existing and new tests pass.
- The packaged Windows application passes picker, drag/drop, video-tool, aggregate-proof, and original-file smoke tests.
- MSI, NSIS, portable ZIP, SHA-256 sums, and exact FFmpeg source/license materials are regenerated from the release commit.
- Release notes identify unsigned-installer behavior and the static verification dates of provider rules.

## Deferred Follow-ups

- Signed or versioned remote rule manifests with explicit user-controlled updates.
- Backward improvement passes that re-encode earlier files when final aggregate slack remains.
- Automatic app updates and code signing.
- Hardware encoder selection after real-world performance demand is measured.
