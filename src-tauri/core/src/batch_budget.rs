use std::collections::HashSet;

use crate::domain::{BatchBudget, BatchBudgetRequest, ItemAllocation, LimitScope};

pub const MIN_ITEM_BUDGET_BYTES: u64 = 8 * 1024;

const LIMIT_TOO_SMALL_REASON: &str = "This total limit is too small for the selected file count.";
const ACCEPTED_EXCEEDS_LIMIT_REASON: &str = "Accepted files exceed the total limit.";

pub fn build_budget(request: &BatchBudgetRequest) -> Result<BatchBudget, String> {
    allocate(request, false)
}

pub fn rebalance_budget(request: &BatchBudgetRequest) -> Result<BatchBudget, String> {
    allocate(request, true)
}

fn allocate(request: &BatchBudgetRequest, require_minimums: bool) -> Result<BatchBudget, String> {
    validate_request(request, require_minimums)?;
    let accepted_bytes = checked_sum(
        request.accepted.iter().map(|item| item.actual_bytes),
        "Accepted byte total is too large.",
    )?;

    if request.scope == LimitScope::PerFile {
        return Ok(BatchBudget {
            scope: request.scope,
            ceiling_bytes: request.ceiling_bytes,
            accepted_bytes,
            remaining_bytes: request.ceiling_bytes,
            allocations: request
                .items
                .iter()
                .map(|item| ItemAllocation {
                    id: item.id.clone(),
                    target_bytes: request.ceiling_bytes,
                })
                .collect(),
            feasible: true,
            reason: None,
        });
    }

    allocate_batch_total(request, accepted_bytes)
}

fn validate_request(request: &BatchBudgetRequest, require_minimums: bool) -> Result<(), String> {
    if request.ceiling_bytes == 0 {
        return Err("The budget ceiling must be greater than zero.".to_string());
    }

    let mut waiting_ids = HashSet::new();
    for item in &request.items {
        if item.source_bytes == 0 {
            return Err(format!(
                "Source size for '{}' must be greater than zero.",
                item.id
            ));
        }
        if !waiting_ids.insert(item.id.as_str()) {
            return Err(format!("Duplicate waiting item ID: '{}'.", item.id));
        }
        if require_minimums && item.minimum_allocation_bytes.is_none() {
            return Err(format!(
                "Rebalancing requires a previous allocation for '{}'.",
                item.id
            ));
        }
        if item
            .minimum_allocation_bytes
            .is_some_and(|minimum| minimum > item.source_bytes)
        {
            return Err(format!(
                "Minimum allocation for '{}' exceeds its source size.",
                item.id
            ));
        }
    }

    let mut accepted_ids = HashSet::new();
    for item in &request.accepted {
        if !accepted_ids.insert(item.id.as_str()) {
            return Err(format!("Duplicate accepted item ID: '{}'.", item.id));
        }
        if waiting_ids.contains(item.id.as_str()) {
            return Err(format!(
                "Item '{}' cannot be both waiting and accepted.",
                item.id
            ));
        }
    }

    Ok(())
}

fn allocate_batch_total(
    request: &BatchBudgetRequest,
    accepted_bytes: u64,
) -> Result<BatchBudget, String> {
    if accepted_bytes > request.ceiling_bytes {
        return Ok(infeasible_budget(
            request,
            accepted_bytes,
            0,
            ACCEPTED_EXCEEDS_LIMIT_REASON,
        ));
    }

    let available = request.ceiling_bytes - accepted_bytes;
    let required_floors = request
        .items
        .iter()
        .map(|item| {
            let base_floor = item.source_bytes.min(MIN_ITEM_BUDGET_BYTES);
            item.minimum_allocation_bytes
                .unwrap_or(base_floor)
                .max(base_floor)
        })
        .collect::<Vec<_>>();
    let required_sum = checked_sum(
        required_floors.iter().copied(),
        "Required allocation total is too large.",
    )?;

    if required_sum > available {
        return Ok(infeasible_budget(
            request,
            accepted_bytes,
            available,
            LIMIT_TOO_SMALL_REASON,
        ));
    }

    let source_sum = checked_sum(
        request.items.iter().map(|item| item.source_bytes),
        "Waiting source byte total is too large.",
    )?;
    if source_sum <= available {
        let targets = request
            .items
            .iter()
            .map(|item| item.source_bytes)
            .collect::<Vec<_>>();
        return Ok(finish_aggregate_budget(
            request,
            accepted_bytes,
            available,
            &required_floors,
            targets,
        ));
    }

    let weights = request
        .items
        .iter()
        .map(|item| {
            let base_floor = item.source_bytes.min(MIN_ITEM_BUDGET_BYTES);
            item.source_bytes.saturating_sub(base_floor).max(1)
        })
        .collect::<Vec<_>>();
    let capacities = request
        .items
        .iter()
        .zip(&required_floors)
        .map(|(item, floor)| item.source_bytes - floor)
        .collect::<Vec<_>>();
    let mut targets = required_floors.clone();
    let mut pool = available - required_sum;
    let mut active = capacities
        .iter()
        .enumerate()
        .filter_map(|(index, capacity)| (*capacity > 0).then_some(index))
        .collect::<Vec<_>>();

    while pool > 0 && !active.is_empty() {
        let weight_sum = checked_sum(
            active.iter().map(|index| weights[*index]),
            "Allocation weight total is too large.",
        )?;
        let shares = active
            .iter()
            .map(|index| {
                let numerator = u128::from(pool) * u128::from(weights[*index]);
                (
                    *index,
                    (numerator / u128::from(weight_sum)) as u64,
                    numerator % u128::from(weight_sum),
                )
            })
            .collect::<Vec<_>>();
        let capped = shares
            .iter()
            .filter_map(|(index, share, _)| (*share >= capacities[*index]).then_some(*index))
            .collect::<Vec<_>>();

        if !capped.is_empty() {
            let capped_set = capped.iter().copied().collect::<HashSet<_>>();
            for index in capped {
                targets[index] = targets[index]
                    .checked_add(capacities[index])
                    .ok_or_else(|| "Allocation total is too large.".to_string())?;
                pool = pool
                    .checked_sub(capacities[index])
                    .ok_or_else(|| "Allocation pool underflowed.".to_string())?;
            }
            active.retain(|index| !capped_set.contains(index));
            continue;
        }

        let distributed = checked_sum(
            shares.iter().map(|(_, share, _)| *share),
            "Distributed allocation total is too large.",
        )?;
        for (index, share, _) in &shares {
            targets[*index] = targets[*index]
                .checked_add(*share)
                .ok_or_else(|| "Allocation total is too large.".to_string())?;
        }
        pool = pool
            .checked_sub(distributed)
            .ok_or_else(|| "Allocation pool underflowed.".to_string())?;

        let mut remainder_order = shares
            .iter()
            .map(|(index, _, remainder)| (*index, *remainder))
            .collect::<Vec<_>>();
        remainder_order.sort_by(
            |(left_index, left_remainder), (right_index, right_remainder)| {
                right_remainder
                    .cmp(left_remainder)
                    .then_with(|| left_index.cmp(right_index))
            },
        );
        for (index, _) in remainder_order {
            if pool == 0 {
                break;
            }
            targets[index] = targets[index]
                .checked_add(1)
                .ok_or_else(|| "Allocation total is too large.".to_string())?;
            pool -= 1;
        }
    }

    assert_eq!(pool, 0);
    let allocation_sum = checked_sum(
        targets.iter().copied(),
        "Final allocation total is too large.",
    )
    .expect("validated water-filling allocation must fit in u64");
    assert_eq!(allocation_sum, available);

    Ok(finish_aggregate_budget(
        request,
        accepted_bytes,
        available,
        &required_floors,
        targets,
    ))
}

fn checked_sum(
    values: impl IntoIterator<Item = u64>,
    overflow_message: &str,
) -> Result<u64, String> {
    values.into_iter().try_fold(0_u64, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| overflow_message.to_string())
    })
}

fn infeasible_budget(
    request: &BatchBudgetRequest,
    accepted_bytes: u64,
    remaining_bytes: u64,
    reason: &str,
) -> BatchBudget {
    BatchBudget {
        scope: request.scope,
        ceiling_bytes: request.ceiling_bytes,
        accepted_bytes,
        remaining_bytes,
        allocations: Vec::new(),
        feasible: false,
        reason: Some(reason.to_string()),
    }
}

fn finish_aggregate_budget(
    request: &BatchBudgetRequest,
    accepted_bytes: u64,
    available: u64,
    required_floors: &[u64],
    targets: Vec<u64>,
) -> BatchBudget {
    assert_eq!(request.items.len(), targets.len());
    assert_eq!(required_floors.len(), targets.len());
    for ((item, required_floor), target) in request.items.iter().zip(required_floors).zip(&targets)
    {
        assert!(*target >= *required_floor);
        assert!(*target <= item.source_bytes);
    }
    let allocation_sum = targets
        .iter()
        .try_fold(0_u64, |sum, target| sum.checked_add(*target))
        .expect("validated allocation sum must fit in u64");
    let consumed = accepted_bytes
        .checked_add(allocation_sum)
        .expect("validated aggregate budget must fit in u64");
    assert!(consumed <= request.ceiling_bytes);

    BatchBudget {
        scope: request.scope,
        ceiling_bytes: request.ceiling_bytes,
        accepted_bytes,
        remaining_bytes: available,
        allocations: request
            .items
            .iter()
            .zip(targets)
            .map(|(item, target_bytes)| ItemAllocation {
                id: item.id.clone(),
                target_bytes,
            })
            .collect(),
        feasible: true,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_budget, rebalance_budget};
    use crate::domain::{AcceptedBudgetItem, BatchBudgetRequest, BudgetItemRequest, LimitScope};

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
        })
        .unwrap();
        assert_eq!(
            budget
                .allocations
                .iter()
                .map(|entry| entry.target_bytes)
                .collect::<Vec<_>>(),
            vec![1_000_000, 1_000_000]
        );
    }

    #[test]
    fn capped_water_filling_preserves_a_previous_small_source_allocation() {
        let budget = rebalance_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 600_000,
            items: vec![
                item("small", 20_000, Some(20_000)),
                item("large", 1_000_000, Some(400_000)),
            ],
            accepted: vec![],
        })
        .unwrap();
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
        })
        .unwrap();
        let previous_b = initial.allocations[1].target_bytes;
        let budget = rebalance_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 900_000,
            items: vec![item("b", 900_000, Some(previous_b))],
            accepted: vec![AcceptedBudgetItem {
                id: "a".to_string(),
                actual_bytes: 300_000,
            }],
        })
        .unwrap();
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
        })
        .unwrap();
        assert!(!budget.feasible);
        assert_eq!(
            budget.reason.as_deref(),
            Some("This total limit is too small for the selected file count.")
        );
    }

    #[test]
    fn already_fitting_sources_get_source_sized_allocations() {
        let budget = build_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 50_000,
            items: vec![item("a", 10_000, None), item("b", 20_000, None)],
            accepted: vec![],
        })
        .unwrap();

        assert_eq!(
            budget.allocations,
            vec![
                crate::domain::ItemAllocation {
                    id: "a".to_string(),
                    target_bytes: 10_000,
                },
                crate::domain::ItemAllocation {
                    id: "b".to_string(),
                    target_bytes: 20_000,
                },
            ]
        );
    }

    #[test]
    fn equal_fractional_remainders_follow_input_order() {
        let budget = build_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 16_387,
            items: vec![item("a", 16_384, None), item("b", 16_384, None)],
            accepted: vec![],
        })
        .unwrap();

        assert_eq!(budget.allocations[0].target_bytes, 8_194);
        assert_eq!(budget.allocations[1].target_bytes, 8_193);
    }

    #[test]
    fn duplicate_ids_return_an_error() {
        let request = BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 100_000,
            items: vec![item("same", 20_000, None), item("same", 30_000, None)],
            accepted: vec![],
        };

        assert!(build_budget(&request).is_err());
    }

    #[test]
    fn accepted_bytes_above_the_aggregate_ceiling_are_infeasible() {
        let budget = build_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 100_000,
            items: vec![item("waiting", 20_000, None)],
            accepted: vec![AcceptedBudgetItem {
                id: "accepted".to_string(),
                actual_bytes: 100_001,
            }],
        })
        .unwrap();

        assert!(!budget.feasible);
    }

    #[test]
    fn failed_or_cancelled_items_absent_from_accepted_consume_zero() {
        let budget = build_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 100_000,
            items: vec![item("waiting", 100_000, None)],
            accepted: vec![],
        })
        .unwrap();

        assert_eq!(budget.accepted_bytes, 0);
        assert_eq!(budget.allocations[0].target_bytes, 100_000);
    }

    #[test]
    fn validates_all_request_identity_and_size_invariants() {
        let valid = BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 100_000,
            items: vec![item("waiting", 20_000, None)],
            accepted: vec![],
        };

        let mut zero_ceiling = valid.clone();
        zero_ceiling.ceiling_bytes = 0;
        assert!(build_budget(&zero_ceiling).is_err());

        let mut zero_source = valid.clone();
        zero_source.items[0].source_bytes = 0;
        assert!(build_budget(&zero_source).is_err());

        let mut minimum_above_source = valid.clone();
        minimum_above_source.items[0].minimum_allocation_bytes = Some(20_001);
        assert!(build_budget(&minimum_above_source).is_err());

        let mut duplicate_accepted = valid.clone();
        duplicate_accepted.accepted = vec![
            AcceptedBudgetItem {
                id: "accepted".to_string(),
                actual_bytes: 1,
            },
            AcceptedBudgetItem {
                id: "accepted".to_string(),
                actual_bytes: 1,
            },
        ];
        assert!(build_budget(&duplicate_accepted).is_err());

        let mut overlapping = valid.clone();
        overlapping.accepted = vec![AcceptedBudgetItem {
            id: "waiting".to_string(),
            actual_bytes: 1,
        }];
        assert!(build_budget(&overlapping).is_err());

        assert!(rebalance_budget(&valid).is_err());
    }

    #[test]
    fn uses_checked_accepted_byte_sums() {
        let request = BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: u64::MAX,
            items: vec![],
            accepted: vec![
                AcceptedBudgetItem {
                    id: "a".to_string(),
                    actual_bytes: u64::MAX,
                },
                AcceptedBudgetItem {
                    id: "b".to_string(),
                    actual_bytes: 1,
                },
            ],
        };

        assert!(build_budget(&request).is_err());
    }

    #[test]
    fn serialized_contracts_use_camel_case_field_and_variant_names() {
        let request = BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 100_000,
            items: vec![item("waiting", 20_000, None)],
            accepted: vec![],
        };

        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["scope"], "batchTotal");
        assert_eq!(value["ceilingBytes"], 100_000);
        assert_eq!(value["items"][0]["sourceBytes"], 20_000);
        assert!(value["items"][0].get("minimumAllocationBytes").is_some());
    }
}
