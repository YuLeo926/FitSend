import type { KeyboardEvent } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { StrategyPicker } from "./StrategyPicker";
import { handleRadioArrowNavigation } from "./radioNavigation";

describe("strategy keyboard controls", () => {
  it("exposes exactly one tab stop and wires the radio navigation handler", () => {
    const props = { value: "balanced" as const, disabled: false, onChange: vi.fn() };
    const html = renderToStaticMarkup(<StrategyPicker {...props} />);
    expect(html.match(/tabindex="0"/g)).toHaveLength(1);
    expect(html.match(/tabindex="-1"/g)).toHaveLength(2);
    const radios = StrategyPicker(props).props.children[1].props.children;
    expect(radios.every((radio: { props: { onKeyDown: unknown } }) => radio.props.onKeyDown === handleRadioArrowNavigation)).toBe(true);
  });

  it("arrows focus and select with wrapping, while Tab and Space keep native behavior", () => {
    const radios = Array.from({ length: 3 }, () => ({ disabled: false, focus: vi.fn(), click: vi.fn(), closest: () => ({ querySelectorAll: () => radios }) }));
    for (const [key, expected] of [["ArrowRight", 1], ["ArrowDown", 1], ["ArrowLeft", 2], ["ArrowUp", 2]] as const) {
      radios.forEach((radio) => { radio.focus.mockClear(); radio.click.mockClear(); });
      const preventDefault = vi.fn();
      handleRadioArrowNavigation({ key, currentTarget: radios[0], preventDefault } as unknown as KeyboardEvent<HTMLButtonElement>);
      expect(preventDefault).toHaveBeenCalledOnce();
      expect(radios[expected].focus).toHaveBeenCalledOnce();
      expect(radios[expected].click).toHaveBeenCalledOnce();
    }
    for (const key of ["Tab", " "]) {
      const preventDefault = vi.fn();
      handleRadioArrowNavigation({ key, currentTarget: radios[0], preventDefault } as unknown as KeyboardEvent<HTMLButtonElement>);
      expect(preventDefault).not.toHaveBeenCalled();
    }
    radios[0].disabled = true;
    const preventDefault = vi.fn();
    handleRadioArrowNavigation({ key: "ArrowRight", currentTarget: radios[0], preventDefault } as unknown as KeyboardEvent<HTMLButtonElement>);
    expect(preventDefault).not.toHaveBeenCalled();
  });
});
