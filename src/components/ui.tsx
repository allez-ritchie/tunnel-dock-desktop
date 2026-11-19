import type { CSSProperties, ReactNode } from "react";
import type { TunnelPhase } from "../api";
import { strings } from "../strings";

const paths: Record<string, ReactNode> = {
  plus: <path d="M12 5v14M5 12h14" />,
  play: <path d="M7 4.5v15l13-7.5z" />,
  stop: <rect x="6.5" y="6.5" width="11" height="11" rx="2" />,
  refresh: (
    <>
      <path d="M20.5 12a8.5 8.5 0 1 1-2.6-6.1" />
      <path d="M21 3.5v6h-6" />
    </>
  ),
  edit: (
    <>
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
    </>
  ),
  copy: (
    <>
      <rect x="9" y="9" width="12" height="12" rx="2" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </>
  ),
  trash: (
    <>
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M6 6l1 14h10l1-14" />
    </>
  ),
  external: (
    <>
      <path d="M14 3h7v7" />
      <path d="M21 3l-9 9" />
      <path d="M10 5H5a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-5" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.9 19.3a1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.7 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9.1a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.09a1.7 1.7 0 0 0 1.56 1.03H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.51 1.03z" />
    </>
  ),
  shield: <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />,
  key: (
    <>
      <circle cx="7.5" cy="15.5" r="3.5" />
      <path d="M10 13l7-7" />
      <path d="M15 5l3 3" />
    </>
  ),
  speed: (
    <>
      <path d="M12 13.5l4-4" />
      <path d="M20.5 17a9 9 0 1 0-17 0" />
    </>
  ),
  route: (
    <>
      <circle cx="5" cy="6" r="2.2" />
      <circle cx="19" cy="18" r="2.2" />
      <path d="M5 8.2V13a4 4 0 0 0 4 4h7.7" />
    </>
  ),
  arrowRight: (
    <>
      <path d="M5 12h14" />
      <path d="M13 6l6 6-6 6" />
    </>
  ),
  arrowLeft: (
    <>
      <path d="M19 12H5" />
      <path d="M11 18l-6-6 6-6" />
    </>
  ),
  chevronRight: <path d="M9 6l6 6-6 6" />,
  chevronUp: <path d="M6 15l6-6 6 6" />,
  chevronDown: <path d="M6 9l6 6 6-6" />,
  more: (
    <>
      <circle cx="5" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="19" cy="12" r="1.4" fill="currentColor" stroke="none" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6L6 18" />,
  check: <path d="M20 6L9 17l-5-5" />,
  alert: (
    <>
      <path d="M12 3l9.5 17H2.5z" />
      <path d="M12 9v5" />
      <circle cx="12" cy="17.4" r="0.8" fill="currentColor" stroke="none" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v6" />
      <circle cx="12" cy="7.5" r="0.9" fill="currentColor" stroke="none" />
    </>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>
  ),
  moon: <path d="M20 15.2A8.5 8.5 0 1 1 8.8 4a7 7 0 0 0 11.2 11.2z" />,
  monitor: (
    <>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8M12 16v4" />
    </>
  ),
  lock: (
    <>
      <rect x="4" y="10" width="16" height="10" rx="2" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </>
  ),
  terminal: (
    <>
      <path d="M4 17l6-6-6-6" />
      <path d="M12 19h8" />
    </>
  ),
  globe: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18" />
      <path d="M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18z" />
    </>
  ),
  swap: (
    <>
      <path d="M7 4v13" />
      <path d="M4 14l3 3 3-3" />
      <path d="M17 20V7" />
      <path d="M14 10l3-3 3 3" />
    </>
  ),
  apps: (
    <>
      <rect x="3" y="3" width="7" height="7" rx="2" />
      <rect x="14" y="3" width="7" height="7" rx="2" />
      <rect x="3" y="14" width="7" height="7" rx="2" />
      <rect x="14" y="14" width="7" height="7" rx="2" />
    </>
  ),
  folder: (
    <>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </>
  ),
  pulse: (
    <>
      <path d="M3 12h4l2.5-7 4 14L16 12h5" />
    </>
  ),
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3.5 2" />
    </>
  ),
  link: (
    <>
      <path d="M10 13a5 5 0 0 0 7.5.5l2-2a5 5 0 0 0-7-7l-1.2 1.2" />
      <path d="M14 11a5 5 0 0 0-7.5-.5l-2 2a5 5 0 0 0 7 7l1.2-1.2" />
    </>
  ),
  trashCan: (
    <>
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M6 6l1 14h10l1-14" />
    </>
  ),
};

export function Icon({
  name,
  size = 18,
  className,
}: {
  name: keyof typeof paths;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name]}
    </svg>
  );
}

export function BrandMark({ size = 42 }: { size?: number }) {
  return (
    <span className="brand-mark" style={{ width: size, height: size }}>
      <svg viewBox="0 0 48 48" width={size * 0.62} height={size * 0.62} fill="none">
        <path
          d="M14 15v18M34 15v18M14 24h6M28 24h6"
          stroke="currentColor"
          strokeWidth="3.6"
          strokeLinecap="round"
        />
        <rect x="20" y="20.5" width="8" height="7" rx="1.6" fill="currentColor" />
      </svg>
    </span>
  );
}

export function phaseColor(phase: TunnelPhase): string {
  switch (phase) {
    case "connected":
      return "var(--online)";
    case "connecting":
    case "reconnecting":
      return "var(--connecting)";
    case "error":
      return "var(--danger)";
    default:
      return "var(--idle)";
  }
}

export function phaseLabel(phase: TunnelPhase): string {
  switch (phase) {
    case "connected":
      return strings.appPhaseConnected;
    case "connecting":
      return strings.appPhaseConnecting;
    case "reconnecting":
      return strings.appPhaseReconnecting;
    case "error":
      return strings.appPhaseError;
    default:
      return strings.appPhaseIdle;
  }
}

export function StatusDot({
  phase,
  size = 9,
}: {
  phase: TunnelPhase;
  size?: number;
}) {
  const active =
    phase === "connected" || phase === "connecting" || phase === "reconnecting";
  return (
    <span
      className={`status-dot${active ? " is-pulsing" : ""}`}
      style={
        {
          width: size,
          height: size,
          background: phaseColor(phase),
          "--dot-color": phaseColor(phase),
        } as CSSProperties
      }
    />
  );
}

export function Chip({
  children,
  color = "var(--text-dim)",
  icon,
}: {
  children: ReactNode;
  color?: string;
  icon?: keyof typeof paths;
}) {
  return (
    <span className="chip" style={{ color, borderColor: `${color}2e` }}>
      {icon ? <Icon name={icon} size={12} /> : null}
      {children}
    </span>
  );
}

export function SectionHeader({
  title,
  hint,
  trailing,
}: {
  title: string;
  hint?: string;
  trailing?: ReactNode;
}) {
  return (
    <div className="section-header">
      <div>
        <h2>{title}</h2>
        {hint ? <p>{hint}</p> : null}
      </div>
      {trailing}
    </div>
  );
}

export function MonoAddress({
  children,
  color,
}: {
  children: ReactNode;
  color?: string;
}) {
  return (
    <span className="mono" style={color ? { color } : undefined}>
      {children}
    </span>
  );
}

export function WarningBox({
  text,
  danger = false,
}: {
  text: string;
  danger?: boolean;
}) {
  return (
    <div className={`warning-box${danger ? " is-danger" : ""}`}>
      <Icon name="alert" size={16} />
      <span>{text}</span>
    </div>
  );
}

export function SecureNotice({ text }: { text: string }) {
  return (
    <p className="secure-notice">
      <Icon name="lock" size={14} />
      <span>{text}</span>
    </p>
  );
}

export type NavTab = "apps" | "tunnels" | "settings";

export function BottomNav({
  tab,
  onChange,
}: {
  tab: NavTab;
  onChange: (tab: NavTab) => void;
}) {
  const items: { value: NavTab; label: string; icon: keyof typeof paths }[] = [
    { value: "apps", label: strings.tabApps, icon: "apps" },
    { value: "tunnels", label: strings.tabTunnels, icon: "route" },
    { value: "settings", label: strings.tabSettings, icon: "settings" },
  ];
  return (
    <nav className="bottom-nav" aria-label={strings.appName}>
      {items.map((item) => (
        <button
          key={item.value}
          type="button"
          className={`bottom-nav-item${tab === item.value ? " is-active" : ""}`}
          aria-current={tab === item.value ? "page" : undefined}
          onClick={() => onChange(item.value)}
        >
          <Icon name={item.icon} size={20} />
          <span>{item.label}</span>
        </button>
      ))}
    </nav>
  );
}

export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  title: string;
  body: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal title={title} onClose={onCancel} small>
      <p className="dialog-body">{body}</p>
      <div className="dialog-actions">
        <button type="button" className="btn ghost" onClick={onCancel}>
          {strings.dialogCancel}
        </button>
        <button type="button" className="btn danger" onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}

export function Modal({
  title,
  onClose,
  children,
  small = false,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  small?: boolean;
}) {
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className={`modal${small ? " is-small" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="modal-header">
          <h3>{title}</h3>
          <button
            type="button"
            className="icon-btn"
            onClick={onClose}
            aria-label={strings.dialogCancel}
          >
            <Icon name="close" size={18} />
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}
