import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ruleById } from "../domain/profiles";
import { savedPlanToRule } from "../domain/savedPlans";
import { CustomPlanEditor } from "./CustomPlanEditor";
import { DestinationPicker } from "./DestinationPicker";

function expectRovingRadioMarkup(html: string) {
  const radios = html.match(/<button[^>]*role="radio"[^>]*>/g) ?? [];
  expect(radios.length).toBeGreaterThan(1);
  for (const radio of radios) {
    if (radio.includes('aria-checked="true"')) {
      expect(radio).toContain('tabindex="0"');
    } else {
      expect(radio).toContain('aria-checked="false"');
      expect(radio).toContain('tabindex="-1"');
    }
  }
}

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
    expect(html).toContain('role="radiogroup" aria-label="Destination family"');
    expect(html).toContain('role="radiogroup" aria-label="Provider tier selection"');
    expectRovingRadioMarkup(html);
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
    expectRovingRadioMarkup(html);
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
    const selectedSaved = renderToStaticMarkup(
      <DestinationPicker {...props} value={savedRule} savedRules={[savedRule]} />,
    );

    expect(withoutSaved).not.toContain(">Saved<");
    expect(withSaved).toContain(">Saved<");
    expect(selectedSaved).toContain('role="radiogroup" aria-label="Saved plan selection"');
    expect(selectedSaved).toContain('aria-checked="true" tabindex="0"');
    expectRovingRadioMarkup(selectedSaved);
  });

  it("renders a roving tab stop for the selected custom scope", () => {
    const html = renderToStaticMarkup(
      <CustomPlanEditor
        value={10}
        unit="MB"
        scope="batchTotal"
        name=""
        validation={{ valid: true, message: null }}
        disabled={false}
        savedPlanError={null}
        canDelete={false}
        onValueChange={vi.fn()}
        onUnitChange={vi.fn()}
        onScopeChange={vi.fn()}
        onNameChange={vi.fn()}
        onSave={vi.fn()}
        onReplace={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(html).toContain('role="radiogroup" aria-labelledby="scope-label"');
    expect(html).toContain('aria-checked="true" tabindex="0"');
    expect(html).toContain('aria-checked="false" tabindex="-1"');
    expectRovingRadioMarkup(html);
  });
});
