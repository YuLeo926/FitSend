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

  it("keeps the first persisted plan for a case-insensitive duplicate name", () => {
    const first = createSavedPlan("Client portal", "perFile", 2_000_000, now, uuid(1));
    const duplicate = createSavedPlan(" client PORTAL ", "batchTotal", 3_000_000, now, uuid(2));
    const raw = JSON.stringify({ schemaVersion: 1, plans: [first, duplicate] });
    expect(parseSavedPlans(raw)).toEqual([first]);
  });

  it("retains the first twenty unique valid persisted plans", () => {
    const plans = Array.from({ length: 21 }, (_, index) =>
      createSavedPlan(`Plan ${index + 1}`, "perFile", 1_000_000, now, uuid(index + 1)),
    );
    const raw = JSON.stringify({ schemaVersion: 1, plans });
    expect(parseSavedPlans(raw)).toEqual(plans.slice(0, 20));
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
