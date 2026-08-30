import { useRef } from "react";
import { Check } from "lucide-react";
import { builtInRules, ruleById, scopeLabel } from "../domain/profiles";
import { formatBytes } from "../domain/format";
import type { DestinationFamily, DestinationRule } from "../domain/types";

type Props = {
  value: DestinationRule;
  savedRules: DestinationRule[];
  disabled: boolean;
  onChange: (rule: DestinationRule) => void;
  onOpenCustom: () => void;
};

const familyLabels: Array<{ id: Exclude<DestinationFamily, "saved">; label: string }> = [
  { id: "discord", label: "Discord" },
  { id: "email", label: "Email" },
  { id: "web", label: "Web upload" },
  { id: "custom", label: "Custom" },
];

const tierLabels: Record<string, string> = {
  "discord-safe": "Free — Safe",
  "discord-basic": "Nitro Basic",
  "discord-nitro": "Nitro",
  "gmail-personal": "Gmail personal",
  "outlook-internet": "Outlook internet email",
};

function ruleScopeLabel(rule: DestinationRule) {
  const label = scopeLabel(rule.scope);
  return label === "Per file" ? "Each file" : "All files together";
}

export function DestinationPicker({ value, savedRules, disabled, onChange, onOpenCustom }: Props) {
  const lastSelected = useRef<Partial<Record<DestinationFamily, DestinationRule>>>({});
  if (value.family === "discord" || value.family === "email" || value.family === "saved") {
    lastSelected.current[value.family] = value;
  }

  const families = savedRules.length > 0
    ? [...familyLabels, { id: "saved" as const, label: "Saved" }]
    : familyLabels;

  const selectFamily = (family: DestinationFamily) => {
    if (family === "custom") {
      onOpenCustom();
      return;
    }
    if (family === "web") {
      onChange(ruleById("web-5mb"));
      return;
    }
    const fallback = family === "discord"
      ? ruleById("discord-safe")
      : family === "email"
        ? ruleById("gmail-personal")
        : savedRules[0];
    const remembered = lastSelected.current[family];
    if (remembered && (family !== "saved" || savedRules.some((rule) => rule.id === remembered.id))) {
      onChange(remembered);
    } else if (fallback) {
      onChange(fallback);
    }
  };

  const visibleTiers = value.family === "discord" || value.family === "email"
    ? builtInRules.filter((rule) => rule.family === value.family)
    : value.family === "saved"
      ? savedRules
      : [];

  return (
    <div className="destination-picker">
      <div className="destination-families" role="radiogroup" aria-label="Destination family">
        {families.map((family) => {
          const selected = value.family === family.id;
          return (
            <button
              className={`family-option ${selected ? "selected" : ""}`}
              type="button"
              role="radio"
              aria-checked={selected}
              disabled={disabled}
              key={family.id}
              onClick={() => selectFamily(family.id)}
            >
              {family.label}
            </button>
          );
        })}
      </div>

      {visibleTiers.length > 0 ? (
        <div className="provider-tiers" role="radiogroup" aria-label={`${value.family === "saved" ? "Saved plan" : "Provider tier"} selection`}>
          {visibleTiers.map((rule) => {
            const selected = value.id === rule.id;
            return (
              <button
                className={`provider-tier ${selected ? "selected" : ""}`}
                type="button"
                role="radio"
                aria-checked={selected}
                disabled={disabled}
                key={rule.id}
                onClick={() => onChange(rule)}
              >
                <span className="provider-tier-copy">
                  <strong>{tierLabels[rule.id] ?? rule.name}</strong>
                  <small>{formatBytes(rule.maxBytes)} working ceiling · {ruleScopeLabel(rule)}</small>
                </span>
                <span className="radio-check" aria-hidden="true">{selected ? <Check size={13} /> : null}</span>
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
