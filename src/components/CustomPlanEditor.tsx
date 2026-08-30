import { Save, Trash2 } from "lucide-react";
import type { LimitScope } from "../domain/types";

const COLLISION_ERROR = "A saved plan with this name already exists.";

type CustomLimitUnit = "KB" | "MB";

type Props = {
  value: number;
  unit: CustomLimitUnit;
  scope: LimitScope;
  name: string;
  validation: { valid: boolean; message: string | null };
  disabled: boolean;
  savedPlanError: string | null;
  canDelete: boolean;
  onValueChange: (value: number) => void;
  onUnitChange: (unit: CustomLimitUnit) => void;
  onScopeChange: (scope: LimitScope) => void;
  onNameChange: (name: string) => void;
  onSave: () => void;
  onReplace: () => void;
  onDelete: () => void;
};

function validSavedName(name: string) {
  const trimmed = name.trim();
  const count = Array.from(trimmed).length;
  return count >= 1 && count <= 40 && trimmed.replace(/[\p{Cc}\p{Cf}]/gu, "").trim().length > 0;
}

export function CustomPlanEditor({
  value,
  unit,
  scope,
  name,
  validation,
  disabled,
  savedPlanError,
  canDelete,
  onValueChange,
  onUnitChange,
  onScopeChange,
  onNameChange,
  onSave,
  onReplace,
  onDelete,
}: Props) {
  const nameValid = validSavedName(name);
  const canSave = !disabled && validation.valid && nameValid;
  const showReplace = canSave && savedPlanError === COLLISION_ERROR;

  return (
    <section className="custom-plan-editor" aria-labelledby="custom-plan-heading">
      <div className="custom-plan-heading">
        <div>
          <span className="section-label">Custom rule</span>
          <h3 id="custom-plan-heading">Set the exact ceiling</h3>
        </div>
        <span>8 KB–10 GB</span>
      </div>

      <label className="custom-limit-label" htmlFor="custom-size">Working ceiling</label>
      <div className="custom-limit-fields">
        <input
          id="custom-size"
          type="number"
          min="0.01"
          step="0.1"
          value={Number.isFinite(value) ? value : ""}
          disabled={disabled}
          aria-invalid={!validation.valid}
          aria-describedby={!validation.valid ? "custom-limit-error" : undefined}
          onChange={(event) => onValueChange(event.target.value === "" ? Number.NaN : Number(event.target.value))}
        />
        <select
          aria-label="Custom limit unit"
          disabled={disabled}
          value={unit}
          onChange={(event) => onUnitChange(event.target.value as CustomLimitUnit)}
        >
          <option value="MB">MB</option>
          <option value="KB">KB</option>
        </select>
      </div>
      {!validation.valid ? <p className="field-error" id="custom-limit-error" role="alert">{validation.message}</p> : null}

      <span className="custom-limit-label" id="scope-label">Limit applies to</span>
      <div className="scope-toggle" role="radiogroup" aria-labelledby="scope-label">
        <button type="button" role="radio" aria-checked={scope === "perFile"} className={scope === "perFile" ? "selected" : ""} disabled={disabled} onClick={() => onScopeChange("perFile")}>Each file</button>
        <button type="button" role="radio" aria-checked={scope === "batchTotal"} className={scope === "batchTotal" ? "selected" : ""} disabled={disabled} onClick={() => onScopeChange("batchTotal")}>All files together</button>
      </div>

      <label className="custom-limit-label" htmlFor="custom-plan-name">Save for next time</label>
      <input
        className="custom-plan-name"
        id="custom-plan-name"
        type="text"
        maxLength={40}
        placeholder="Plan name"
        value={name}
        disabled={disabled}
        aria-describedby={savedPlanError ? "saved-plan-error" : undefined}
        onChange={(event) => onNameChange(event.target.value)}
      />

      {savedPlanError ? <p className="field-error" id="saved-plan-error" role="alert">{savedPlanError}</p> : null}
      <div className="saved-plan-actions">
        {showReplace ? (
          <button className="replace-plan-button" type="button" onClick={onReplace} disabled={!canSave}>Replace saved plan</button>
        ) : (
          <button className="save-plan-button" type="button" onClick={onSave} disabled={!canSave}><Save size={14} /> Save plan</button>
        )}
        {canDelete ? (
          <button className="delete-plan-button" type="button" onClick={onDelete} disabled={disabled}><Trash2 size={14} /> Delete saved plan</button>
        ) : null}
      </div>
    </section>
  );
}
