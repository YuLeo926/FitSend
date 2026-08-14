import { Check, Gauge, Scale, Sparkles } from "lucide-react";
import { strategies } from "../domain/strategies";
import type { CompressionStrategy } from "../domain/types";

type Props = {
  value: CompressionStrategy;
  disabled: boolean;
  onChange: (strategy: CompressionStrategy) => void;
};

const icons = {
  precise: Scale,
  balanced: Gauge,
  smallest: Sparkles,
};

export function StrategyPicker({ value, disabled, onChange }: Props) {
  return (
    <div className="strategy-section">
      <div className="aside-heading strategy-heading">
        <span className="step-number">2</span>
        <div><span className="section-label">Compression</span><h2>How should it fit?</h2></div>
      </div>
      <div className="strategy-options" role="radiogroup" aria-label="Compression strategy">
        {strategies.map((strategy) => {
          const Icon = icons[strategy.id];
          const selected = value === strategy.id;
          return (
            <button
              className={`strategy-option ${selected ? "selected" : ""}`}
              type="button"
              role="radio"
              aria-checked={selected}
              disabled={disabled}
              key={strategy.id}
              onClick={() => onChange(strategy.id)}
            >
              <span className="strategy-icon"><Icon size={16} /></span>
              <span><strong>{strategy.name}</strong><small>{strategy.description}</small></span>
              <span className="radio-check">{selected ? <Check size={13} /> : null}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
