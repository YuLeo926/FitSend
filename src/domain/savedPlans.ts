import { formatBytes } from "./format";
import { validateCustomLimit } from "./profiles";
import type { DestinationRule, LimitScope, SavedPlanMutation, SavedPlanRecord } from "./types";

export const SAVED_PLANS_KEY = "fitsend.sendPlans.v1";

type SavedPlansEnvelope = {
  schemaVersion: 1;
  plans: SavedPlanRecord[];
};

const MAX_SAVED_PLANS = 20;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const RFC_3339_PATTERN = /^(\d{4})-(\d{2})-(\d{2})T(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function isRfc3339Timestamp(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = RFC_3339_PATTERN.exec(value);
  if (!match || !Number.isFinite(Date.parse(value))) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return month >= 1 && month <= 12 && day >= 1 && day <= daysInMonth;
}

function isScope(value: unknown): value is LimitScope {
  return value === "perFile" || value === "batchTotal";
}

function nameError(name: unknown): string | null {
  if (typeof name !== "string") return "Saved plan names must be between 1 and 40 characters.";
  const trimmed = name.trim();
  const count = Array.from(trimmed).length;
  if (count < 1 || count > 40) return "Saved plan names must be between 1 and 40 characters.";
  if (trimmed.replace(/[\p{Cc}\p{Cf}]/gu, "").trim().length === 0) {
    return "Saved plan names must include a visible character.";
  }
  return null;
}

function planError(plan: unknown): string | null {
  if (!isRecord(plan)) return "Invalid saved plan.";
  if (plan.schemaVersion !== 1 || !isUuid(plan.id) || !isRfc3339Timestamp(plan.createdAt) || !isRfc3339Timestamp(plan.updatedAt)) {
    return "Invalid saved plan.";
  }
  if (nameError(plan.name) || !isScope(plan.scope)) return "Invalid saved plan.";
  if (typeof plan.maxBytes !== "number" || !Number.isFinite(plan.maxBytes) || !Number.isInteger(plan.maxBytes) || !validateCustomLimit(plan.maxBytes).valid) {
    return "Invalid saved plan.";
  }
  return null;
}

function mutationError(candidate: SavedPlanRecord): string | null {
  const invalidName = nameError(candidate.name);
  if (invalidName) return invalidName;
  if (!isScope(candidate.scope)) return "Choose whether this limit applies to each file or the full batch.";
  if (!Number.isFinite(candidate.maxBytes) || !Number.isInteger(candidate.maxBytes)) {
    return "Custom limit must be a whole number of bytes.";
  }
  return validateCustomLimit(candidate.maxBytes).message;
}

function comparableName(name: string): string {
  return name.trim().toLowerCase();
}

export function createSavedPlan(name: string, scope: LimitScope, maxBytes: number, time: string, id: string): SavedPlanRecord {
  if (!isUuid(id)) throw new Error("Saved plan IDs must be UUIDs.");
  if (!isRfc3339Timestamp(time)) throw new Error("Saved plan times must be RFC 3339 timestamps.");
  return { schemaVersion: 1, id, name: name.trim(), scope, maxBytes, createdAt: time, updatedAt: time };
}

export function parseSavedPlans(raw: string | null): SavedPlanRecord[] {
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed) || parsed.schemaVersion !== 1 || !Array.isArray(parsed.plans)) return [];
    const names = new Set<string>();
    const ids = new Set<string>();
    const validPlans: SavedPlanRecord[] = [];
    for (const plan of parsed.plans) {
      if (planError(plan) !== null) continue;
      const savedPlan = plan as SavedPlanRecord;
      const name = comparableName(savedPlan.name);
      const id = savedPlan.id.toLowerCase();
      if (names.has(name) || ids.has(id) || validPlans.length >= MAX_SAVED_PLANS) continue;
      names.add(name);
      ids.add(id);
      validPlans.push(savedPlan);
    }
    return validPlans;
  } catch {
    return [];
  }
}

export function serializeSavedPlans(plans: SavedPlanRecord[]): string {
  const envelope: SavedPlansEnvelope = { schemaVersion: 1, plans };
  return JSON.stringify(envelope);
}

export function upsertSavedPlan(plans: SavedPlanRecord[], candidate: SavedPlanRecord, replace: boolean): SavedPlanMutation {
  const error = mutationError(candidate);
  if (error) return { plans: [...plans], error };

  const duplicateIndex = plans.findIndex((plan) => comparableName(plan.name) === comparableName(candidate.name));
  if (duplicateIndex >= 0) {
    if (!replace) return { plans: [...plans], error: "A saved plan with this name already exists." };
    const original = plans[duplicateIndex];
    const updated: SavedPlanRecord = { ...candidate, id: original.id, createdAt: original.createdAt };
    return { plans: plans.map((plan, index) => index === duplicateIndex ? updated : plan), error: null };
  }

  if (plans.length >= MAX_SAVED_PLANS) return { plans: [...plans], error: "FitSend can keep up to 20 saved plans." };
  return { plans: [...plans, candidate], error: null };
}

export function deleteSavedPlan(plans: SavedPlanRecord[], id: string): SavedPlanRecord[] {
  return plans.filter((plan) => plan.id !== id);
}

export function savedPlanToRule(plan: SavedPlanRecord): DestinationRule {
  const batchSuffix = plan.scope === "batchTotal" ? " total" : "";
  return {
    id: plan.id,
    family: "saved",
    name: plan.name,
    shortLabel: plan.name,
    description: plan.scope === "perFile" ? "Fits each file within your saved limit" : "Fits all files together within your saved limit",
    scope: plan.scope,
    publishedLimitLabel: `${formatBytes(plan.maxBytes)}${batchSuffix}`,
    maxBytes: plan.maxBytes,
    acceptedKinds: ["image", "video"],
    sourceLabel: "Saved on this computer",
    sourceUrl: null,
    verifiedOn: null,
    ruleNote: "Uses your saved custom limit.",
    accent: "ink",
    builtIn: false,
  };
}
