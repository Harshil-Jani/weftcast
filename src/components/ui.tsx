import { useEffect, useRef, type ReactNode, type ButtonHTMLAttributes } from "react";
import type { LucideIcon } from "lucide-react";

/** Lucide icons at the house stroke. */
export function Icon({ as: I, size = 16, label }: { as: LucideIcon; size?: number; label?: string }) {
  return <I size={size} strokeWidth={1.5} aria-hidden={label ? undefined : true} aria-label={label} />;
}

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  kind?: "default" | "primary" | "destructive" | "plain";
  large?: boolean;
  icon?: LucideIcon;
};
export function Button({ kind = "default", large, icon, children, className = "", ...rest }: BtnProps) {
  return (
    <button type="button" {...rest} className={`btn ${kind === "default" ? "" : kind} ${large ? "lg" : ""} ${className}`}>
      {icon && <Icon as={icon} size={large ? 16 : 14} />}
      {children}
    </button>
  );
}

/** Icon-only toolbar button; the label is the tooltip and the VoiceOver name. */
export function IconButton({ icon, label, small, pressed, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & {
  icon: LucideIcon; label: string; small?: boolean; pressed?: boolean;
}) {
  return (
    <button type="button" {...rest} className={`btn icon ${small ? "sm" : ""}`} title={label} aria-label={label} aria-pressed={pressed}>
      <Icon as={icon} size={small ? 14 : 16} />
    </button>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label, full }: {
  value: T; options: { value: T; label: ReactNode; disabled?: boolean; title?: string }[]; onChange: (v: T) => void; label: string; full?: boolean;
}) {
  const move = (dir: number) => {
    const live = options.filter((o) => !o.disabled);
    const i = live.findIndex((o) => o.value === value);
    const next = live[(i + dir + live.length) % live.length];
    if (next) onChange(next.value);
  };
  return (
    <div className={`segmented ${full ? "full" : ""}`} role="radiogroup" aria-label={label}
      onKeyDown={(e) => { if (e.key === "ArrowRight") { e.preventDefault(); move(1); } if (e.key === "ArrowLeft") { e.preventDefault(); move(-1); } }}>
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={o.value === value} tabIndex={o.value === value ? 0 : -1}
          disabled={o.disabled} title={o.title} onClick={() => onChange(o.value)}>{o.label}</button>
      ))}
    </div>
  );
}

export function Toggle({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <span className="toggle">
      <input type="checkbox" role="switch" aria-label={label} checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </span>
  );
}

export function Checkbox({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return <input type="checkbox" className="check" aria-label={label} checked={checked} onChange={(e) => onChange(e.target.checked)} />;
}

export function Slider({ value, min, max, step = 1, ticks, onChange, label }: {
  value: number; min: number; max: number; step?: number; ticks?: string[]; onChange: (v: number) => void; label: string;
}) {
  const pct = `${((value - min) / (max - min)) * 100}%`;
  return (
    <div className="slider">
      <input type="range" min={min} max={max} step={step} value={value} aria-label={label}
        style={{ ["--pct" as string]: pct }} onChange={(e) => onChange(Number(e.target.value))} />
      {ticks && <div className="ticks" aria-hidden>{ticks.map((t) => <span key={t}>{t}</span>)}</div>}
    </div>
  );
}

export function Progress({ value }: { value?: number }) {
  return (
    <div className={`progress ${value === undefined ? "indeterminate" : ""}`} role="progressbar"
      aria-valuemin={0} aria-valuemax={100} aria-valuenow={value === undefined ? undefined : Math.round(value * 100)}>
      <i style={value === undefined ? undefined : { width: `${value * 100}%` }} />
    </div>
  );
}

export type Status = { text: string; error?: boolean } | null;
/** One line under the control that caused it: a spinner and progress while busy, then the result. */
export function TaskStatus({ busy, progress, status }: { busy?: string; progress?: { label: string; value?: number } | null; status?: Status }) {
  if (busy || progress) {
    return (
      <div className="task" aria-live="polite">
        <div className="hstack"><span className="spinner" />{progress?.label ?? `${busy}...`}</div>
        {progress && progress.value !== undefined && <Progress value={progress.value} />}
      </div>
    );
  }
  if (!status) return null;
  return <p className={`callout ${status.error ? "error" : "ok"}`} role={status.error ? "alert" : "status"}>{status.text}</p>;
}

/** A macOS sheet: drops from the top of the window, Esc or the cancel button closes it. */
export function Sheet({ title, subtitle, children, footer, onClose, wide }: {
  title: string; subtitle?: ReactNode; children: ReactNode; footer?: ReactNode; onClose?: () => void; wide?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>("[autofocus], input, select, textarea, button");
    first?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && onClose) { e.stopPropagation(); onClose(); }
      if (e.key === "Tab" && ref.current) {
        const f = Array.from(ref.current.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), select, textarea, [tabindex='0']"));
        if (!f.length) return;
        const [a, z] = [f[0], f[f.length - 1]];
        if (e.shiftKey && document.activeElement === a) { e.preventDefault(); z.focus(); }
        else if (!e.shiftKey && document.activeElement === z) { e.preventDefault(); a.focus(); }
      }
    };
    window.addEventListener("keydown", key, true);
    return () => { window.removeEventListener("keydown", key, true); prev?.focus?.(); };
  }, [onClose]);
  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose?.(); }}>
      <div ref={ref} className={`sheet ${wide ? "wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
        <header><h2 className="title2">{title}</h2>{subtitle && <p className="callout secondary">{subtitle}</p>}</header>
        <div className="body">{children}</div>
        {footer && <footer>{footer}</footer>}
      </div>
    </div>
  );
}

export function Group({ children }: { children: ReactNode }) {
  return <div className="group">{children}</div>;
}

export function Row({ label, detail, children, block }: { label?: ReactNode; detail?: ReactNode; children?: ReactNode; block?: boolean }) {
  return (
    <div className={`row ${block ? "block" : ""}`}>
      {label !== undefined && <div className="label"><span>{label}</span>{detail && <small>{detail}</small>}</div>}
      {children}
    </div>
  );
}

export function SidebarItem({ icon, label, current, count, onClick }: { icon: LucideIcon; label: string; current: boolean; count?: number; onClick: () => void }) {
  return (
    <button type="button" className="side-item" aria-current={current ? "page" : undefined} onClick={onClick}>
      <Icon as={icon} />{label}{count !== undefined && <span className="count">{count}</span>}
    </button>
  );
}

/** Copyable link field. */
export function CopyRow({ value, onCopy }: { value: string; onCopy: () => void }) {
  return (
    <div className="copy-row"><code title={value}>{value}</code><Button onClick={onCopy}>Copy</Button></div>
  );
}
