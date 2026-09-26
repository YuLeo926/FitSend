import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import App from "./App";
import { CustomPlanEditor } from "./components/CustomPlanEditor";
import { DestinationPicker } from "./components/DestinationPicker";
import { createSavedPlan, serializeSavedPlans } from "./domain/savedPlans";
import { ruleById } from "./domain/profiles";

// A tiny hook/event harness exercises the actual App callbacks without adding a DOM dependency.
const harness = vi.hoisted(() => ({ slots: [] as unknown[], cursor: 0, openUrl: vi.fn() }));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useState: (initial: unknown) => {
    const index = harness.cursor++;
    if (!(index in harness.slots)) harness.slots[index] = typeof initial === "function" ? initial() : initial;
    return [harness.slots[index], (value: unknown) => { harness.slots[index] = typeof value === "function" ? value(harness.slots[index]) : value; }];
  },
  useMemo: (factory: () => unknown) => factory(),
  useCallback: (callback: unknown) => callback,
  useEffect: () => undefined,
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: harness.openUrl }));
vi.mock("./hooks/useBatchQueue", () => ({ useBatchQueue: () => ({
  items: [], running: false, allTerminal: false, hasRunnable: false,
  proof: { acceptedFiles: 0 }, totals: { originalBytes: 0 }, clear: vi.fn(), addPaths: vi.fn(),
}) }));

function render() { harness.cursor = 0; return App(); }
function nodes(node: ReactNode): ReactElement<Record<string, unknown>>[] {
  if (Array.isArray(node)) return node.flatMap(nodes);
  if (!node || typeof node !== "object" || !("props" in node)) return [];
  const element = node as ReactElement<Record<string, unknown>>;
  return [element, ...nodes(element.props.children as ReactNode)];
}
function component<P>(tree: ReactNode, type: (props: P) => ReactNode): P {
  return nodes(tree).find((node) => node.type === type)!.props as P;
}
function text(tree: ReactNode): string {
  if (Array.isArray(tree)) return tree.map(text).join(" ");
  if (tree && typeof tree === "object" && "props" in tree) return text((tree as ReactElement<{ children: ReactNode }>).props.children);
  return typeof tree === "string" ? tree : "";
}
const saved = createSavedPlan("Original plan", "batchTotal", 10000, "2026-08-30T12:00:00Z", "00000000-0000-4000-8000-000000000001");

beforeEach(() => {
  harness.slots = []; harness.cursor = 0; harness.openUrl.mockReset();
  vi.stubGlobal("window", { localStorage: { getItem: () => serializeSavedPlans([saved]), setItem: vi.fn() } });
});

describe("send-plan review regressions", () => {
  it("name-only Saved edits become Custom and remove the old Delete target", () => {
    let tree = render();
    const picker = component(tree, DestinationPicker);
    picker.onChange(picker.savedRules[0]);
    tree = render();
    expect(component(tree, CustomPlanEditor).canDelete).toBe(true);
    component(tree, CustomPlanEditor).onNameChange("New name");
    tree = render();
    expect(component(tree, DestinationPicker).value.id).toBe("custom");
    expect(component(tree, CustomPlanEditor)).toMatchObject({ name: "New name", canDelete: false, scope: "batchTotal" });
    expect(component(tree, DestinationPicker).savedRules[0].name).toBe("Original plan");
  });

  it.each(["getter", "getItem"])("mounts and keeps Custom usable when storage %s throws", (failure) => {
    const unavailable = () => { throw new Error("blocked storage"); };
    vi.stubGlobal("window", failure === "getter" ? Object.defineProperty({}, "localStorage", { get: unavailable }) : { localStorage: { getItem: unavailable, setItem: unavailable } });
    let tree = render();
    expect(component(tree, DestinationPicker).savedRules).toEqual([]);
    expect(text(tree)).toContain("storage is unavailable");
    component(tree, DestinationPicker).onOpenCustom();
    tree = render();
    const editor = component(tree, CustomPlanEditor);
    expect(editor.disabled).toBe(false);
    expect(editor.savedPlanError).toContain("storage is unavailable");
    editor.onNameChange("Unsaved");
    tree = render();
    component(tree, CustomPlanEditor).onSave();
    tree = render();
    expect(component(tree, DestinationPicker).savedRules).toEqual([]);
    expect(component(tree, CustomPlanEditor).savedPlanError).toContain("could not save");
  });

  it("successful source retry clears only the source error, leaving picker errors intact", async () => {
    let tree = render();
    const source = () => nodes(tree).find((node) => node.props.className === "rule-source-link")!.props.onClick as () => Promise<void>;
    harness.openUrl.mockRejectedValueOnce(new Error("opener failed"));
    await source()(); await Promise.resolve();
    tree = render();
    expect(text(tree)).toContain("could not open the official source");
    const picker = nodes(tree).find((node) => String(node.props.className).startsWith("drop-zone"))!.props.onClick as () => void;
    picker(); tree = render();
    expect(text(tree)).toContain("browser preview");
    harness.openUrl.mockResolvedValueOnce(undefined);
    await source()(); await Promise.resolve();
    tree = render();
    expect(text(tree)).not.toContain("could not open the official source");
    expect(text(tree)).toContain("browser preview");
  });

  it("renders provider qualifications without changing ceilings or checked dates", () => {
    let tree = render();
    expect(text(tree)).toContain("10 / 20 MB");
    expect(text(tree)).toContain("Safe uses the lower limit");
    for (const [id, limit, note] of [["gmail-personal", 24, "Google Workspace"], ["outlook-internet", 18, "entire message"]] as const) {
      component(tree, DestinationPicker).onChange(ruleById(id)); tree = render();
      expect(text(tree)).toContain(note);
      expect(text(tree)).toContain("Custom");
      expect(text(tree)).toContain("Checked 30 Aug 2026");
      expect(component(tree, DestinationPicker).value.maxBytes).toBe(limit * 1024 * 1024);
    }
  });
});
