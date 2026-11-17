// 界面的基础零件。所有组件共用这里的尺寸和颜色，保持整体一致。

import * as Context from "@radix-ui/react-context-menu";
import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import * as Tooltip from "@radix-ui/react-tooltip";
import { ChevronRight, LoaderCircle, Rss, X } from "lucide-react";
import {
  type ButtonHTMLAttributes,
  type CSSProperties,
  type InputHTMLAttributes,
  type MouseEvent,
  type PointerEvent,
  type ReactElement,
  type ReactNode,
  type SelectHTMLAttributes,
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { useT } from "../lib/i18n";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

/** 图标统一用 16px、1.75 描边。 */
export const ICON = { size: 16, strokeWidth: 1.75 } as const;

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-[4px] border border-line-strong bg-hover px-1 font-sans text-[11px] font-medium text-fg-2">
      {children}
    </kbd>
  );
}

export function Tip({
  label,
  shortcut,
  side = "bottom",
  children,
}: {
  label: string;
  shortcut?: string;
  side?: "top" | "bottom" | "left" | "right";
  children: ReactNode;
}) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side={side}
          sideOffset={6}
          className="anim-fade z-50 flex items-center gap-2 rounded-md bg-surface px-2 py-1 text-[12px] text-fg shadow-pop"
        >
          {label}
          {shortcut && <Kbd>{shortcut}</Kbd>}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  shortcut?: string;
  active?: boolean;
  tipSide?: "top" | "bottom" | "left" | "right";
}

export function IconButton({
  label,
  shortcut,
  active,
  tipSide,
  className,
  children,
  ...rest
}: IconButtonProps) {
  return (
    <Tip label={label} shortcut={shortcut} side={tipSide}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={active}
        className={cx(
          "inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors",
          "disabled:pointer-events-none disabled:opacity-40",
          // 开着的开关（自动翻译、全文模式、AI 面板）一眼能看出来：强调色底和图标
          active ? "bg-accent-soft text-accent" : "text-fg-2 hover:bg-hover hover:text-fg",
          className,
        )}
        {...rest}
      >
        {children}
      </button>
    </Tip>
  );
}

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  busy?: boolean;
}

export function Button({ variant = "secondary", busy, className, children, disabled, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      disabled={disabled || busy}
      className={cx(
        "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md px-2.5 text-[13px] font-medium transition-colors",
        "disabled:pointer-events-none disabled:opacity-50",
        variant === "primary" && "bg-accent-strong text-white hover:bg-accent",
        variant === "secondary" &&
          "border border-line-strong bg-hover text-fg hover:bg-active",
        variant === "ghost" && "text-fg-2 hover:bg-hover hover:text-fg",
        variant === "danger" && "bg-danger text-white hover:opacity-90",
        className,
      )}
      {...rest}
    >
      {busy && <Spinner />}
      {children}
    </button>
  );
}

/** 开关。 */
export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative inline-flex h-[18px] w-8 shrink-0 items-center rounded-full transition-colors disabled:opacity-50",
        checked ? "bg-accent-strong" : "bg-active ring-1 ring-inset ring-line-strong",
      )}
    >
      <span
        className={cx(
          "inline-block size-3.5 rounded-full bg-white shadow-sm transition-transform",
          checked ? "translate-x-[16px]" : "translate-x-[2px]",
        )}
      />
    </button>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return <LoaderCircle size={size} strokeWidth={2} className="animate-spin" aria-hidden />;
}

const inputClass =
  "h-8 w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] text-fg " +
  "placeholder:text-fg-3 outline-none transition-colors " +
  "focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50";

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      // React 的 autoFocus 不会留下 DOM 属性，这里补一个标记，让对话框打开时能找到它
      data-autofocus={rest.autoFocus ? "" : undefined}
      type="text"
      autoComplete="off"
      autoCorrect="off"
      autoCapitalize="off"
      spellCheck={false}
      className={cx(inputClass, className)}
      {...rest}
    />
  );
}

export function Select({ className, children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={cx(inputClass, "pr-7", className)} {...rest}>
      {children}
    </select>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-[12px] font-medium text-fg-2">{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-[12px] leading-relaxed text-fg-3">{hint}</span>}
    </label>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="radiogroup" className="inline-flex rounded-md border border-line-strong bg-bg p-0.5">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          role="radio"
          aria-checked={opt.value === value}
          onClick={() => onChange(opt.value)}
          className={cx(
            "h-6 rounded-[5px] px-2.5 text-[12px] font-medium transition-colors",
            opt.value === value ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2",
          )}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

/**
 * 滑块：拖动立即生效，双击回到默认值。用系统自带的 range 控件，键盘方向键也能调。
 * 填充的部分用 CSS 变量 `--fill` 画，样式见 styles.css 里的 `.slider`。
 */
export function Slider({
  value,
  min,
  max,
  step,
  label,
  defaultValue,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  label: string;
  defaultValue?: number;
  onChange: (value: number) => void;
}) {
  const fill = max > min ? ((value - min) / (max - min)) * 100 : 0;
  return (
    <input
      type="range"
      aria-label={label}
      className="slider"
      min={min}
      max={max}
      step={step}
      value={value}
      style={{ "--fill": `${fill}%` } as CSSProperties}
      onChange={(e) => onChange(Number(e.currentTarget.value))}
      onDoubleClick={defaultValue === undefined ? undefined : () => onChange(defaultValue)}
    />
  );
}

export function Modal({
  open,
  onClose,
  title,
  width = 440,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  width?: number;
  children: ReactNode;
}) {
  const t = useT();
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="anim-fade fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          // 默认会聚焦第一个可聚焦元素，也就是关闭按钮。有指定的输入框时聚焦它。
          onOpenAutoFocus={(e) => {
            const target = (e.currentTarget as HTMLElement | null)?.querySelector<HTMLElement>(
              "[data-autofocus]",
            );
            if (target) {
              e.preventDefault();
              target.focus();
            }
          }}
          style={{ width }}
          className="anim-pop fixed left-1/2 top-[14vh] z-40 flex max-h-[76vh] max-w-[calc(100vw-32px)] -translate-x-1/2 flex-col rounded-xl bg-surface shadow-pop outline-none"
        >
          <div className="flex h-11 shrink-0 items-center justify-between border-b border-line pl-4 pr-2">
            <Dialog.Title className="text-[13px] font-semibold">{title}</Dialog.Title>
            <Dialog.Close asChild>
              <button
                type="button"
                aria-label={t("action.close")}
                className="inline-flex size-7 items-center justify-center rounded-md text-fg-3 hover:bg-hover hover:text-fg"
              >
                <X {...ICON} />
              </button>
            </Dialog.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** 菜单项放在哪种菜单里：点按钮弹出的，还是右键弹出的。两种菜单的条目组件不能混用。 */
const MenuKind = createContext<"dropdown" | "context">("dropdown");

const MENU_CONTENT = "anim-pop z-50 min-w-[180px] rounded-lg bg-surface p-1 shadow-pop outline-none";

// 菜单关闭时默认会把焦点还给触发它的元素。如果菜单项刚打开了一个对话框，
// 这会把焦点从对话框的输入框里抢走，所以这种情况下不还。
function keepDialogFocus(e: Event) {
  if (document.querySelector("[role=dialog]")) e.preventDefault();
}

/**
 * 右键菜单：在 `children` 上点右键（触屏上长按）弹出 `items`。`items` 用 MenuItem 和 MenuSeparator。
 *
 * 默认在外面包一层带圆角描边的 div。`bare` 时 `children` 必须是单个元素，直接当作触发区域，
 * 打开时它会带上 `data-state="open"`，样式自己定。
 */
export function ContextMenu({
  items,
  children,
  bare,
  disabled,
  onContextMenu,
}: {
  items: ReactNode;
  children: ReactNode;
  bare?: boolean;
  disabled?: boolean;
  /** 菜单打开前调用，可以在这里记下右键点在了什么上面。 */
  onContextMenu?: (e: MouseEvent) => void;
}) {
  return (
    <Context.Root>
      <Context.Trigger asChild disabled={disabled} onContextMenu={onContextMenu}>
        {bare ? (
          (children as ReactElement)
        ) : (
          <div className="rounded-md data-[state=open]:outline data-[state=open]:outline-1 data-[state=open]:-outline-offset-1 data-[state=open]:outline-accent/60">
            {children}
          </div>
        )}
      </Context.Trigger>
      <Context.Portal>
        <Context.Content onCloseAutoFocus={keepDialogFocus} className={MENU_CONTENT}>
          <MenuKind.Provider value="context">{items}</MenuKind.Provider>
        </Context.Content>
      </Context.Portal>
    </Context.Root>
  );
}

/**
 * 鼠标移上去就打开、移开一会儿后关上的弹出层（工具栏上的「列表样式」「折叠」）。
 * 只认鼠标：触屏和笔还是点一下打开。从按钮移到弹出层中间的空隙不会关掉。
 */
export function useHoverOpen(closeDelay = 180) {
  const [open, setOpen] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const hover = {
    onPointerEnter: (e: PointerEvent) => {
      if (e.pointerType !== "mouse") return;
      clearTimeout(timer.current);
      setOpen(true);
    },
    onPointerLeave: (e: PointerEvent) => {
      if (e.pointerType !== "mouse") return;
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setOpen(false), closeDelay);
    },
  };
  return { open, setOpen, hover };
}

export function Menu({
  trigger,
  align = "start",
  hover = false,
  children,
}: {
  trigger: ReactNode;
  align?: "start" | "end";
  /** 鼠标移上去就打开。 */
  hover?: boolean;
  children: ReactNode;
}) {
  const hovering = useHoverOpen();
  const control = hover ? { open: hovering.open, onOpenChange: hovering.setOpen, modal: false } : {};
  const hoverProps = hover ? hovering.hover : {};
  return (
    <Dropdown.Root {...control}>
      <Dropdown.Trigger asChild {...hoverProps}>
        {trigger}
      </Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content
          align={align}
          sideOffset={4}
          onCloseAutoFocus={keepDialogFocus}
          className={MENU_CONTENT}
          {...hoverProps}
        >
          <MenuKind.Provider value="dropdown">{children}</MenuKind.Provider>
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

export function MenuItem({
  icon,
  danger,
  onSelect,
  children,
}: {
  icon?: ReactNode;
  danger?: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  const Item = useContext(MenuKind) === "context" ? Context.Item : Dropdown.Item;
  return (
    <Item
      onSelect={onSelect}
      className={cx(
        "flex h-7 items-center gap-2 rounded-md px-2 text-[13px] outline-none data-[highlighted]:bg-active",
        danger ? "text-danger" : "text-fg",
      )}
    >
      <span className={cx("flex size-4 items-center justify-center", !danger && "text-fg-3")}>{icon}</span>
      {children}
    </Item>
  );
}

/** 子菜单，比如右键菜单里的「分类」。 */
export function MenuSub({ icon, label, children }: { icon?: ReactNode; label: ReactNode; children: ReactNode }) {
  const kind = useContext(MenuKind);
  const trigger = (
    <>
      <span className="flex size-4 items-center justify-center text-fg-3">{icon}</span>
      <span className="flex-1">{label}</span>
      <ChevronRight size={14} className="ml-3 text-fg-3" />
    </>
  );
  const triggerClass =
    "flex h-7 items-center gap-2 rounded-md px-2 text-[13px] text-fg outline-none data-[highlighted]:bg-active data-[state=open]:bg-active";
  if (kind === "context") {
    return (
      <Context.Sub>
        <Context.SubTrigger className={triggerClass}>{trigger}</Context.SubTrigger>
        <Context.Portal>
          <Context.SubContent sideOffset={2} alignOffset={-4} className={MENU_CONTENT}>
            {children}
          </Context.SubContent>
        </Context.Portal>
      </Context.Sub>
    );
  }
  return (
    <Dropdown.Sub>
      <Dropdown.SubTrigger className={triggerClass}>{trigger}</Dropdown.SubTrigger>
      <Dropdown.Portal>
        <Dropdown.SubContent sideOffset={2} alignOffset={-4} className={MENU_CONTENT}>
          {children}
        </Dropdown.SubContent>
      </Dropdown.Portal>
    </Dropdown.Sub>
  );
}

/** 菜单顶上一行灰色的小字，说明菜单作用于什么。 */
export function MenuLabel({ children }: { children: ReactNode }) {
  return <div className="max-w-[260px] truncate px-2 pb-1 pt-1.5 text-[12px] font-medium text-fg-3">{children}</div>;
}

export function MenuSeparator() {
  const Separator = useContext(MenuKind) === "context" ? Context.Separator : Dropdown.Separator;
  return <Separator className="mx-1 my-1 h-px bg-line" />;
}

/** 订阅源的图标。加载不出来时退回到通用的订阅图标。 */
export function Favicon({ src, size = 16 }: { src: string | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [src]);

  if (!src || failed) {
    return (
      <span
        className="inline-flex shrink-0 items-center justify-center rounded-[4px] bg-active text-fg-3"
        style={{ width: size, height: size }}
        aria-hidden
      >
        <Rss size={Math.round(size * 0.68)} strokeWidth={2} />
      </span>
    );
  }
  return (
    <img
      src={src}
      alt=""
      width={size}
      height={size}
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      draggable={false}
      onError={() => setFailed(true)}
      className="shrink-0 rounded-[4px] object-contain"
      style={{ width: size, height: size }}
    />
  );
}

/** 由字符串算出一个稳定的色相，同一个人每次颜色都一样。 */
function hue(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
  return Math.abs(h) % 360;
}

/** 发件人或邮箱账户的头像：取名字的第一个字。 */
export function Avatar({ name, seed, size = 16 }: { name: string; seed?: string | null; size?: number }) {
  const letter = Array.from(name.trim().replace(/^["'<(]+/, ""))[0]?.toUpperCase() ?? "?";
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center rounded-full font-semibold text-white"
      style={{
        width: size,
        height: size,
        fontSize: Math.max(9, Math.round(size * 0.5)),
        background: `hsl(${hue((seed || name).toLowerCase())} 38% 52%)`,
      }}
    >
      {letter}
    </span>
  );
}

/** 应用图标，和 src-tauri/app-icon.svg、public/favicon.svg 是同一个图形。 */
export function Logo({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden>
      <defs>
        <linearGradient id="postflow-logo" x1="0.15" y1="0" x2="0.85" y2="1">
          <stop offset="0" stopColor="#A25BFF" />
          <stop offset="1" stopColor="#6A1FE0" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="15" fill="url(#postflow-logo)" />
      <g fill="none" stroke="#fff" strokeWidth="5.5" strokeLinecap="round">
        <path d="M17 22h30" />
        <path d="M17 32h22" opacity=".8" />
        <path d="M17 42h13" opacity=".55" />
      </g>
    </svg>
  );
}

export const TooltipProvider = Tooltip.Provider;
