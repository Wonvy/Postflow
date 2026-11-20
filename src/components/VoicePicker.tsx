// 朗读控制条上直接换声音：不用打开设置。列出正在念的这种语言能用的声音，点一下就从这一句开头起换成它。
// 上面一排是用什么念（自动、系统语音、装着的本机模型），下面是那种念法里的声音；本机声音的选择和
// 「设置 → 朗读」里是同一份。每种声音前面有序号（和设置里的一样），顶上的 ‹ › 一个一个往下换，方便挨个试听。

import * as Popover from "@radix-ui/react-popover";
import { useQuery } from "@tanstack/react-query";
import { AudioLines, Check, ChevronLeft, ChevronRight, Settings2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useLang, useT } from "../lib/i18n";
import { speechSupported, useSpeech } from "../lib/speech";
import { useUi } from "../lib/store";
import { languageName } from "../lib/translate";
import { pickModel, pickSample, pickVoice, usableVoices, type VoiceLang, voiceGender, voicesFor } from "../lib/voiceNames";
import { readySamples, useVoices, voiceOverview } from "../lib/voices";
import { cx, IconButton } from "./ui";
import { useSystemVoices, voiceText } from "./VoiceSettings";

interface Choice {
  key: string;
  label: string;
  current: boolean;
  pick: () => void;
  /** 序号（从 1 起，按列表里的顺序）。「自动挑选」没有。 */
  n?: number;
}

/** 一行可选的声音：选中的打勾，打开时滚到它那里。 */
function ChoiceRow({ choice }: { choice: Choice }) {
  const ref = useRef<HTMLButtonElement | null>(null);
  // 打开时、用 ‹ › 换到它时，滚到看得见的地方
  useEffect(() => {
    if (choice.current) ref.current?.scrollIntoView({ block: "nearest" });
  }, [choice.current]);
  return (
    <button
      ref={ref}
      type="button"
      role="menuitemradio"
      aria-checked={choice.current}
      onClick={choice.pick}
      className={cx(
        "flex h-7 w-full shrink-0 items-center gap-2 rounded-md px-2 text-left text-[13px] outline-none hover:bg-hover focus-visible:bg-hover touch:h-10",
        choice.current ? "text-fg" : "text-fg-2",
      )}
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-accent">
        {choice.current && <Check size={14} />}
      </span>
      <span className="w-6 shrink-0 text-right text-[11px] tabular-nums text-fg-4">{choice.n ?? ""}</span>
      <span className="min-w-0 flex-1 truncate">{choice.label}</span>
    </button>
  );
}

function GroupLabel({ children }: { children: string }) {
  return <div className="px-2 pb-0.5 pt-2 text-[11px] font-medium capitalize text-fg-3">{children}</div>;
}

/** 控制条上「换声音」的按钮和弹出的列表。 */
export function VoicePicker() {
  const t = useT();
  const ui = useLang();
  const [open, setOpen] = useState(false);
  const tag = useSpeech((s) => s.lang);
  const local = useSpeech((s) => s.local);
  const engine = useVoices((s) => s.engine);
  const chosen = useVoices((s) => s.chosen);
  const systemChosen = useVoices((s) => s.system);
  const systemVoices = useSystemVoices();
  const state = useQuery({ queryKey: ["voice-picker"], queryFn: voiceOverview, enabled: open, staleTime: 0 });

  const code = tag.slice(0, 2);
  const lang: VoiceLang | null = code === "zh" || code === "en" ? code : null;
  const models = state.data?.models ?? [];
  const ready = readySamples(state.data?.samples ?? []);
  const usable = lang
    ? models.filter(
        (m) => m.installed && m.supported && m.languages.includes(lang) && usableVoices(m, lang, ready).length > 0,
      )
    : [];
  const modelId = lang ? pickModel(models, lang, engine, ready) : null;
  const model = models.find((m) => m.id === modelId);

  const apply = () => useSpeech.getState().refreshVoice();
  const prefs = useVoices.getState();

  const engines: Choice[] = [
    { key: "auto", label: t("voices.auto"), current: engine === "auto", pick: () => (prefs.setEngine("auto"), apply()) },
    ...(speechSupported()
      ? [{ key: "system", label: t("voices.system"), current: engine === "system", pick: () => (prefs.setEngine("system"), apply()) }]
      : []),
    ...usable.map((m) => ({
      key: m.id,
      label: m.name,
      current: engine === m.id,
      pick: () => (prefs.setEngine(m.id), apply()),
    })),
  ];

  // 下面列哪些声音：本机模型自带的（分女声男声）、ZipVoice 的声音样本，或者系统语音
  const groups: { label?: string; choices: Choice[] }[] = [];
  if (lang && model?.kind === "zipvoice") {
    const current = (local?.model === model.id ? local.voice : null) ?? pickSample(ready, chosen[`${model.id}:${lang}`]);
    groups.push({
      choices: (state.data?.samples ?? [])
        .filter((s) => ready.includes(s.id))
        .map((s) => ({
          key: s.id,
          label: s.name,
          current: s.id === current,
          pick: () => (prefs.choose(model.id, lang, s.id), apply()),
        })),
    });
  } else if (lang && model) {
    const speakers = model.speakers ?? [];
    const current =
      (local?.model === model.id ? local.voice : null) ?? pickVoice(speakers, model.options, lang, chosen[`${model.id}:${lang}`]);
    const list = voicesFor(speakers, lang);
    const row = (v: string): Choice => ({
      key: v,
      label: voiceText(t, ui, v),
      current: v === current,
      pick: () => (prefs.choose(model.id, lang, v), apply()),
    });
    const women = list.filter((v) => voiceGender(v) !== "male").map(row);
    const men = list.filter((v) => voiceGender(v) === "male").map(row);
    if (women.length > 0) groups.push({ label: t("voices.female"), choices: women });
    if (men.length > 0) groups.push({ label: t("voices.male"), choices: men });
  } else if (speechSupported()) {
    const mine = systemVoices.filter((v) => v.lang.replace("_", "-").toLowerCase().startsWith(code));
    const picked = systemChosen[code] && mine.some((v) => v.voiceURI === systemChosen[code]) ? systemChosen[code] : "";
    groups.push({
      choices: [
        { key: "", label: t("voices.systemAuto"), current: picked === "", pick: () => (prefs.chooseSystem(code, null), apply()) },
        ...mine.map((v) => ({
          key: v.voiceURI,
          label: v.name,
          current: v.voiceURI === picked,
          pick: () => (prefs.chooseSystem(code, v.voiceURI), apply()),
        })),
      ],
    });
  }

  // 给声音编号：按列表里的顺序接着往下数（女声在前、男声在后），「自动挑选」不算
  const numbered = groups.flatMap((g) => g.choices).filter((c) => c.key !== "");
  numbered.forEach((c, i) => {
    c.n = i + 1;
  });
  const at = numbered.findIndex((c) => c.current);
  /** 换到前一个（-1）或后一个（1），到头了绕回来。 */
  const step = (delta: number) => {
    if (numbered.length === 0) return;
    const from = at < 0 ? (delta > 0 ? -1 : 0) : at;
    numbered[(from + delta + numbered.length) % numbered.length]?.pick();
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <IconButton label={t("listen.voice")} active={open}>
          <AudioLines size={15} strokeWidth={1.75} />
        </IconButton>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="end"
          sideOffset={6}
          collisionPadding={8}
          aria-label={t("listen.voice")}
          className="anim-pop z-50 flex max-h-[min(70vh,460px)] w-[260px] flex-col rounded-lg bg-surface p-1 shadow-pop outline-none"
        >
          <div className="flex items-center gap-1 pb-1 pl-2 pr-0.5 pt-0.5">
            <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-fg-3">
              {t("voices.voiceFor", { lang: languageName(lang ?? code, ui) })}
            </span>
            {numbered.length > 1 && (
              <>
                <IconButton label={t("listen.prevVoice")} onClick={() => step(-1)}>
                  <ChevronLeft size={14} strokeWidth={1.75} />
                </IconButton>
                <span className="min-w-[44px] text-center text-[11px] tabular-nums text-fg-3" aria-live="polite">
                  {at >= 0 ? `${at + 1} / ${numbered.length}` : `– / ${numbered.length}`}
                </span>
                <IconButton label={t("listen.nextVoice")} onClick={() => step(1)}>
                  <ChevronRight size={14} strokeWidth={1.75} />
                </IconButton>
              </>
            )}
          </div>
          {lang && engines.length > 1 && (
            <div role="radiogroup" aria-label={t("voices.engine")} className="flex flex-wrap gap-1 px-1.5 pb-1.5">
              {engines.map((e) => (
                <button
                  key={e.key}
                  type="button"
                  role="radio"
                  aria-checked={e.current}
                  onClick={e.pick}
                  className={cx(
                    "h-6 max-w-full truncate rounded-md px-2 text-[12px] transition-colors",
                    e.current ? "bg-accent text-white" : "bg-hover text-fg-2 hover:text-fg",
                  )}
                >
                  {e.label}
                </button>
              ))}
            </div>
          )}
          <div role="menu" className="flex min-h-0 flex-col overflow-y-auto border-t border-line pt-1">
            {state.isPending && open ? (
              <div className="px-2 py-2 text-[12px] text-fg-3">{t("voices.preparing")}</div>
            ) : groups.every((g) => g.choices.length === 0) ? (
              <div className="px-2 py-2 text-[12px] text-fg-3">{t("voices.noSystemVoice")}</div>
            ) : (
              groups.map((g) => (
                <div key={g.label ?? "voices"} className="flex flex-col">
                  {g.label && <GroupLabel>{g.label}</GroupLabel>}
                  {g.choices.map((c) => (
                    <ChoiceRow key={c.key || "auto"} choice={c} />
                  ))}
                </div>
              ))
            )}
          </div>
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              useUi.getState().setDialog({ type: "settings", page: "voices" });
            }}
            className="mt-1 flex h-7 shrink-0 items-center gap-2 rounded-md border-t border-line px-2 pt-1 text-left text-[13px] text-fg-2 hover:bg-hover"
          >
            <Settings2 size={14} className="text-fg-3" />
            {t("listen.settings")}
          </button>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
