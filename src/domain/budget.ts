import type { BatchItem } from "./batch";
import type {
  AcceptedBudgetItem,
  BatchBudget,
  BatchBudgetRequest,
  DestinationRule,
} from "./types";

export function acceptedBudgetItems(items: BatchItem[]): AcceptedBudgetItem[] {
  return items.flatMap((item) => {
    if (item.status !== "completed" && item.status !== "noChange") return [];
    const actualBytes = item.result?.outputBytes ?? item.analysis?.sizeBytes;
    return actualBytes === undefined ? [] : [{ id: item.id, actualBytes }];
  });
}

export function budgetRequest(
  items: BatchItem[],
  rule: DestinationRule,
  preserveMinimums: boolean,
): BatchBudgetRequest {
  return {
    scope: rule.scope,
    ceilingBytes: rule.maxBytes,
    items: items.flatMap((item) => {
      if (item.status !== "waiting" || !item.analysis) return [];
      return [{
        id: item.id,
        sourceBytes: item.analysis.sizeBytes,
        minimumAllocationBytes: preserveMinimums ? item.allocationBytes : null,
      }];
    }),
    accepted: acceptedBudgetItems(items),
  };
}

export function applyBudgetAllocations(items: BatchItem[], budget: BatchBudget): BatchItem[] {
  const itemIds = new Set(items.map((item) => item.id));
  for (const allocation of budget.allocations) {
    if (!itemIds.has(allocation.id)) {
      throw new Error(`Budget returned an allocation for unknown queue item: ${allocation.id}`);
    }
  }

  const allocations = new Map(budget.allocations.map((allocation) => [allocation.id, allocation.targetBytes]));
  return items.map((item) => {
    if (item.status !== "waiting" || !allocations.has(item.id)) return item;
    return { ...item, allocationBytes: allocations.get(item.id) ?? null };
  });
}
