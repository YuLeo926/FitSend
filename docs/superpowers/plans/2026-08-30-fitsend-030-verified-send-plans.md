# FitSend 0.3.0 Verified Send Plans Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship FitSend 0.3.0 with source-backed destination rules, per-file and aggregate batch ceilings, fair forward-rebalanced byte budgets, saved custom plans, and scope-correct verification receipts.

**Architecture:** Keep provider metadata and saved-plan persistence in focused TypeScript modules, while adding a pure Rust `batch_budget` engine for deterministic capped proportional allocation. The existing React queue remains the sequential scheduler: it asks Rust for an initial budget, processes one item through the existing verified media pipeline, and rebalances waiting allocations using actual accepted bytes after every terminal result.

**Tech Stack:** React 19, TypeScript 5.8, Vitest 3, Tauri 2, Rust 2021, `image` 0.25, bundled FFmpeg/FFprobe, PowerShell acceptance and Windows packaging scripts.

**Spec:** `docs/superpowers/specs/2026-08-30-fitsend-030-verified-send-plans-design.md`

## Global Constraints

- Limit scope is exactly `perFile` or `batchTotal`; UI copy must never blur the distinction.
- Built-in working ceilings are Discord Safe `floor(9.8 MiB)`, Nitro Basic `49 MiB`, Nitro `490 MiB`, Gmail personal `24 MiB` aggregate, Outlook internet email `18 MiB` aggregate, and generic web upload `5 MiB` per file.
- Custom working ceilings are between `8 KiB` and `10 GiB` inclusive.
- Aggregate allocation reserves `min(sourceBytes, 8 KiB)` per valid item, uses capped proportional water-filling, and resolves fractional ties by selection order.
- Forward reallocation may increase waiting allocations but must never reduce an allocation already shown to the user.
- Completed files are never re-encoded and earlier failures are never retried in 0.3.0.
- Failed and cancelled items consume zero aggregate accepted bytes; partial summaries must not claim the entire selected batch is ready.
- Preserve Precise Fit, Balanced, and Smallest Acceptable behavior and all existing SSIM floors.
- Process one file at a time; preserve original files, transparency, orientation, duration, audio, cleanup, and collision-safe output behavior.
- Saved plans remain local under `fitsend.sendPlans.v1`, use schema version `1`, and are limited to 20 unique case-insensitive names of 1–40 visible characters.
- Provider rules are static in this release; no rule download, telemetry, cloud upload, or account is allowed.
- Release version is `0.3.0` in npm, Tauri, both Cargo manifests, Cargo.lock, README, and the app footer.

---

## File Structure

### New files

- `src/domain/savedPlans.ts` — validate, parse, mutate, and persist version-1 named custom plans.
- `src/domain/savedPlans.test.ts` — malformed storage, name collision, replacement, deletion, and capacity tests.
- `src/domain/budget.ts` — pure TypeScript conversion between queue items and Rust budget requests/results.
- `src/domain/budget.test.ts` — request construction, allocation application, accepted-byte, and proof tests.
- `src/domain/batch.test-fixtures.ts` — complete reusable queue-item builders shared by batch and budget tests.
- `src/hooks/useSavedPlans.ts` — React state wrapper around the tested saved-plan persistence functions.
- `src/components/DestinationPicker.tsx` — destination family and Discord/email child selection.
- `src/components/CustomPlanEditor.tsx` — custom limit, scope, validation, save, replace, and delete controls.
- `src/components/DestinationPicker.test.tsx` — server-rendered semantic/copy coverage without a browser dependency.
- `src-tauri/core/src/batch_budget.rs` — deterministic initial and forward-rebalanced allocation engine.
- `docs/releases/0.3.0.md` — release-note source covering scope semantics, provider check dates, and unsigned Windows packages.

### Modified files

- `src/domain/types.ts` — destination rule, saved-plan, limit-scope, and batch-budget command contracts.
- `src/domain/profiles.ts` — replace four generic profiles with grouped source-backed rules and custom conversion helpers.
- `src/domain/domain.test.ts` — destination fallback and custom range regression.
- `src/domain/batch.ts` — allocation state, accepted totals, proof state, and scope-correct summary copy.
- `src/domain/batch.test.ts` — aggregate/per-file proof, partial result, locking, and allocation tests.
- `src/hooks/useBatchQueue.ts` — initial budget request, per-item allocation use, and post-result rebalancing.
- `src/components/FileQueue.tsx` — show waiting allocation and preserve row result actions.
- `src/components/BatchSummary.tsx` — complete/partial and per-file/aggregate proof variants.
- `src/App.tsx` — compose destination picker, saved plans, scope-aware route, queue, and receipt.
- `src/styles.css` — destination family tabs, child tiers, scope selector, rule metadata, saved-plan controls, allocation labels, and amber partial summary.
- `src-tauri/core/src/domain.rs` — serialized budget request/result contracts.
- `src-tauri/core/src/lib.rs` — register/export `batch_budget` and its public types/functions.
- `src-tauri/src/lib.rs` — expose initial and rebalance Tauri commands.
- `src-tauri/capabilities/default.json` — allow opening provider source URLs through the existing opener plugin.
- `src-tauri/core/examples/acceptance_matrix.rs` — add twelve aggregate batch cases using real generated media.
- `scripts/build-windows-bundled.ps1` — regenerate package hashes beside the three Windows artifacts.
- `README.md` — document rule scope, built-ins, saved custom plans, and aggregate proof semantics.
- `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/core/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json` — set version `0.3.0`.

---

### Task 1: Replace Generic Profiles with Verified Destination Rules

**Files:**
- Modify: `src/domain/types.ts`
- Modify: `src/domain/profiles.ts`
- Modify: `src/domain/domain.test.ts`

**Interfaces:**
- Produces `LimitScope`, `DestinationFamily`, and `DestinationRule`.
- Produces `builtInRules`, `ruleById`, `rulesForFamily`, `customRule`, `validateCustomLimit`, `scopeLabel`, and `scopeDescription`.
- Preserves `bytesFromCustomLimit(value, unit)` for existing callers and tests.

- [ ] **Step 1: Write failing destination-rule tests**

Replace the old profile fallback assertions and add exact rule coverage in `src/domain/domain.test.ts`:

```ts
import {
  builtInRules,
  customRule,
  ruleById,
  rulesForFamily,
  validateCustomLimit,
} from "./profiles";

it("defines unique source-backed rules with exact scopes and ceilings", () => {
  expect(builtInRules.map((rule) => [rule.id, rule.scope, rule.maxBytes])).toEqual([
    ["discord-safe", "perFile", Math.floor(9.8 * 1024 * 1024)],
    ["discord-basic", "perFile", 49 * 1024 * 1024],
    ["discord-nitro", "perFile", 490 * 1024 * 1024],
    ["gmail-personal", "batchTotal", 24 * 1024 * 1024],
    ["outlook-internet", "batchTotal", 18 * 1024 * 1024],
    ["web-5mb", "perFile", 5 * 1024 * 1024],
  ]);
  expect(new Set(builtInRules.map((rule) => rule.id)).size).toBe(builtInRules.length);
  expect(builtInRules.every((rule) => rule.sourceLabel.length > 0)).toBe(true);
  expect(builtInRules.every((rule) => rule.maxBytes > 0 && rule.acceptedKinds.length > 0)).toBe(true);
});

it("maps legacy IDs and groups provider children", () => {
  expect(ruleById("discord").id).toBe("discord-safe");
  expect(ruleById("gmail").id).toBe("gmail-personal");
  expect(ruleById("custom").id).toBe("custom");
  expect(rulesForFamily("discord").map((rule) => rule.id)).toEqual([
    "discord-safe",
    "discord-basic",
    "discord-nitro",
  ]);
});

it("validates and materializes an exact custom rule", () => {
  expect(validateCustomLimit(7 * 1024).valid).toBe(false);
  expect(validateCustomLimit(8 * 1024).valid).toBe(true);
  expect(validateCustomLimit(10 * 1024 * 1024 * 1024).valid).toBe(true);
  expect(validateCustomLimit(10 * 1024 * 1024 * 1024 + 1).valid).toBe(false);
  expect(customRule(12 * 1024 * 1024, "batchTotal").scope).toBe("batchTotal");
});
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `npm test -- src/domain/domain.test.ts`

Expected: FAIL because `DestinationRule`, the new IDs, and grouping helpers do not exist.

- [ ] **Step 3: Add the exact TypeScript contracts**

Replace `DestinationProfile` in `src/domain/types.ts` with:

```ts
export type LimitScope = "perFile" | "batchTotal";
export type DestinationFamily = "discord" | "email" | "web" | "custom" | "saved";

export type DestinationRule = {
  id: string;
  family: DestinationFamily;
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

// Compatibility bridge used by App.tsx until Task 6 migrates the UI.
export type DestinationProfile = DestinationRule;
```

- [ ] **Step 4: Implement the built-in registry and helpers**

In `src/domain/profiles.ts`, define constants and the six built-ins in the table order from Step 1. Use these verified dates and URLs exactly:

```ts
const KIB = 1024;
const MIB = 1024 * KIB;
export const MIN_CUSTOM_BYTES = 8 * KIB;
export const MAX_CUSTOM_BYTES = 10 * 1024 * MIB;

export const DISCORD_ATTACHMENTS_URL =
  "https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ";
export const DISCORD_CAPS_URL =
  "https://support.discord.com/hc/en-us/articles/33694251638295-Discord-Account-Caps-Server-Caps-and-More";
export const GMAIL_ATTACHMENTS_URL = "https://support.google.com/mail/answer/6584";
export const OUTLOOK_ATTACHMENTS_URL =
  "https://support.microsoft.com/en-US/Outlook/reduce-attachment-size-to-send-large-files-with-outlook";
```

Set every provider rule's `verifiedOn` to `"2026-08-30"`. Use `sourceUrl: DISCORD_CAPS_URL` for Discord Safe and `DISCORD_ATTACHMENTS_URL` for both paid tiers. Use `sourceUrl: null`, `verifiedOn: null`, and `sourceLabel: "FitSend generic default"` for Web upload and Custom.

Implement legacy mapping and safe fallback:

```ts
const legacyRuleIds: Record<string, string> = {
  discord: "discord-safe",
  gmail: "gmail-personal",
  web: "web-5mb",
  custom: "custom",
};

export function ruleById(id: string): DestinationRule {
  const resolved = legacyRuleIds[id] ?? id;
  if (resolved === "custom") return customRule(10 * MIB, "perFile");
  return builtInRules.find((rule) => rule.id === resolved) ?? builtInRules[0];
}
```

`customRule` creates an unsaved `builtIn: false` rule with ID `custom`, exact supplied bytes, and the selected scope. `validateCustomLimit` returns `{ valid: boolean; message: string | null }` with explicit range messages.

Keep the current UI compiling between tasks with these temporary-compatible exports; Task 6 stops using them, but leaving the aliases is harmless for external imports:

```ts
export const profiles: DestinationRule[] = [
  ...builtInRules,
  customRule(10 * MIB, "perFile"),
];
export const profileById = ruleById;
```

- [ ] **Step 5: Run frontend tests and production build**

Run:

```powershell
npm test -- src/domain/domain.test.ts
npm run build
```

Expected: tests and the production build PASS through the compatibility aliases.

- [ ] **Step 6: Commit the rule contracts**

```powershell
git add src/domain/types.ts src/domain/profiles.ts src/domain/domain.test.ts
git commit -m "feat: add verified destination rule contracts"
```

---

### Task 2: Add Local Saved Custom Plans

**Files:**
- Create: `src/domain/savedPlans.ts`
- Create: `src/domain/savedPlans.test.ts`
- Create: `src/hooks/useSavedPlans.ts`
- Modify: `src/domain/types.ts`

**Interfaces:**
- Produces `SavedPlanRecord` and `SavedPlanMutation`.
- Produces `parseSavedPlans`, `serializeSavedPlans`, `createSavedPlan`, `upsertSavedPlan`, `deleteSavedPlan`, and `savedPlanToRule`.
- Produces `useSavedPlans(): { plans, error, save, replace, remove, clearError }`.

- [ ] **Step 1: Write failing persistence tests**

Create `src/domain/savedPlans.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  createSavedPlan,
  deleteSavedPlan,
  parseSavedPlans,
  serializeSavedPlans,
  upsertSavedPlan,
} from "./savedPlans";

const now = "2026-08-30T12:00:00.000Z";
const uuid = (suffix: number) => `00000000-0000-4000-8000-${suffix.toString().padStart(12, "0")}`;

describe("saved send plans", () => {
  it("round-trips valid version-one entries and ignores malformed siblings", () => {
    const valid = createSavedPlan("Client portal", "batchTotal", 12 * 1024 * 1024, now, uuid(1));
    const raw = JSON.stringify({ schemaVersion: 1, plans: [valid, { id: 3, name: "broken" }] });
    expect(parseSavedPlans(raw)).toEqual([valid]);
    expect(parseSavedPlans(serializeSavedPlans([valid]))).toEqual([valid]);
  });

  it("requires explicit replacement for case-insensitive duplicate names", () => {
    const original = createSavedPlan("Client portal", "perFile", 2_000_000, now, uuid(1));
    const candidate = createSavedPlan(" client PORTAL ", "batchTotal", 3_000_000, now, uuid(2));
    expect(upsertSavedPlan([original], candidate, false).error).toBe("A saved plan with this name already exists.");
    const replaced = upsertSavedPlan([original], candidate, true);
    expect(replaced.error).toBeNull();
    expect(replaced.plans).toHaveLength(1);
    expect(replaced.plans[0].id).toBe(original.id);
    expect(replaced.plans[0].scope).toBe("batchTotal");
  });

  it("does not silently evict the twenty-first plan", () => {
    const plans = Array.from({ length: 20 }, (_, index) =>
      createSavedPlan(`Plan ${index + 1}`, "perFile", 1_000_000, now, uuid(index + 1)),
    );
    const extra = createSavedPlan("Plan 21", "perFile", 1_000_000, now, uuid(21));
    expect(upsertSavedPlan(plans, extra, false).error).toBe("FitSend can keep up to 20 saved plans.");
    expect(deleteSavedPlan(plans, uuid(4))).toHaveLength(19);
  });
});
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `npm test -- src/domain/savedPlans.test.ts`

Expected: FAIL because `savedPlans.ts` does not exist.

- [ ] **Step 3: Add saved-plan contracts and validation**

In `src/domain/types.ts` add:

```ts
export type SavedPlanRecord = {
  schemaVersion: 1;
  id: string;
  name: string;
  scope: LimitScope;
  maxBytes: number;
  createdAt: string;
  updatedAt: string;
};

export type SavedPlanMutation = {
  plans: SavedPlanRecord[];
  error: string | null;
};
```

`createSavedPlan` trims the name and throws only for programmer misuse of an invalid UUID/time input; user validation returns messages from `upsertSavedPlan`. Validate finite integer bytes through `validateCustomLimit`, count trimmed Unicode code points for the `1..=40` name limit, reject control/format-only names, compare names case-insensitively, and validate schema version, RFC 3339 timestamps, UUIDs, and the 20-plan capacity.

- [ ] **Step 4: Implement independent-entry parsing and exact replacement**

Use this storage envelope and key:

```ts
export const SAVED_PLANS_KEY = "fitsend.sendPlans.v1";

type SavedPlansEnvelope = {
  schemaVersion: 1;
  plans: SavedPlanRecord[];
};
```

`parseSavedPlans` returns `[]` for absent/invalid JSON/envelope data and filters invalid sibling entries. `upsertSavedPlan(plans, candidate, replace)` preserves the original ID and `createdAt` when replacing, updates `updatedAt`, and returns a new array without mutating input. `savedPlanToRule` returns a `DestinationRule` with family `saved`, `builtIn: false`, no source URL/date, and source label `Saved on this computer`.

- [ ] **Step 5: Add the local-storage hook**

Create `src/hooks/useSavedPlans.ts` with lazy initialization and write-through persistence:

```ts
export function useSavedPlans() {
  const [plans, setPlans] = useState<SavedPlanRecord[]>(() =>
    parseSavedPlans(window.localStorage.getItem(SAVED_PLANS_KEY)),
  );
  const [error, setError] = useState<string | null>(null);

  const commit = useCallback((mutation: SavedPlanMutation) => {
    if (mutation.error) {
      setError(mutation.error);
      return false;
    }
    try {
      window.localStorage.setItem(SAVED_PLANS_KEY, serializeSavedPlans(mutation.plans));
      setPlans(mutation.plans);
      setError(null);
      return true;
    } catch {
      setError("The plan is active, but FitSend could not save it on this computer.");
      return false;
    }
  }, []);
```

Expose `save(name, scope, maxBytes)`, `replace`, and `remove`, creating UUIDs with `crypto.randomUUID()` and times with `new Date().toISOString()`.

- [ ] **Step 6: Run tests and commit**

Run:

```powershell
npm test -- src/domain/savedPlans.test.ts
npm test
```

Commit:

```powershell
git add src/domain/types.ts src/domain/savedPlans.ts src/domain/savedPlans.test.ts src/hooks/useSavedPlans.ts
git commit -m "feat: persist custom send plans locally"
```

---

### Task 3: Add the Rust Aggregate Budget Engine

**Files:**
- Create: `src-tauri/core/src/batch_budget.rs`
- Modify: `src-tauri/core/src/domain.rs`
- Modify: `src-tauri/core/src/lib.rs`

**Interfaces:**
- Produces `LimitScope::{PerFile,BatchTotal}`.
- Produces `BudgetItemRequest`, `AcceptedBudgetItem`, `BatchBudgetRequest`, `ItemAllocation`, and `BatchBudget`.
- Produces `build_budget(&BatchBudgetRequest)` and `rebalance_budget(&BatchBudgetRequest)`.

- [ ] **Step 1: Write failing allocator tests**

Create the test module in `batch_budget.rs` with exact fixtures:

```rust
fn item(id: &str, source_bytes: u64, minimum: Option<u64>) -> BudgetItemRequest {
    BudgetItemRequest {
        id: id.to_string(),
        source_bytes,
        minimum_allocation_bytes: minimum,
    }
}

#[test]
fn assigns_the_full_ceiling_per_file() {
    let budget = build_budget(&BatchBudgetRequest {
        scope: LimitScope::PerFile,
        ceiling_bytes: 1_000_000,
        items: vec![item("a", 5_000_000, None), item("b", 2_000, None)],
        accepted: vec![],
    }).unwrap();
    assert_eq!(budget.allocations.iter().map(|entry| entry.target_bytes).collect::<Vec<_>>(), vec![1_000_000, 1_000_000]);
}

#[test]
fn capped_water_filling_preserves_a_previous_small_source_allocation() {
    let budget = rebalance_budget(&BatchBudgetRequest {
        scope: LimitScope::BatchTotal,
        ceiling_bytes: 600_000,
        items: vec![item("small", 20_000, Some(20_000)), item("large", 1_000_000, Some(400_000))],
        accepted: vec![],
    }).unwrap();
    assert!(budget.feasible);
    assert_eq!(budget.allocations[0].target_bytes, 20_000);
    assert_eq!(budget.allocations[1].target_bytes, 580_000);
}

#[test]
fn forwards_unused_bytes_without_shrinking_waiting_items() {
    let initial = build_budget(&BatchBudgetRequest {
        scope: LimitScope::BatchTotal,
        ceiling_bytes: 900_000,
        items: vec![item("a", 900_000, None), item("b", 900_000, None)],
        accepted: vec![],
    }).unwrap();
    let previous_b = initial.allocations[1].target_bytes;
    let budget = rebalance_budget(&BatchBudgetRequest {
        scope: LimitScope::BatchTotal,
        ceiling_bytes: 900_000,
        items: vec![item("b", 900_000, Some(previous_b))],
        accepted: vec![AcceptedBudgetItem { id: "a".to_string(), actual_bytes: 300_000 }],
    }).unwrap();
    assert_eq!(budget.allocations[0].target_bytes, 600_000);
    assert!(budget.allocations[0].target_bytes >= previous_b);
}

#[test]
fn rejects_a_ceiling_below_small_source_reserves() {
    let budget = build_budget(&BatchBudgetRequest {
        scope: LimitScope::BatchTotal,
        ceiling_bytes: 9_000,
        items: vec![item("tiny", 2_000, None), item("normal", 100_000, None)],
        accepted: vec![],
    }).unwrap();
    assert!(!budget.feasible);
    assert_eq!(budget.reason.as_deref(), Some("This total limit is too small for the selected file count."));
}
```

Also test: already-fitting sources get source-sized allocations; equal fractional remainders follow input order; duplicate IDs return `Err`; accepted bytes above the ceiling return `feasible = false`; failed/cancelled IDs are absent from `accepted` and therefore consume zero.

- [ ] **Step 2: Run the focused Rust test and verify failure**

Run: `cargo test -p fitsend-core batch_budget::tests --manifest-path src-tauri/Cargo.toml`

Expected: compilation fails because the module and serialized types do not exist.

- [ ] **Step 3: Add serialized Rust contracts**

In `domain.rs` add:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LimitScope { PerFile, BatchTotal }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetItemRequest {
    pub id: String,
    pub source_bytes: u64,
    pub minimum_allocation_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedBudgetItem {
    pub id: String,
    pub actual_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchBudgetRequest {
    pub scope: LimitScope,
    pub ceiling_bytes: u64,
    pub items: Vec<BudgetItemRequest>,
    pub accepted: Vec<AcceptedBudgetItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemAllocation {
    pub id: String,
    pub target_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchBudget {
    pub scope: LimitScope,
    pub ceiling_bytes: u64,
    pub accepted_bytes: u64,
    pub remaining_bytes: u64,
    pub allocations: Vec<ItemAllocation>,
    pub feasible: bool,
    pub reason: Option<String>,
}
```

- [ ] **Step 4: Implement validation and per-file allocation**

Define `pub const MIN_ITEM_BUDGET_BYTES: u64 = 8 * 1024;`. Reject zero ceilings, zero source sizes, duplicate waiting IDs, duplicate accepted IDs, and IDs present in both lists. Sum accepted bytes with `checked_add` and return a non-feasible budget when they exceed an aggregate ceiling.

For `PerFile`, return the complete ceiling for every waiting item, `accepted_bytes` as the checked accepted sum, and `remaining_bytes = ceiling_bytes` because accepted totals do not consume independent per-file ceilings.

- [ ] **Step 5: Implement capped proportional water-filling**

For aggregate scope:

```rust
let available = request.ceiling_bytes - accepted_bytes;
let base_floor = item.source_bytes.min(MIN_ITEM_BUDGET_BYTES);
let required_floor = item.minimum_allocation_bytes
    .unwrap_or(base_floor)
    .max(base_floor);
```

Reject a supplied `minimum_allocation_bytes` above its source size before evaluating the formula. Reserve every `required_floor`. If their checked sum exceeds `available`, return `feasible = false`. If waiting source sizes sum to at most `available`, allocate every source size. For aggregate results, set `remaining_bytes = ceiling_bytes - accepted_bytes`; it is the pool still available to waiting items before their allocations, not unassigned slack after allocation.

Otherwise repeatedly distribute the remaining pool across uncapped items using fixed demand weight `max(source_bytes - base_floor, 1)`. Compute products and division with `u128`. In each pass:

1. Calculate each floor share and remainder.
2. Cap shares at each item's remaining source capacity.
3. Remove capped items and recompute the undistributed pool for active items.
4. Once no cap fires, assign leftover single bytes by descending remainder and ascending input index.

Assert before returning that every target is between its required floor and source size and that `accepted_bytes + sum(targets) <= ceiling_bytes`.

- [ ] **Step 6: Implement forward rebalance as the same validated policy**

`rebalance_budget` calls the same internal allocator. It requires every waiting item to provide its prior target through `minimum_allocation_bytes`; `build_budget` permits `None`. This makes the no-shrink rule an input invariant rather than a UI convention.

- [ ] **Step 7: Export, run Rust checks, and commit**

Export all contracts and both functions from `core/src/lib.rs`, then run:

```powershell
cargo test -p fitsend-core batch_budget::tests --manifest-path src-tauri/Cargo.toml
cargo test -p fitsend-core --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Commit:

```powershell
git add src-tauri/core/src/domain.rs src-tauri/core/src/batch_budget.rs src-tauri/core/src/lib.rs
git commit -m "feat: add aggregate batch budget engine"
```

---

### Task 4: Expose Batch Budgets Across the Tauri Boundary

**Files:**
- Create: `src/domain/budget.ts`
- Create: `src/domain/budget.test.ts`
- Modify: `src/domain/types.ts`
- Modify: `src/domain/batch.ts`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces TypeScript mirrors of every Rust budget contract.
- Produces `budgetRequest`, `applyBudgetAllocations`, `acceptedBudgetItems`, and `allocationFor`.
- Produces Tauri commands `build_batch_budget` and `rebalance_batch_budget`.

- [ ] **Step 1: Write failing TypeScript budget adapter tests**

Create `src/domain/budget.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { acceptedBudgetItems, applyBudgetAllocations, budgetRequest } from "./budget";
import type { BatchBudget } from "./types";
import { batchItem } from "./batch.test-fixtures";
import { ruleById } from "./profiles";

const aggregateRule = {
  ...ruleById("gmail-personal"),
  maxBytes: 24 * 1024 * 1024,
};

it("builds a stable request in queue order", () => {
  const items = [batchItem("a", 9_000_000), batchItem("b", 2_000_000)];
  expect(budgetRequest(items, aggregateRule, false)).toEqual({
    scope: "batchTotal",
    ceilingBytes: aggregateRule.maxBytes,
    items: [
      { id: "a", sourceBytes: 9_000_000, minimumAllocationBytes: null },
      { id: "b", sourceBytes: 2_000_000, minimumAllocationBytes: null },
    ],
    accepted: [],
  });
});

it("applies allocations and includes only accepted actual bytes", () => {
  const budget = {
    feasible: true,
    allocations: [{ id: "a", targetBytes: 700_000 }],
  } as BatchBudget;
  const allocated = applyBudgetAllocations([batchItem("a", 1_000_000)], budget);
  expect(allocated[0].allocationBytes).toBe(700_000);
  expect(acceptedBudgetItems(allocated)).toEqual([]);
});
```

Move the existing local `batchItem` builder from `batch.test.ts` into the new test-only `src/domain/batch.test-fixtures.ts`. The file contains concrete defaults for every `BatchItem` field and accepts ID/source size overrides.

- [ ] **Step 2: Run the focused tests and verify failure**

Run: `npm test -- src/domain/budget.test.ts`

Expected: FAIL because the budget contracts and adapters do not exist.

- [ ] **Step 3: Add matching TypeScript contracts**

In `types.ts`, mirror Rust field names in camelCase:

```ts
export type BudgetItemRequest = {
  id: string;
  sourceBytes: number;
  minimumAllocationBytes: number | null;
};
export type AcceptedBudgetItem = { id: string; actualBytes: number };
export type BatchBudgetRequest = {
  scope: LimitScope;
  ceilingBytes: number;
  items: BudgetItemRequest[];
  accepted: AcceptedBudgetItem[];
};
export type ItemAllocation = { id: string; targetBytes: number };
export type BatchBudget = {
  scope: LimitScope;
  ceilingBytes: number;
  acceptedBytes: number;
  remainingBytes: number;
  allocations: ItemAllocation[];
  feasible: boolean;
  reason: string | null;
};
```

Add `allocationBytes: number | null` to `BatchItem` and initialize it to `null`.

- [ ] **Step 4: Implement the pure adapters**

`budgetRequest(items, rule, preserveMinimums)` includes only waiting items with analysis and successful terminal items in `accepted`. For waiting items, set `minimumAllocationBytes` to the current allocation only when `preserveMinimums` is true. `acceptedBudgetItems` includes `completed` and `noChange` rows using `result.outputBytes` or source size; it excludes failed/cancelled rows.

`applyBudgetAllocations` rejects unknown allocation IDs and leaves terminal rows untouched. It clears no existing waiting allocation unless the returned budget explicitly replaces it.

- [ ] **Step 5: Add the Tauri commands**

In `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
fn build_batch_budget(request: BatchBudgetRequest) -> Result<BatchBudget, String> {
    fitsend_core::build_budget(&request)
}

#[tauri::command]
fn rebalance_batch_budget(request: BatchBudgetRequest) -> Result<BatchBudget, String> {
    fitsend_core::rebalance_budget(&request)
}
```

Import the new public types and register both commands in `generate_handler!` before the existing plan/process commands.

- [ ] **Step 6: Run boundary checks and commit**

Run:

```powershell
npm test -- src/domain/budget.test.ts src/domain/batch.test.ts
cargo test --all-targets --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Commit:

```powershell
git add src/domain/types.ts src/domain/batch.ts src/domain/budget.ts src/domain/budget.test.ts src/domain/batch.test-fixtures.ts src/domain/batch.test.ts src-tauri/src/lib.rs
git commit -m "feat: expose batch budgets to the desktop queue"
```

---

### Task 5: Apply and Rebalance Budgets in the Sequential Queue

**Files:**
- Modify: `src/hooks/useBatchQueue.ts`
- Modify: `src/domain/batch.ts`
- Modify: `src/domain/batch.test.ts`
- Modify: `src/domain/budget.ts`
- Modify: `src/App.tsx`

**Interfaces:**
- Changes `useBatchQueue({ targetBytes, strategy })` to `useBatchQueue({ rule, strategy })`.
- Produces initial budget creation, current allocation processing, and post-terminal forward rebalancing.
- Preserves the hook's existing public queue actions and progress semantics.

- [ ] **Step 1: Write failing queue-state tests**

Add pure state assertions in `batch.test.ts`:

```ts
it("locks configuration as soon as a budget allocation exists", () => {
  const item = { ...batchItem("a", 1_000_000), allocationBytes: 700_000 };
  expect(isBatchConfigurationLocked([item])).toBe(true);
});

it("marks every waiting row failed when the batch budget is infeasible", () => {
  const items = [batchItem("a", 1_000_000), batchItem("b", 1_000_000)];
  const next = failWaitingItems(items, "This total limit is too small for the selected file count.");
  expect(next.map((item) => item.status)).toEqual(["failed", "failed"]);
  expect(next.every((item) => item.error?.includes("too small"))).toBe(true);
});
```

Add `failWaitingItems(items, message)` as a pure helper and update configuration locking to include `allocationBytes !== null`.

- [ ] **Step 2: Run tests and verify failure**

Run: `npm test -- src/domain/batch.test.ts src/domain/budget.test.ts`

Expected: FAIL because allocation locking and `failWaitingItems` are absent.

- [ ] **Step 3: Make queue state updates ref-safe**

Replace asynchronous ref synchronization with one mutation helper:

```ts
const replaceItems = useCallback((transform: (current: BatchItem[]) => BatchItem[]) => {
  const next = transform(itemsRef.current);
  itemsRef.current = next;
  setItems(next);
  return next;
}, []);
```

Use it from `patchItem`, add/remove/clear, progress events, cancellation, allocation application, and terminal updates. This prevents rebalancing from reading the previous React render while a processing result has already completed.

- [ ] **Step 4: Request and apply the initial budget**

Change the hook option to:

```ts
type UseBatchQueueOptions = {
  rule: DestinationRule;
  strategy: CompressionStrategy;
};
```

At the start of `start()`, invoke:

```ts
const initial = await invoke<BatchBudget>("build_batch_budget", {
  request: budgetRequest(itemsRef.current, rule, false),
});
```

If `initial.feasible` is false, call `failWaitingItems` with its reason, stop running, and create no media plans. Otherwise apply every allocation before processing the first row.

- [ ] **Step 5: Process the current allocation**

For each waiting snapshot, read the live row from `itemsRef.current` and require `allocationBytes`. Pass it as `targetBytes` to both `build_plan` and `process_media`. Never fall back to `rule.maxBytes` after a batch budget has been created; a missing allocation is a batch-level error.

- [ ] **Step 6: Rebalance after every terminal item**

After created, no-change, failed, or cancelled state is written, gather remaining waiting rows. If any remain, invoke:

```ts
const nextBudget = await invoke<BatchBudget>("rebalance_batch_budget", {
  request: budgetRequest(itemsRef.current, rule, true),
});
```

Apply the new allocations. If the command throws or returns infeasible, keep completed outputs, fail every waiting row with one shared clear message, and stop. Cancellation remains authoritative: do not rebalance after `cancelRequested.current` becomes true.

- [ ] **Step 7: Preserve progress and one-file behavior**

Verify that progress events still patch only the active job/item, failures continue to later rows, and a one-item aggregate plan receives a source-sized allocation when already under its total ceiling. Keep `overallProgress` unchanged.

Migrate the existing App call without redesigning the selector yet:

```ts
const queueRule = profileId === "custom"
  ? customRule(targetBytes, "perFile")
  : activeProfile;
const queue = useBatchQueue({ rule: queueRule, strategy });
```

Continue using `targetBytes` for the old receipt copy until Task 6 replaces the selector. This compatibility step keeps every task buildable.

- [ ] **Step 8: Run frontend tests/build and commit**

Run:

```powershell
npm test
npm run build
```

Expected: all tests and the production build pass.

Commit:

```powershell
git add src/hooks/useBatchQueue.ts src/domain/batch.ts src/domain/batch.test.ts src/domain/budget.ts src/App.tsx
git commit -m "feat: apply aggregate budgets to sequential batches"
```

---

### Task 6: Add Destination Families, Provider Tiers, and Saved Plan Controls

**Files:**
- Create: `src/components/DestinationPicker.tsx`
- Create: `src/components/CustomPlanEditor.tsx`
- Create: `src/components/DestinationPicker.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `src-tauri/capabilities/default.json`

**Interfaces:**
- Produces an accessible family selector and provider child selector.
- Consumes `DestinationRule`, grouped built-ins, saved rules, custom values, and configuration lock.
- Opens built-in source URLs through `@tauri-apps/plugin-opener`.

- [ ] **Step 1: Write failing semantic rendering tests**

Use `renderToStaticMarkup` from `react-dom/server`:

```tsx
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { DestinationPicker } from "./DestinationPicker";
import { ruleById } from "../domain/profiles";

it("renders destination families and the selected Discord tiers", () => {
  const html = renderToStaticMarkup(
    <DestinationPicker
      value={ruleById("discord-safe")}
      savedRules={[]}
      disabled={false}
      onChange={vi.fn()}
      onOpenCustom={vi.fn()}
    />,
  );
  expect(html).toContain("Discord");
  expect(html).toContain("Email");
  expect(html).toContain("Free — Safe");
  expect(html).toContain("Nitro Basic");
  expect(html).toContain("Each file");
});
```

Add a second test selecting Gmail and asserting `Gmail personal`, `Outlook internet email`, and `All files together`. Add a Saved-family test asserting the family is absent with no saved rules and present when one saved rule is supplied.

- [ ] **Step 2: Run the component test and verify failure**

Run: `npm test -- src/components/DestinationPicker.test.tsx`

Expected: FAIL because the components do not exist.

- [ ] **Step 3: Implement the destination picker**

Use native buttons with `role="radiogroup"`, `role="radio"`, `aria-checked`, and visible focus states. Top-level families are Discord, Email, Web upload, Custom, and conditionally Saved. Selecting Discord or Email chooses the last selected child within that family, defaulting to Discord Safe and Gmail personal. Selecting Web chooses `web-5mb`; Custom calls `onOpenCustom` without silently changing a locked plan.

Provider children appear as compact tier buttons under the family strip. Each child displays name, working ceiling, and `scopeLabel(rule.scope)`.

- [ ] **Step 4: Implement the custom editor**

`CustomPlanEditor` props contain exact current value/unit/scope/name, validation, disabled state, saved-plan error, and callbacks for change/save/replace/delete. Use a segmented `Each file` / `All files together` control. Keep Save disabled until the name and limit are valid. When duplicate-name save returns the exact collision error, show **Replace saved plan**; do not replace on the initial click.

- [ ] **Step 5: Migrate App state and queue composition**

In `App.tsx`:

```ts
const [ruleId, setRuleId] = useState("discord-safe");
const [customValue, setCustomValue] = useState(10);
const [customUnit, setCustomUnit] = useState<"KB" | "MB">("MB");
const [customScope, setCustomScope] = useState<LimitScope>("perFile");
const saved = useSavedPlans();

const customBytes = bytesFromCustomLimit(customValue, customUnit);
const savedRules = saved.plans.map(savedPlanToRule);
const activeRule = ruleId === "custom"
  ? customRule(customBytes, customScope)
  : [...builtInRules, ...savedRules].find((rule) => rule.id === ruleId) ?? ruleById(ruleId);
const queue = useBatchQueue({ rule: activeRule, strategy });
```

Replace the old profile buttons/custom limit block with `DestinationPicker` and `CustomPlanEditor`. Keep destination/strategy disabled while running or configuration is locked. Deleting an active saved plan switches selection to Discord Safe only when configuration is unlocked.

- [ ] **Step 6: Add rule provenance and source links**

The constraint receipt displays scope, working ceiling, published context, and `Checked 30 Aug 2026`. Import `openUrl` and open only the constant source URL attached to the selected built-in rule. Add an `opener:allow-open-url` permission object to `default.json` whose `allow` array contains exactly the four source URLs from Task 1; do not grant a general wildcard. Generic Web and Custom show their non-provider source labels and no link button.

- [ ] **Step 7: Style the refined right rail**

Add focused selectors for `.destination-families`, `.family-option`, `.provider-tiers`, `.provider-tier`, `.scope-toggle`, `.rule-source`, `.saved-plan-actions`, and `.custom-plan-name`. Preserve the current paper/coral/blue aesthetic, 340 px sticky rail, focus-visible outline, disabled opacity, and responsive single-column layout. Do not add a new font, gradient theme, or page-wide redesign.

- [ ] **Step 8: Run tests/build and commit**

Run:

```powershell
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

Commit:

```powershell
git add src/components/DestinationPicker.tsx src/components/CustomPlanEditor.tsx src/components/DestinationPicker.test.tsx src/App.tsx src/styles.css src-tauri/capabilities/default.json
git commit -m "feat: add destination tiers and saved plan controls"
```

---

### Task 7: Add Scope-Correct Allocation and Verification Receipts

**Files:**
- Modify: `src/domain/batch.ts`
- Modify: `src/domain/batch.test.ts`
- Modify: `src/components/FileQueue.tsx`
- Modify: `src/components/BatchSummary.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Produces `BatchProof` and `batchProof(items, rule)`.
- Extends `BatchTotals` and changes `batchTotals(items)` to `batchTotals(items, rule)`.
- Produces complete/partial, per-file/aggregate summary strings from pure domain data.
- Displays assigned budgets only for aggregate waiting/processing rows.

- [ ] **Step 1: Write failing proof tests**

Add exact assertions in `batch.test.ts`:

```ts
it("proves every accepted per-file result without claiming failed rows", () => {
  const items = [completedItem("a", 900_000), failedItem("b", 2_000_000)];
  const proof = batchProof(items, { ...ruleById("discord-safe"), maxBytes: 1_000_000 });
  expect(proof.valid).toBe(true);
  expect(proof.allSelectedAccepted).toBe(false);
  expect(proof.headline).toBe("1 accepted file is under 977 KB; 1 file needs attention");
});

it("uses actual accepted totals for aggregate proof", () => {
  const items = [completedItem("a", 9_000_000), noChangeItem("b", 3_000_000)];
  const proof = batchProof(items, { ...ruleById("gmail-personal"), maxBytes: 24 * 1024 * 1024 });
  expect(proof.valid).toBe(true);
  expect(proof.acceptedBytes).toBe(12_000_000);
  expect(proof.headline).toContain("verified under 24 MB");
});

it("rejects an aggregate proof above the real ceiling", () => {
  const items = [completedItem("a", 13_000_000), completedItem("b", 13_000_000)];
  expect(batchProof(items, ruleById("gmail-personal")).valid).toBe(false);
});

it("exposes scope-aware accepted totals to the summary", () => {
  const totals = batchTotals(
    [completedItem("a", 9_000_000), failedItem("b", 2_000_000)],
    ruleById("gmail-personal"),
  );
  expect(totals).toMatchObject({
    acceptedFiles: 1,
    acceptedBytes: 9_000_000,
    limitScope: "batchTotal",
    workingCeilingBytes: 24 * 1024 * 1024,
    allSelectedAccepted: false,
    proofValid: true,
  });
});
```

Add concrete `completedItem`, `noChangeItem`, and `failedItem` fixture helpers to `batch.test-fixtures.ts`; no test uses type assertions to omit required fields.

- [ ] **Step 2: Run proof tests and verify failure**

Run: `npm test -- src/domain/batch.test.ts`

Expected: FAIL because `BatchProof` and `batchProof` do not exist.

- [ ] **Step 3: Implement totals and proof state**

Add:

```ts
export type BatchProof = {
  scope: LimitScope;
  valid: boolean;
  allSelectedAccepted: boolean;
  acceptedFiles: number;
  acceptedBytes: number;
  ceilingBytes: number;
  attentionFiles: number;
  tone: "success" | "partial" | "invalid";
  headline: string;
  detail: string;
};
```

Extend the existing `BatchTotals` contract with the six fields asserted above. `batchTotals(items, rule)` derives them from accepted rows and the selected rule; keep the existing `sendableBytes` field as a compatibility alias of `acceptedBytes` during 0.3.0. `proofValid` uses the same predicates as `batchProof`, so the totals and receipt cannot disagree.

Per-file validity checks every accepted row's actual bytes against the ceiling. Aggregate validity checks the accepted sum. `allSelectedAccepted` requires every selected row to be completed or no-change. `tone` is success only when both validity and all-selected acceptance are true, partial when proof is valid but any row failed/cancelled, and invalid otherwise.

Use `formatBytes` for exact copy and singular/plural helpers so the three test strings do not diverge between component and domain.

- [ ] **Step 4: Display assigned allocation in queue rows**

For aggregate rules, waiting and processing rows show `Budget ≤ {formatBytes(allocationBytes)}` next to source facts once an allocation exists. Do not show an allocation as a result or proof; created rows continue to show actual verified bytes.

- [ ] **Step 5: Render scope-correct plan route and summary**

Change `BatchSummary` props to `{ totals, proof, destination }`. Render green success or amber partial styling from `proof.tone`; invalid proof uses the existing error styling and never says ready.

In `App.tsx`, the plan route label is `Limit for each file` or `Limit for all files`, the safe note describes the active scope, and the finished note uses `batchProof`. Remove every hard-coded `per file` sentence from aggregate paths.

- [ ] **Step 6: Add partial and allocation styles**

Add `.batch-summary.partial`, `.summary-mark.partial`, `.allocation-label`, and `.proof-invalid` using the existing amber/coral palette. Maintain WCAG-readable text contrast and keep allocation labels visually subordinate to actual result sizes.

- [ ] **Step 7: Run frontend checks and commit**

Run:

```powershell
npm test
npm run build
```

Commit:

```powershell
git add src/domain/batch.ts src/domain/batch.test.ts src/domain/batch.test-fixtures.ts src/components/FileQueue.tsx src/components/BatchSummary.tsx src/App.tsx src/styles.css
git commit -m "feat: show scope-aware verification receipts"
```

---

### Task 8: Extend Acceptance, Update Documentation, and Prepare 0.3.0

**Files:**
- Modify: `src-tauri/core/examples/acceptance_matrix.rs`
- Modify: `scripts/build-windows-bundled.ps1`
- Create: `docs/releases/0.3.0.md`
- Modify: `README.md`
- Modify: `src/App.tsx`
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/core/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Preserves the existing 56 media cases and adds twelve named batch scenarios, including one per-file regression.
- Produces consistent version `0.3.0` and release-ready Windows artifacts.

- [ ] **Step 1: Add aggregate acceptance helpers**

In `acceptance_matrix.rs`, add a helper that runs the same sequence as the frontend queue. Define the result type in the example before the helper:

```rust
struct AggregateCaseResult {
    results: Vec<Result<ProcessResult, String>>,
    allocations: Vec<ItemAllocation>,
    accepted_bytes: u64,
    ceiling_bytes: u64,
}

fn run_aggregate_batch(
    inputs: &[PathBuf],
    outputs: &Path,
    ceiling_bytes: u64,
    strategy: CompressionStrategy,
) -> AggregateCaseResult {
    let analyses = inputs.iter().map(|path| analyze(path.to_str().unwrap()).unwrap()).collect::<Vec<_>>();
    let ids = (0..analyses.len()).map(|index| format!("aggregate-{index}")).collect::<Vec<_>>();
    let mut accepted: Vec<AcceptedBudgetItem> = Vec::new();
    let mut allocations = build_budget(&budget_request(&ids, &analyses, ceiling_bytes, &accepted)).unwrap();
    let mut applied_allocations = Vec::new();
    let mut results = Vec::new();
    for (index, analysis) in analyses.iter().enumerate() {
        let id = &ids[index];
        let target = allocations.allocations.iter().find(|entry| &entry.id == id).unwrap().target_bytes;
        applied_allocations.push(ItemAllocation { id: id.clone(), target_bytes: target });
        let result = process(&ProcessRequest {
            output_path: aggregate_output_path(outputs, id, analysis).to_string_lossy().to_string(),
            analysis: analysis.clone(),
            target_bytes: target,
            strategy,
        });
        if let Ok(ref completed) = result {
            accepted.push(AcceptedBudgetItem { id: id.clone(), actual_bytes: completed.output_bytes });
        }
        results.push(result);
        if index + 1 < analyses.len() {
            let request = remaining_budget_request(
                &ids,
                &analyses,
                &accepted,
                index + 1,
                ceiling_bytes,
                &allocations,
            );
            allocations = rebalance_budget(&request).unwrap();
        }
    }
    AggregateCaseResult {
        accepted_bytes: accepted.iter().map(|entry| entry.actual_bytes).sum(),
        allocations: applied_allocations,
        results,
        ceiling_bytes,
    }
}

fn aggregate_output_path(outputs: &Path, id: &str, analysis: &MediaAnalysis) -> PathBuf {
    let extension = match &analysis.kind {
        MediaKind::Video => "mp4",
        MediaKind::Image if analysis.has_alpha => "png",
        MediaKind::Image => "jpg",
    };
    outputs.join(format!("{id}.{extension}"))
}
```

Implement `budget_request` and `remaining_budget_request` immediately beside this helper with these signatures:

```rust
fn budget_request(
    ids: &[String],
    analyses: &[MediaAnalysis],
    ceiling_bytes: u64,
    accepted: &[AcceptedBudgetItem],
) -> BatchBudgetRequest;

fn remaining_budget_request(
    ids: &[String],
    analyses: &[MediaAnalysis],
    accepted: &[AcceptedBudgetItem],
    start_index: usize,
    ceiling_bytes: u64,
    previous: &BatchBudget,
) -> BatchBudgetRequest;
```

They use the supplied `ids` vector as the only stable identity. `remaining_budget_request` includes only indices at or after `start_index` as waiting entries and copies each target from `previous.allocations` into `minimum_allocation_bytes`. Output paths never become IDs. The helper asserts every created output opens and every original hash is unchanged. The normal helper accepts analyzed media only; the corrupt-input row analyzes every selected path separately, records the analysis failure as attention, and sends only valid analyses to this helper, matching the frontend queue.

- [ ] **Step 2: Add the twelve specified aggregate cases**

Add named report rows for:

1. Images already under Gmail total.
2. Images over Gmail total.
3. Videos over Gmail total.
4. Mixed image/video total.
5. Small source cap redistribution.
6. Actual-under-allocation forward redistribution.
7. Impossible sum of per-item reserves.
8. One quality-floor failure.
9. One corrupt input.
10. Cancellation during aggregate video processing.
11. Partial proof excluding failed bytes.
12. Discord per-file regression.

Each row records scope, ceiling, actual accepted total, allocations, accepted count, attention count, and pass/fail reason in `latest.json` and `latest.md`. Aggregate passes assert actual accepted sum `<= ceiling`; per-file passes assert each accepted output `<= ceiling`.

Add a serializable optional batch field without disrupting the existing 56 rows:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchCaseMetrics {
    scope: LimitScope,
    ceiling_bytes: u64,
    allocation_bytes: Vec<u64>,
    accepted_files: usize,
    attention_files: usize,
    accepted_bytes: u64,
}
```

Add `batch: Option<BatchCaseMetrics>` to `CaseResult`, set it to `None` in every existing constructor, and populate it for the twelve new rows. Extend the Markdown table with Scope, Ceiling, Allocations, Accepted, Attention, and Accepted total columns.

For the cancellation row, use `process_with_progress` on the selected video and return `false` from its progress callback after encoding begins; assert that the process then returns `Err(PROCESS_CANCELLED)`. Mark later rows cancelled without processing, assert the active partial output and processing residue are absent, count cancelled rows as attention, and include only outputs accepted before cancellation in `accepted_bytes`.

- [ ] **Step 3: Run the acceptance matrix before version changes**

Run: `npm run test:acceptance`

Expected: the report includes the original 56 cases plus the twelve named aggregate rows, all passing. A quality-floor case passes when the row fails safely without publishing an invalid output.

- [ ] **Step 4: Update all version authorities**

Run `npm version 0.3.0 --no-git-tag-version`, then set these exact manifest values:

```toml
# src-tauri/Cargo.toml and src-tauri/core/Cargo.toml
version = "0.3.0"
```

```json
// src-tauri/tauri.conf.json
"version": "0.3.0"
```

Run `cargo check --manifest-path src-tauri/Cargo.toml` so Cargo.lock records both FitSend packages as 0.3.0. Change the footer to `FitSend 0.3.0`.

- [ ] **Step 5: Rewrite README destination and proof sections**

Document:

- Per-file versus all-files-together semantics.
- The six built-in rules and their checked date.
- Discord Safe's intentionally conservative name.
- Gmail personal and Outlook internet-email scope caveats.
- Custom exact ceiling/scope and local saved-plan limit.
- Forward-only reallocation and no backward re-encoding.
- Partial proof behavior.
- Static provider rules and local-only media processing.

Remove old statements that every selected destination applies separately to every item.

Create `docs/releases/0.3.0.md` with the user-visible destination-plan changes, provider rules checked on 2026-08-30, aggregate verification and partial-result semantics, local saved-plan behavior, bundled FFmpeg statement, and a clear note that the Windows installer is unsigned and may trigger SmartScreen or antivirus warnings.

- [ ] **Step 6: Run the complete verification suite**

Run:

```powershell
npm test
npm run build
cargo test -p fitsend-core --manifest-path src-tauri/Cargo.toml
cargo test --all-targets --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run test:acceptance
```

Expected: frontend, Rust, lint, original 56 media scenarios, and twelve new batch scenarios all pass.

- [ ] **Step 7: Commit the verified release source**

```powershell
git add README.md docs/releases/0.3.0.md scripts/build-windows-bundled.ps1 src/App.tsx package.json package-lock.json src-tauri/Cargo.toml src-tauri/core/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/core/examples/acceptance_matrix.rs
git commit -m "release: prepare FitSend 0.3.0"
git status --short
```

Expected: the release-source commit succeeds and the worktree is clean. All packaged artifacts in the next step are therefore generated from an identifiable commit.

- [ ] **Step 8: Build and inspect Windows packages**

Run: `npm run bundle:windows`

Verify these files exist and are non-empty:

```text
release/FitSend_0.3.0_x64-setup.exe
release/FitSend_0.3.0_x64_en-US.msi
release/FitSend_0.3.0_portable.zip
release/SHA256SUMS.txt
```

Update `scripts/build-windows-bundled.ps1` after the three artifacts are finalized: hash those three files with `Get-FileHash -Algorithm SHA256`, sort by filename, and write UTF-8 lines in `<lowercase hash>  <filename>` form to `release/SHA256SUMS.txt`. Delete/recreate the checksum file on each build so stale artifacts cannot remain listed. Recompute each artifact hash during verification and assert it matches the file.

Before assembling the portable directory, the script resolves it under the exact `release` directory and recreates only that versioned directory. This prevents files left by a previous build from entering the ZIP.

List the portable archive and assert it contains `FitSend.exe`, `ffmpeg/ffmpeg.exe`, `ffmpeg/ffprobe.exe`, `ffmpeg/BUILD_README.txt`, `ffmpeg/LICENSE.GPLv3.txt`, and `ffmpeg/FITSEND-FFMPEG-NOTICE.txt`. Confirm `ffmpeg -version` is the same revision named by BUILD_README and still lacks `--enable-nonfree`.

- [ ] **Step 9: Perform packaged desktop smoke checks**

Launch the portable build and verify:

- Native multi-select and multi-file drag/drop.
- Discord Safe proves each file.
- Gmail proves the accepted total.
- Custom aggregate plan saves, reloads after restart, replaces only after explicit confirmation, and deletes while unlocked.
- Source URL opens externally.
- One bad file produces an amber partial result without invalidating good outputs.
- Originals remain byte-identical.
- Video works without a separately installed FFmpeg.

If a smoke check exposes a defect, add a focused regression test, implement the smallest fix, rerun Step 6, commit the fix, and repeat Steps 8–9. Never keep packages built from a commit older than the final source commit.

- [ ] **Step 10: Final branch audit**

Run:

```powershell
git status --short
git log --oneline --decorate master..HEAD
git diff --check master...HEAD
```

Expected: clean worktree, only the planned 0.3.0 commits after the design commits, and no whitespace errors. Do not tag, merge, push, or publish until the user explicitly asks for that release operation.
