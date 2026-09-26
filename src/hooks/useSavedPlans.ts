import { useCallback, useState } from "react";
import {
  SAVED_PLANS_KEY,
  createSavedPlan,
  deleteSavedPlan,
  parseSavedPlans,
  serializeSavedPlans,
  upsertSavedPlan,
} from "../domain/savedPlans";
import type { LimitScope, SavedPlanMutation, SavedPlanRecord } from "../domain/types";

export function loadSavedPlans(): { plans: SavedPlanRecord[]; error: string | null } {
  try {
    return { plans: parseSavedPlans(window.localStorage.getItem(SAVED_PLANS_KEY)), error: null };
  } catch {
    return { plans: [], error: "Saved plan storage is unavailable on this computer. Custom limits still work, but plans may not be saved." };
  }
}

export function useSavedPlans() {
  const [initial] = useState(loadSavedPlans);
  const [plans, setPlans] = useState<SavedPlanRecord[]>(initial.plans);
  const [error, setError] = useState<string | null>(initial.error);

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

  const save = useCallback((name: string, scope: LimitScope, maxBytes: number) =>
    commit(upsertSavedPlan(plans, createSavedPlan(name, scope, maxBytes, new Date().toISOString(), crypto.randomUUID()), false)),
  [commit, plans]);

  const replace = useCallback((name: string, scope: LimitScope, maxBytes: number) =>
    commit(upsertSavedPlan(plans, createSavedPlan(name, scope, maxBytes, new Date().toISOString(), crypto.randomUUID()), true)),
  [commit, plans]);

  const remove = useCallback((id: string) =>
    commit({ plans: deleteSavedPlan(plans, id), error: null }),
  [commit, plans]);

  const clearError = useCallback(() => setError(initial.error), [initial.error]);

  return { plans, error, save, replace, remove, clearError };
}
