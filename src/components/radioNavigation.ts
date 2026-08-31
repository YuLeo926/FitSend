import type { KeyboardEvent } from "react";

const arrowOffsets: Partial<Record<string, number>> = {
  ArrowLeft: -1,
  ArrowUp: -1,
  ArrowRight: 1,
  ArrowDown: 1,
};

export function handleRadioArrowNavigation(event: KeyboardEvent<HTMLButtonElement>) {
  const offset = arrowOffsets[event.key];
  if (event.currentTarget.disabled || offset === undefined) return;

  const group = event.currentTarget.closest('[role="radiogroup"]');
  const radios = Array.from(
    group?.querySelectorAll<HTMLButtonElement>('button[role="radio"]:not(:disabled)') ?? [],
  );
  const currentIndex = radios.indexOf(event.currentTarget);
  if (currentIndex < 0 || radios.length < 2) return;

  event.preventDefault();
  const nextRadio = radios[(currentIndex + offset + radios.length) % radios.length];
  nextRadio.focus();
  nextRadio.click();
}
