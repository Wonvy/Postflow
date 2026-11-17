import type { ReactNode } from "react";

/** 设置页里的一节。 */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b border-line px-4 py-4 last:border-b-0">
      <h2 className="mb-3 text-[12px] font-medium text-fg-3">{title}</h2>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

/** 设置页里的一行：左边名称和说明，右边控件。 */
export function SettingRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="text-fg">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}
