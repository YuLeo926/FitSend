import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ruleById } from "../domain/profiles";
import { savedPlanToRule } from "../domain/savedPlans";
import { DestinationPicker } from "./DestinationPicker";

describe("DestinationPicker", () => {
  it("renders destination families and the selected Discord tiers", () => {
    const html = renderToStaticMarkup(
      <DestinationPicker
        value={ruleById("discord-safe")}
        savedRules={[]}
        disabled={false}
        onChange={vi.fn()}
        onOpenCustom={vi.fn()}
      />,
    );

    expect(html).toContain("Discord");
    expect(html).toContain("Email");
    expect(html).toContain("Free — Safe");
    expect(html).toContain("Nitro Basic");
    expect(html).toContain("Each file");
  });

  it("renders email providers with aggregate scope language", () => {
    const html = renderToStaticMarkup(
      <DestinationPicker
        value={ruleById("gmail-personal")}
        savedRules={[]}
        disabled={false}
        onChange={vi.fn()}
        onOpenCustom={vi.fn()}
      />,
    );

    expect(html).toContain("Gmail personal");
    expect(html).toContain("Outlook internet email");
    expect(html).toContain("All files together");
  });

  it("shows the Saved family only when saved rules exist", () => {
    const props = {
      value: ruleById("discord-safe"),
      disabled: false,
      onChange: vi.fn(),
      onOpenCustom: vi.fn(),
    };
    const withoutSaved = renderToStaticMarkup(<DestinationPicker {...props} savedRules={[]} />);
    const savedRule = savedPlanToRule({
      schemaVersion: 1,
      id: "00000000-0000-4000-8000-000000000001",
      name: "Client portal",
      scope: "perFile",
      maxBytes: 2 * 1024 * 1024,
      createdAt: "2026-08-30T12:00:00.000Z",
      updatedAt: "2026-08-30T12:00:00.000Z",
    });
    const withSaved = renderToStaticMarkup(<DestinationPicker {...props} savedRules={[savedRule]} />);

    expect(withoutSaved).not.toContain(">Saved<");
    expect(withSaved).toContain(">Saved<");
  });
});
