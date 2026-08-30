import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, formatElapsed, outputDefaultPath, savedPercent } from "./format";
import {
  builtInRules,
  bytesFromCustomLimit,
  customRule,
  profileById,
  ruleById,
  rulesForFamily,
  validateCustomLimit,
} from "./profiles";
import { strategies, strategyById } from "./strategies";

describe("FitSend domain helpers", () => {
  it("converts custom limits to bytes", () => {
    expect(bytesFromCustomLimit(10, "MB")).toBe(10 * 1024 * 1024);
    expect(bytesFromCustomLimit(500, "KB")).toBe(500 * 1024);
    expect(bytesFromCustomLimit(-1, "MB")).toBe(0);
  });

  it("formats byte sizes for the receipt", () => {
    expect(formatBytes(900)).toBe("900 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });

  it("formats durations", () => {
    expect(formatDuration(7.4)).toBe("7s");
    expect(formatDuration(67)).toBe("1:07");
  });

  it("creates a default output without changing the source name", () => {
    expect(outputDefaultPath("C:\\clips\\demo.mov", "mp4")).toBe(
      "C:/clips/demo.fitsend.mp4",
    );
  });

  it("defines unique source-backed rules with exact scopes and ceilings", () => {
    expect(builtInRules.map((rule) => [rule.id, rule.scope, rule.maxBytes])).toEqual([
      ["discord-safe", "perFile", Math.floor(9.8 * 1024 * 1024)],
      ["discord-basic", "perFile", 49 * 1024 * 1024],
      ["discord-nitro", "perFile", 490 * 1024 * 1024],
      ["gmail-personal", "batchTotal", 24 * 1024 * 1024],
      ["outlook-internet", "batchTotal", 18 * 1024 * 1024],
      ["web-5mb", "perFile", 5 * 1024 * 1024],
    ]);
    expect(new Set(builtInRules.map((rule) => rule.id)).size).toBe(builtInRules.length);
    expect(builtInRules.every((rule) => rule.sourceLabel.length > 0)).toBe(true);
    expect(builtInRules.every((rule) => rule.maxBytes > 0 && rule.acceptedKinds.length > 0)).toBe(true);
  });

  it("maps legacy IDs and groups provider children", () => {
    expect(ruleById("discord").id).toBe("discord-safe");
    expect(ruleById("gmail").id).toBe("gmail-personal");
    expect(ruleById("custom").id).toBe("custom");
    expect(rulesForFamily("discord").map((rule) => rule.id)).toEqual([
      "discord-safe",
      "discord-basic",
      "discord-nitro",
    ]);
  });

  it("validates and materializes an exact custom rule", () => {
    expect(validateCustomLimit(7 * 1024).valid).toBe(false);
    expect(validateCustomLimit(8 * 1024).valid).toBe(true);
    expect(validateCustomLimit(10 * 1024 * 1024 * 1024).valid).toBe(true);
    expect(validateCustomLimit(10 * 1024 * 1024 * 1024 + 1).valid).toBe(false);
    expect(customRule(12 * 1024 * 1024, "batchTotal").scope).toBe("batchTotal");
  });

  it("keeps the legacy profile fallback alias", () => {
    expect(profileById("missing").id).toBe("discord-safe");
  });

  it("formats processing results", () => {
    expect(formatElapsed(420)).toBe("Under 1s");
    expect(formatElapsed(65_000)).toBe("1m 5s");
    expect(savedPercent(10_000, 2_500)).toBe(75);
    expect(savedPercent(100, 120)).toBe(0);
  });

  it("exposes three unique compression strategies with balanced as the fallback", () => {
    expect(strategies.map((strategy) => strategy.id)).toEqual([
      "precise",
      "balanced",
      "smallest",
    ]);
    expect(new Set(strategies.map((strategy) => strategy.id)).size).toBe(3);
    expect(strategyById("balanced").name).toBe("Balanced");
  });
});
