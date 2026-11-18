import type { ReactNode } from "react";

/** 设置页里的一行：左边名称和说明，右边控件。 */
export function SettingRow({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
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

/** 一页设置里的小标题，把同类的几行归在一起。 */
export function SettingsHeading({ children }: { children: ReactNode }) {
  return <h3 className="-mb-1 pt-2 text-[12px] font-medium text-fg-3 first:pt-0">{children}</h3>;
}
