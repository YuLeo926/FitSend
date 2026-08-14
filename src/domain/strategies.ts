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
    description: "Highest quality under the limit",
  },
  {
    id: "balanced",
    name: "Balanced",
    description: "Smaller with almost no visible change",
  },
  {
    id: "smallest",
    name: "Smallest acceptable",
    description: "As small as possible above a safe quality floor",
  },
];

export function strategyById(id: CompressionStrategy): StrategyOption {
  return strategies.find((strategy) => strategy.id === id) ?? strategies[1];
}
