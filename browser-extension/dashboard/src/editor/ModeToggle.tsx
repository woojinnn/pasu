import "./ModeToggle.css";

export type EditorMode = "builder" | "code";

const MODES: Array<{ value: EditorMode; label: string }> = [
  { value: "builder", label: "Builder" },
  { value: "code", label: "Code" },
];

interface ModeToggleProps {
  mode: EditorMode;
  onChange: (mode: EditorMode) => void;
}

export function ModeToggle({ mode, onChange }: ModeToggleProps) {
  return (
    <div className="mode-toggle" role="tablist" aria-label="Editor mode">
      {MODES.map((m) => (
        <button
          key={m.value}
          type="button"
          role="tab"
          aria-selected={mode === m.value}
          className={"mode-btn" + (mode === m.value ? " active" : "")}
          onClick={() => onChange(m.value)}
        >
          {m.label}
        </button>
      ))}
    </div>
  );
}
