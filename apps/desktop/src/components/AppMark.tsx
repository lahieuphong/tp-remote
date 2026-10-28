/** Application mark: two overlapping screens. Decorative only. */
export function AppMark({ className = "" }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 64" aria-hidden="true" className={className}>
      <rect x="2" y="2" width="60" height="60" rx="14" className="fill-neutral-800" />
      <rect
        x="12"
        y="16"
        width="28"
        height="20"
        rx="3"
        className="fill-none stroke-teal-400"
        strokeWidth="3"
      />
      <rect x="24" y="26" width="28" height="20" rx="3" className="fill-teal-400" />
      <rect x="32" y="50" width="12" height="3" rx="1.5" className="fill-teal-400" />
    </svg>
  );
}
