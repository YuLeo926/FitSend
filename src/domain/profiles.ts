import type { DestinationFamily, DestinationProfile, DestinationRule, LimitScope } from "./types";

const KIB = 1024;
const MIB = 1024 * KIB;
export const MIN_CUSTOM_BYTES = 8 * KIB;
export const MAX_CUSTOM_BYTES = 10 * 1024 * MIB;
export const DISCORD_ATTACHMENTS_URL = "https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ";
export const DISCORD_CAPS_URL = "https://support.discord.com/hc/en-us/articles/33694251638295-Discord-Account-Caps-Server-Caps-and-More";
export const GMAIL_ATTACHMENTS_URL = "https://support.google.com/mail/answer/6584";
export const OUTLOOK_ATTACHMENTS_URL = "https://support.microsoft.com/en-US/Outlook/reduce-attachment-size-to-send-large-files-with-outlook";

const mediaKinds = ["image", "video"] as const;
const providerRule = (rule: Omit<DestinationRule, "builtIn" | "acceptedKinds">): DestinationRule => ({ ...rule, acceptedKinds: [...mediaKinds], builtIn: true });

export const builtInRules: DestinationRule[] = [
  providerRule({ id: "discord-safe", family: "discord", name: "Discord (safe)", shortLabel: "Discord", description: "Uses the conservative free upload ceiling", scope: "perFile", publishedLimitLabel: "10 / 20 MB", maxBytes: Math.floor(9.8 * MIB), sourceLabel: "Discord account caps", sourceUrl: DISCORD_CAPS_URL, verifiedOn: "2026-08-30", ruleNote: "The checked Discord sources show 10 MB account caps and a 20 MB rollout FAQ. Safe uses the lower limit; use Custom for your confirmed account limit.", accent: "coral" }),
  providerRule({ id: "discord-basic", family: "discord", name: "Discord Basic", shortLabel: "Discord Basic", description: "Fits the published 50 MB attachment limit", scope: "perFile", publishedLimitLabel: "50 MB", maxBytes: 49 * MIB, sourceLabel: "Discord file attachments FAQ", sourceUrl: DISCORD_ATTACHMENTS_URL, verifiedOn: "2026-08-30", ruleNote: "Leaves delivery room below the published cap.", accent: "coral" }),
  providerRule({ id: "discord-nitro", family: "discord", name: "Discord Nitro", shortLabel: "Discord Nitro", description: "Fits the published 500 MB attachment limit", scope: "perFile", publishedLimitLabel: "500 MB", maxBytes: 490 * MIB, sourceLabel: "Discord file attachments FAQ", sourceUrl: DISCORD_ATTACHMENTS_URL, verifiedOn: "2026-08-30", ruleNote: "Leaves delivery room below the published cap.", accent: "coral" }),
  providerRule({ id: "gmail-personal", family: "email", name: "Gmail personal", shortLabel: "Gmail", description: "Fits Gmail's 25 MB total attachment limit", scope: "batchTotal", publishedLimitLabel: "25 MB total", maxBytes: 24 * MIB, sourceLabel: "Google Gmail Help", sourceUrl: GMAIL_ATTACHMENTS_URL, verifiedOn: "2026-08-30", ruleNote: "For personal Gmail attachments in total. Google Workspace limits can depend on your administrator; use Custom for your confirmed limit.", accent: "blue" }),
  providerRule({ id: "outlook-internet", family: "email", name: "Outlook internet", shortLabel: "Outlook", description: "Leaves attachment room within a 20 MB message limit", scope: "batchTotal", publishedLimitLabel: "20 MB entire message", maxBytes: 18 * MIB, sourceLabel: "Microsoft Outlook Support", sourceUrl: OUTLOOK_ATTACHMENTS_URL, verifiedOn: "2026-08-30", ruleNote: "The 20 MB limit covers the entire message, including attachments and message content. Exchange or administrator limits may differ; use Custom for your confirmed limit.", accent: "blue" }),
  providerRule({ id: "web-5mb", family: "web", name: "Web upload", shortLabel: "Web", description: "Fits a conservative 5 MB upload form", scope: "perFile", publishedLimitLabel: "5 MB", maxBytes: 5 * MIB, sourceLabel: "FitSend generic default", sourceUrl: null, verifiedOn: null, ruleNote: "A conservative default for unspecified upload forms.", accent: "ink" }),
];

const legacyRuleIds: Record<string, string> = { discord: "discord-safe", gmail: "gmail-personal", web: "web-5mb", custom: "custom" };
export function ruleById(id: string): DestinationRule {
  const resolved = legacyRuleIds[id] ?? id;
  if (resolved === "custom") return customRule(10 * MIB, "perFile");
  return builtInRules.find((rule) => rule.id === resolved) ?? builtInRules[0];
}
export function rulesForFamily(family: DestinationFamily): DestinationRule[] { return builtInRules.filter((rule) => rule.family === family); }
export function customRule(maxBytes: number, scope: LimitScope): DestinationRule {
  return { id: "custom", family: "custom", name: "Custom limit", shortLabel: "Custom", description: "Fit the exact ceiling you need", scope, publishedLimitLabel: "Custom", maxBytes, acceptedKinds: [...mediaKinds], sourceLabel: "FitSend generic default", sourceUrl: null, verifiedOn: null, ruleNote: "Set a limit between 8 KiB and 10 GiB.", accent: "ink", builtIn: false };
}
export function validateCustomLimit(value: number): { valid: boolean; message: string | null } {
  if (!Number.isFinite(value) || value < MIN_CUSTOM_BYTES) return { valid: false, message: "Custom limit must be at least 8 KiB." };
  if (value > MAX_CUSTOM_BYTES) return { valid: false, message: "Custom limit must be at most 10 GiB." };
  return { valid: true, message: null };
}
export function scopeLabel(scope: LimitScope): string { return scope === "perFile" ? "Per file" : "Batch total"; }
export function scopeDescription(scope: LimitScope): string { return scope === "perFile" ? "Each file must fit within this limit." : "All selected files together must fit within this limit."; }
export const profiles: DestinationProfile[] = [...builtInRules, customRule(10 * MIB, "perFile")];
export const profileById = ruleById;
export function bytesFromCustomLimit(value: number, unit: "KB" | "MB"): number {
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.floor(value * (unit === "MB" ? MIB : KIB));
}
