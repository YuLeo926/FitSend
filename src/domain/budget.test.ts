import { describe, expect, it } from "vitest";
import { acceptedBudgetItems, applyBudgetAllocations, budgetRequest } from "./budget";
import type { BatchBudget } from "./types";
import { batchItem, result } from "./batch.test-fixtures";
import { ruleById } from "./profiles";

const aggregateRule = {
  ...ruleById("gmail-personal"),
  maxBytes: 24 * 1024 * 1024,
};

describe("budget adapters", () => {
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

  it("uses prior allocations only when preserving minimums and records successful actual bytes", () => {
    const items = [
      batchItem({ id: "waiting", allocationBytes: 800_000 }),
      batchItem({ id: "completed", status: "completed", result: result(600_000, "created") }),
      batchItem({ id: "no-change", status: "noChange" }),
      batchItem({ id: "failed", status: "failed" }),
      batchItem({ id: "processing", status: "processing" }),
    ];

    expect(budgetRequest(items, aggregateRule, true)).toMatchObject({
      items: [{ id: "waiting", minimumAllocationBytes: 800_000 }],
      accepted: [
        { id: "completed", actualBytes: 600_000 },
        { id: "no-change", actualBytes: 1_000 },
      ],
    });
    expect(budgetRequest(items, aggregateRule, false).items[0].minimumAllocationBytes).toBeNull();
  });

  it("preserves untouched waiting and terminal rows while rejecting unknown allocations", () => {
    const items = [
      batchItem({ id: "waiting", allocationBytes: 900_000 }),
      batchItem({ id: "completed", status: "completed", allocationBytes: 800_000 }),
    ];
    const budget = { allocations: [{ id: "completed", targetBytes: 700_000 }] } as BatchBudget;

    expect(applyBudgetAllocations(items, budget)).toEqual(items);
    expect(() => applyBudgetAllocations(items, {
      allocations: [{ id: "missing", targetBytes: 700_000 }],
    } as BatchBudget)).toThrow("unknown queue item: missing");
  });
});
