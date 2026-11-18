// 新手引导：第一次装好、设置完数据目录后弹出，一步步带用户选语言和主题、添加邮箱和订阅、认识界面。
// 每一步都可以跳过，跳过或看完后这台设备上不再自动弹出，设置 → 关于里可以再打开。
//
// 引导里的按钮会打开别的对话框（添加邮箱、添加订阅、设置）。那时引导先藏起来，对话框关掉后回到同一步，
// 并显示刚加好的邮箱和订阅。

import { Check, Keyboard, Languages, LayoutPanelLeft, Mail, MousePointerClick, Puzzle, Rss, Sparkles, Workflow } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { CATALOG_CATEGORIES, type CatalogCategory, catalogLang, starterFeeds } from "../lib/catalog";
import { type MessageKey, useLang, useT } from "../lib/i18n";
import { useNav } from "../lib/queries";
import { welcomeOnStart } from "../lib/welcome";
import { type LangPref, type SettingsPage, type ThemePref, useUi } from "../lib/store";
import { categoryName, useCatalogSubscribe } from "./FeedCatalog";
import { Button, cx, ICON, Kbd, Logo, Modal, Segmented, Select, Spinner } from "./ui";

const STEPS = ["hello", "mail", "feeds", "layout", "more"] as const;
type Step = (typeof STEPS)[number];

/** 第一次进入主界面时打开引导，规则见 `welcomeOnStart`。 */
export function useFirstRunWelcome() {
  const nav = useNav();
  const checked = useRef(false);
  useEffect(() => {
    if (checked.current || !nav.data) return;
    checked.current = true;
    const ui = useUi.getState();
    const next = welcomeOnStart(ui.welcomeDone, nav.data.accounts.length, nav.data.feeds.length);
    if (next === "open") ui.setWelcomeStep(0);
    else if (next === "mark-done") ui.finishWelcome();
  }, [nav.data]);
}

export function WelcomeGuide() {
  const t = useT();
  const step = useUi((s) => s.welcomeStep);
  // 别的对话框（添加邮箱、设置……）开着时先藏起来
  const otherDialog = useUi((s) => s.dialog !== null);
  const setStep = useUi((s) => s.setWelcomeStep);
  const finish = useUi((s) => s.finishWelcome);

  const at = Math.min(Math.max(step ?? 0, 0), STEPS.length - 1);
  const id: Step = STEPS[at] ?? "hello";
  const last = at === STEPS.length - 1;

  return (
    <Modal
      open={step !== null && !otherDialog}
      onClose={finish}
      title={t("welcome.title", { n: at + 1, total: STEPS.length })}
      width={520}
    >
      <div className="flex min-h-[400px] flex-col gap-4 px-5 pb-4 pt-5">
        {id === "hello" && <Hello />}
        {id === "mail" && <MailStep />}
        {id === "feeds" && <FeedsStep />}
        {id === "layout" && <LayoutStep />}
        {id === "more" && <MoreStep />}
      </div>
      <div className="flex items-center gap-3 border-t border-line px-5 py-3">
        {last ? (
          <span className="flex-1" />
        ) : (
          <button type="button" onClick={finish} className="flex-1 text-left text-[12px] text-fg-3 hover:text-fg">
            {t("welcome.skip")}
          </button>
        )}
        <div className="flex gap-1.5" aria-hidden>
          {STEPS.map((s, i) => (
            <span key={s} className={cx("size-1.5 rounded-full", i === at ? "bg-accent" : "bg-line-strong")} />
          ))}
        </div>
        <div className="flex flex-1 justify-end gap-2">
          {at > 0 && (
            <Button className="h-8" onClick={() => setStep(at - 1)}>
              {t("welcome.back")}
            </Button>
          )}
          <Button
            variant="primary"
            className="h-8"
            data-autofocus
            onClick={() => (last ? finish() : setStep(at + 1))}
          >
            {last ? t("welcome.finish") : t("welcome.next")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

function Heading({ icon, title, text }: { icon: ReactNode; title: string; text: string }) {
  return (
    <div className="flex flex-col gap-3">
      {icon}
      <div>
        <h2 className="text-[17px] font-semibold tracking-tight">{title}</h2>
        <p className="mt-1 leading-relaxed text-fg-3">{text}</p>
      </div>
    </div>
  );
}

function StepIcon({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex size-9 items-center justify-center rounded-lg bg-accent-soft text-accent">{children}</span>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="shrink-0 text-[13px] text-fg-2">{label}</span>
      {children}
    </div>
  );
}

function Hello() {
  const t = useT();
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const lang = useUi((s) => s.lang);
  const setLang = useUi((s) => s.setLang);
  return (
    <>
      <Heading icon={<Logo size={36} />} title={t("welcome.hello.title")} text={t("welcome.hello.text")} />
      <div className="flex flex-col gap-3 rounded-lg border border-line p-3">
        <Row label={t("settings.language")}>
          <div className="w-40">
            <Select value={lang === "zh" || lang === "en" ? lang : "system"} onChange={(e) => setLang(e.target.value as LangPref)}>
              <option value="system">{t("settings.langSystem")}</option>
              <option value="zh">中文</option>
              <option value="en">English</option>
            </Select>
          </div>
        </Row>
        <Row label={t("settings.theme")}>
          <Segmented<ThemePref>
            value={theme}
            onChange={setTheme}
            options={[
              { value: "system", label: t("settings.themeSystem") },
              { value: "dark", label: t("settings.themeDark") },
              { value: "light", label: t("settings.themeLight") },
            ]}
          />
        </Row>
      </div>
    </>
  );
}

/** 已经加好的邮箱或订阅，最多列几个。 */
function Added({ names, more }: { names: string[]; more: number }) {
  const t = useT();
  if (names.length === 0) return null;
  return (
    <div className="flex flex-col gap-1 rounded-lg border border-line p-3 text-[13px]">
      {names.map((name, i) => (
        <span key={`${name}-${i}`} className="flex items-center gap-2 text-fg-2">
          <Check size={14} strokeWidth={2} className="shrink-0 text-accent" />
          <span className="truncate">{name}</span>
        </span>
      ))}
      {more > 0 && <span className="pl-6 text-[12px] text-fg-3">{t("welcome.more", { n: more })}</span>}
    </div>
  );
}

const SHOWN = 4;

function MailStep() {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  const accounts = useNav().data?.accounts ?? [];
  return (
    <>
      <Heading
        icon={
          <StepIcon>
            <Mail {...ICON} />
          </StepIcon>
        }
        title={t("welcome.mail.title")}
        text={t("welcome.mail.text")}
      />
      <Added names={accounts.slice(0, SHOWN).map((a) => a.email || a.name)} more={accounts.length - SHOWN} />
      <div className="flex items-center gap-3">
        <Button className="h-8" onClick={() => setDialog({ type: "add-account" })}>
          {accounts.length > 0 ? t("welcome.mail.another") : t("action.addAccount")}
        </Button>
        <span className="text-[12px] text-fg-3">{t("welcome.later")}</span>
      </div>
    </>
  );
}

function FeedsStep() {
  const t = useT();
  const lang = useLang();
  const setDialog = useUi((s) => s.setDialog);
  const feeds = useNav().data?.feeds ?? [];
  // 选几个感兴趣的分类，一次订阅每类几个推荐的官方源，放进以分类命名的文件夹
  const [picked, setPicked] = useState<CatalogCategory[]>([]);
  const { subscribed, adding, subscribe } = useCatalogSubscribe();
  const starters = starterFeeds(picked, catalogLang(lang)).filter((f) => !subscribed.has(f.id));
  const busy = adding.size > 0;
  const toggle = (c: CatalogCategory) =>
    setPicked((p) => (p.includes(c) ? p.filter((x) => x !== c) : [...p, c]));
  return (
    <>
      <Heading
        icon={
          <StepIcon>
            <Rss {...ICON} />
          </StepIcon>
        }
        title={t("welcome.feeds.title")}
        text={t("welcome.feeds.text")}
      />
      <div>
        <p className="mb-2 text-[12px] font-medium text-fg-2">{t("welcome.feeds.pick")}</p>
        <div className="flex flex-wrap gap-1.5">
          {CATALOG_CATEGORIES.map((c) => (
            <button
              key={c}
              type="button"
              aria-pressed={picked.includes(c)}
              onClick={() => toggle(c)}
              className={cx(
                "flex h-7 items-center gap-1 rounded-full border px-3 text-[12px] transition-colors",
                picked.includes(c)
                  ? "border-accent bg-accent-soft text-accent"
                  : "border-line-strong text-fg-2 hover:bg-hover hover:text-fg",
              )}
            >
              {picked.includes(c) && <Check size={12} strokeWidth={2.25} />}
              {categoryName(t, c)}
            </button>
          ))}
        </div>
      </div>
      <Added names={feeds.slice(0, SHOWN).map((f) => f.title || f.url)} more={feeds.length - SHOWN} />
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          className="h-8"
          disabled={busy || starters.length === 0}
          onClick={() => void subscribe(starters, (f) => categoryName(t, f.category))}
        >
          {busy && <Spinner size={12} />}
          {t("welcome.feeds.subscribePicked", { n: starters.length })}
        </Button>
        <Button className="h-8" onClick={() => setDialog({ type: "add-feed" })}>
          {feeds.length > 0 ? t("welcome.feeds.another") : t("welcome.feeds.add")}
        </Button>
        <Button className="h-8" onClick={() => setDialog({ type: "settings", page: "feeds" })}>
          {t("welcome.feeds.opml")}
        </Button>
      </div>
    </>
  );
}

function LayoutStep() {
  const t = useT();
  const keys: [string, MessageKey][] = [
    ["J / K", "welcome.key.move"],
    ["E", "welcome.key.archive"],
    ["S", "welcome.key.star"],
    ["U", "welcome.key.read"],
    ["/", "welcome.key.search"],
    ["C", "welcome.key.compose"],
    ["F", "welcome.key.maximize"],
  ];
  return (
    <>
      <Heading
        icon={
          <StepIcon>
            <LayoutPanelLeft {...ICON} />
          </StepIcon>
        }
        title={t("welcome.layout.title")}
        text={t("welcome.layout.text")}
      />
      {/* 三栏示意 */}
      <div className="grid h-[72px] grid-cols-[1fr_1.4fr_2fr] overflow-hidden rounded-lg border border-line text-[12px]">
        <div className="flex items-center justify-center bg-sidebar px-2 text-center text-fg-2">
          {t("welcome.layout.sidebar")}
        </div>
        <div className="flex items-center justify-center border-x border-line px-2 text-center text-fg-2">
          {t("welcome.layout.list")}
        </div>
        <div className="flex items-center justify-center px-2 text-center text-fg-2">{t("welcome.layout.reader")}</div>
      </div>
      <p className="flex items-start gap-2 text-[13px] leading-relaxed text-fg-2">
        <MousePointerClick size={15} strokeWidth={1.75} className="mt-0.5 shrink-0 text-fg-3" />
        {t("welcome.layout.rightClick")}
      </p>
      <div className="flex items-start gap-2 text-[13px] text-fg-2">
        <Keyboard size={15} strokeWidth={1.75} className="mt-0.5 shrink-0 text-fg-3" />
        <div className="grid flex-1 grid-cols-2 gap-x-4 gap-y-1">
          {keys.map(([k, label]) => (
            <span key={k} className="flex items-center gap-2">
              <Kbd>{k}</Kbd>
              <span className="text-fg-3">{t(label)}</span>
            </span>
          ))}
        </div>
      </div>
    </>
  );
}

function MoreStep() {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  const items: { page: SettingsPage; icon: ReactNode; title: MessageKey; text: MessageKey }[] = [
    { page: "rules", icon: <Workflow {...ICON} />, title: "welcome.more.rules", text: "welcome.more.rulesText" },
    { page: "translation", icon: <Languages {...ICON} />, title: "welcome.more.translate", text: "welcome.more.translateText" },
    { page: "ai", icon: <Sparkles {...ICON} />, title: "welcome.more.ai", text: "welcome.more.aiText" },
    { page: "plugins", icon: <Puzzle {...ICON} />, title: "welcome.more.plugins", text: "welcome.more.pluginsText" },
  ];
  return (
    <>
      <Heading
        icon={
          <StepIcon>
            <Check {...ICON} />
          </StepIcon>
        }
        title={t("welcome.done.title")}
        text={t("welcome.done.text")}
      />
      <div className="grid grid-cols-2 gap-2">
        {items.map((it) => (
          <button
            key={it.page}
            type="button"
            onClick={() => setDialog({ type: "settings", page: it.page })}
            className="flex items-start gap-2.5 rounded-lg border border-line p-3 text-left transition-colors hover:bg-hover"
          >
            <span className="mt-px text-fg-3">{it.icon}</span>
            <span className="min-w-0">
              <span className="block text-[13px] font-medium text-fg">{t(it.title)}</span>
              <span className="mt-0.5 block text-[12px] leading-relaxed text-fg-3">{t(it.text)}</span>
            </span>
          </button>
        ))}
      </div>
    </>
  );
}
