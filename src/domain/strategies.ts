import type { CompressionStrategy } from "./types";

export type StrategyOption = {
  id: CompressionStrategy;
  name: string;
  description: string;
};

export const strategies: readonly StrategyOption[] = [
  {
    id: "precise",
    name: "Precise fit",
    description: "Use the most quality that still fits",
  },
  {
    id: "balanced",
    name: "Balanced",
    description: "Keep it looking original; shrink only when worthwhile",
  },
  {
    id: "smallest",
    name: "Smallest acceptable",
    description: "Go as small as possible without crossing the quality floor",
  },
];

export function strategyById(id: CompressionStrategy): StrategyOption {
  return strategies.find((strategy) => strategy.id === id) ?? strategies[1];
}
