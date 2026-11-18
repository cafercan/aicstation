import { useT } from "../i18n";

/** Small "Remote Control is on" mark; a click opens the session on claude.ai (the caller does the work). */
export default function RemoteMark({ onOpen, className = "remote", error }: {
  onOpen: () => void; className?: string; error?: string | null;
}) {
  const t = useT();
  const label = t("remote.on");
  return (
    <button type="button" className={className} aria-label={label} title={error ? `${label}\n${error}` : label}
      onClick={(e) => { e.stopPropagation(); onOpen(); }}
      onKeyDown={(e) => e.stopPropagation()}>
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor"
        strokeWidth="1.5" strokeLinecap="round">
        <circle cx="8" cy="8" r="1.4" fill="currentColor" stroke="none" />
        <path d="M5.2 5.2a4 4 0 0 0 0 5.6M10.8 5.2a4 4 0 0 1 0 5.6M3 3a7 7 0 0 0 0 10M13 3a7 7 0 0 1 0 10" />
      </svg>
    </button>
  );
}
