import type { DestinationProfile } from "./types";

const MB = 1024 * 1024;

export const profiles: DestinationProfile[] = [
  {
    id: "discord",
    name: "Discord",
    shortLabel: "Discord",
    description: "Fits the current 10 MB free upload limit",
    maxBytes: Math.floor(9.8 * MB),
    acceptedKinds: ["image", "video"],
    accent: "coral",
  },
  {
    id: "gmail",
    name: "Email attachment",
    shortLabel: "Email",
    description: "Fits common 25 MB caps with delivery room",
    maxBytes: 24 * MB,
    acceptedKinds: ["image", "video"],
    accent: "blue",
  },
  {
    id: "web",
    name: "Web upload",
    shortLabel: "Web",
    description: "Fits a conservative 5 MB upload form",
    maxBytes: 5 * MB,
    acceptedKinds: ["image", "video"],
    accent: "ink",
  },
  {
    id: "custom",
    name: "Custom limit",
    shortLabel: "Custom",
    description: "Fit the exact ceiling you need",
    maxBytes: 10 * MB,
    acceptedKinds: ["image", "video"],
    accent: "ink",
  },
];

export function profileById(id: string): DestinationProfile {
  return profiles.find((profile) => profile.id === id) ?? profiles[0];
}

export function bytesFromCustomLimit(value: number, unit: "KB" | "MB"): number {
  if (!Number.isFinite(value) || value <= 0) return 0;
  const multiplier = unit === "MB" ? MB : 1024;
  return Math.floor(value * multiplier);
}
