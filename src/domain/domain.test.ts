import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, formatElapsed, outputDefaultPath, savedPercent } from "./format";
import { bytesFromCustomLimit, profileById } from "./profiles";
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

  it("falls back to the default profile", () => {
    expect(profileById("missing").id).toBe("discord");
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
