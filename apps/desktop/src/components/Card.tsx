import type { ReactNode } from "react";

export function Card({
  title,
  children,
  className = "",
  labelledBy,
}: {
  title: string;
  children: ReactNode;
  className?: string;
  labelledBy: string;
}) {
  return (
    <section
      aria-labelledby={labelledBy}
      className={`rounded-xl border border-neutral-800 bg-neutral-900/60 p-5 ${className}`}
    >
      <h2
        id={labelledBy}
        className="mb-3 text-xs font-semibold tracking-wider text-neutral-400 uppercase"
      >
        {title}
      </h2>
      {children}
    </section>
  );
}
