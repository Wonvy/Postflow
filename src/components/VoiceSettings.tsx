// 设置里的「朗读」：用什么声音念（本机声音或系统语音），中文、英文各用哪种声音（可以试听），
// 听新闻的选项（每篇念什么、念完接着念、念完标为已读），以及本机声音模型的下载、删除、导入。
//
// 本机声音是 Kokoro 模型，在核心那台电脑上合成，文字不会发到网上。模型不随安装包带，目录来自
// public/catalog/voices.json，核心每天联网看一次有没有新的。选择记在这个浏览器里（见 lib/voices.ts）。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderOpen, Play, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, type SpeechModel } from "../lib/api";
import { errorText, type MessageKey, useLang, useT } from "../lib/i18n";
import { revealFolder } from "../lib/platform";
import { speechSupported, systemVoiceFor, useSpeech, voiceError } from "../lib/speech";
import { LISTEN_CONTENTS, type ListenContent } from "../lib/listenText";
import { languageName } from "../lib/translate";
import { isTauri } from "../lib/transport";
import {
  pickModel,
  pickSample,
  pickVoice,
  voiceAccent,
  voiceGender,
  voiceLabel,
  type VoiceLang,
  voicesFor,
} from "../lib/voiceNames";
import { forgetVoiceOverview, PREVIEW_TEXT, readySamples, unlockAudio, useVoices, wavUrl } from "../lib/voices";
import { VoiceSamples } from "./VoiceSamples";
import { ModelList } from "./ModelList";
import { CONTENT_LABEL } from "./ReadAloud";
import { SettingRow, SettingsHeading } from "./SettingsParts";
import { toast } from "./Toast";
import { Button, IconButton, Select, Spinner, Switch } from "./ui";

const LANGS: VoiceLang[] = ["zh", "en"];

const CONTENT_HINT: Record<ListenContent, MessageKey> = {
  full: "listen.content.fullHint",
  summary: "listen.content.summaryHint",
  ai: "listen.content.aiHint",
  title: "listen.content.titleHint",
};

/** 系统里的声音。有的浏览器一开始给空的，过一会儿（voiceschanged）才有。 */
function useSystemVoices(): SpeechSynthesisVoice[] {
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>(() => (speechSupported() ? window.speechSynthesis.getVoices() : []));
  useEffect(() => {
    if (!speechSupported()) return;
    const update = () => setVoices(window.speechSynthesis.getVoices());
    window.speechSynthesis.addEventListener("voiceschanged", update);
    update();
    return () => window.speechSynthesis.removeEventListener("voiceschanged", update);
  }, []);
  return voices;
}

/** 试听：同一时间只放一个。 */
function usePreview() {
  const audio = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState<string | null>(null);
  const [loading, setLoading] = useState<string | null>(null);
  const token = useRef(0);

  const stop = () => {
    token.current++;
    audio.current?.pause();
    if (speechSupported()) window.speechSynthesis.cancel();
    setPlaying(null);
    setLoading(null);
  };
  useEffect(() => stop, []);

  const local = async (key: string, model: string, voice: string, lang: VoiceLang, onError: (err: unknown) => void) => {
    stop();
    const mine = token.current;
    if (!audio.current) audio.current = new Audio();
    unlockAudio(audio.current);
    // 正在朗读文章的话先停下
    useSpeech.getState().stop();
    setLoading(key);
    try {
      const rate = useSpeech.getState().rate;
      const r = await api.voicesSpeak(model, voice, PREVIEW_TEXT[lang], rate);
      if (mine !== token.current) return;
      const url = wavUrl(r.audio);
      const el = audio.current;
      el.src = url;
      // 实际念得比要的慢（ZipVoice 的短句）：放快一些补上
      el.defaultPlaybackRate = el.playbackRate = rate / (r.speed ?? rate);
      el.onended = () => {
        URL.revokeObjectURL(url);
        if (mine === token.current) setPlaying(null);
      };
      setLoading(null);
      setPlaying(key);
      await el.play();
    } catch (err) {
      if (mine !== token.current) return;
      setLoading(null);
      setPlaying(null);
      onError(err);
    }
  };

  const system = (key: string, voice: SpeechSynthesisVoice | undefined, lang: VoiceLang) => {
    stop();
    if (!speechSupported()) return;
    useSpeech.getState().stop();
    const mine = token.current;
    const u = new SpeechSynthesisUtterance(PREVIEW_TEXT[lang]);
    u.lang = lang === "zh" ? "zh-CN" : "en-US";
    if (voice) u.voice = voice;
    u.rate = useSpeech.getState().rate;
    u.onend = () => {
      if (mine === token.current) setPlaying(null);
    };
    setPlaying(key);
    window.speechSynthesis.speak(u);
  };

  return { playing, loading, stop, local, system };
}

export function VoiceSettings({ open }: { open: boolean }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const engine = useVoices((s) => s.engine);
  const setEngine = useVoices((s) => s.setEngine);
  const chosen = useVoices((s) => s.chosen);
  const choose = useVoices((s) => s.choose);
  const systemChosen = useVoices((s) => s.system);
  const chooseSystem = useVoices((s) => s.chooseSystem);
  const downloads = useVoices((s) => s.downloads);
  const content = useVoices((s) => s.content);
  const setContent = useVoices((s) => s.setContent);
  const autoNext = useVoices((s) => s.autoNext);
  const setAutoNext = useVoices((s) => s.setAutoNext);
  const markRead = useVoices((s) => s.markRead);
  const setMarkRead = useVoices((s) => s.setMarkRead);
  const systemVoices = useSystemVoices();
  const preview = usePreview();
  const overview = useQuery({
    queryKey: ["voices"],
    queryFn: api.voicesOverview,
    enabled: open,
    staleTime: 0,
  });
  const samples = useQuery({
    queryKey: ["voice-samples"],
    queryFn: api.voicesSamples,
    enabled: open,
    staleTime: 0,
  });
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!open) preview.stop();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const errorOf = (err: unknown) => voiceError(err);

  const run = async (key: string, action: () => Promise<unknown>): Promise<boolean> => {
    setBusy(key);
    try {
      await action();
      forgetVoiceOverview();
      await qc.invalidateQueries({ queryKey: ["voices"] });
      await qc.invalidateQueries({ queryKey: ["voice-samples"] });
      return true;
    } catch (err) {
      toast(errorText(t, errorOf(err)));
      return false;
    } finally {
      setBusy(null);
    }
  };

  const download = (m: SpeechModel) => {
    useVoices.setState((s) => ({
      downloads: { ...s.downloads, [m.id]: { received: 0, total: m.size, state: "downloading" } },
    }));
    api.voicesDownload(m.id).catch((err) => {
      useVoices.setState((s) => ({
        downloads: { ...s.downloads, [m.id]: { received: 0, total: m.size, state: "failed" } },
      }));
      toast(errorText(t, errorOf(err)));
    });
  };

  const models = overview.data?.models ?? [];
  const installed = models.filter((m) => m.installed && m.supported);
  const sampleList = samples.data?.samples ?? [];
  const ready = readySamples(sampleList);
  const zipvoice = installed.find((m) => m.kind === "zipvoice");

  const voiceText = (name: string) => {
    const label = voiceLabel(name, lang);
    const gender = voiceGender(name);
    const accent = voiceAccent(name);
    const base =
      "n" in label
        ? t(gender === "male" ? "voices.maleN" : "voices.femaleN", { n: label.n })
        : t("voices.named", { name: label.name, gender: t(gender === "male" ? "voices.male" : "voices.female") });
    return accent ? `${base} · ${t(accent === "uk" ? "voices.uk" : "voices.us")}` : base;
  };

  const languageRow = (l: VoiceLang) => {
    const modelId = pickModel(models, l, engine, ready);
    const model = models.find((m) => m.id === modelId);
    const key = `${l}`;
    const previewButton = (onPlay: () => void) => (
      <IconButton
        label={preview.playing === key ? t("voices.stopPreview") : t("voices.preview")}
        onClick={() => (preview.playing === key ? preview.stop() : onPlay())}
      >
        {preview.loading === key ? (
          <Spinner size={13} />
        ) : preview.playing === key ? (
          <Square size={12} fill="currentColor" strokeWidth={0} />
        ) : (
          <Play size={14} fill="currentColor" strokeWidth={0} />
        )}
      </IconButton>
    );
    if (model?.kind === "zipvoice") {
      const current = pickSample(ready, chosen[`${model.id}:${l}`]) ?? "";
      return (
        <SettingRow key={l} label={languageName(l, lang)} hint={t("voices.sampleHint", { model: model.name })}>
          <Select
            className="w-44"
            aria-label={t("voices.voiceFor", { lang: languageName(l, lang) })}
            value={current}
            onChange={(e) => {
              choose(model.id, l, e.target.value);
              void preview.local(key, model.id, e.target.value, l, (err) => toast(errorText(t, errorOf(err))));
            }}
          >
            {sampleList
              .filter((s) => ready.includes(s.id))
              .map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
          </Select>
          {previewButton(() => void preview.local(key, model.id, current, l, (err) => toast(errorText(t, errorOf(err)))))}
        </SettingRow>
      );
    }
    if (model) {
      const speakers = model.speakers ?? [];
      const current = pickVoice(speakers, model.options, l, chosen[`${model.id}:${l}`]) ?? "";
      const list = voicesFor(speakers, l);
      const women = list.filter((v) => voiceGender(v) !== "male");
      const men = list.filter((v) => voiceGender(v) === "male");
      return (
        <SettingRow key={l} label={languageName(l, lang)} hint={t("voices.localHint", { model: model.name })}>
          <Select
            className="w-44"
            aria-label={t("voices.voiceFor", { lang: languageName(l, lang) })}
            value={current}
            onChange={(e) => {
              choose(model.id, l, e.target.value);
              void preview.local(key, model.id, e.target.value, l, (err) => toast(errorText(t, errorOf(err))));
            }}
          >
            {[
              [t("voices.female"), women],
              [t("voices.male"), men],
            ].map(([group, voices]) =>
              (voices as string[]).length > 0 ? (
                <optgroup key={group as string} label={group as string}>
                  {(voices as string[]).map((v) => (
                    <option key={v} value={v}>
                      {voiceText(v)}
                    </option>
                  ))}
                </optgroup>
              ) : null,
            )}
          </Select>
          {previewButton(() => void preview.local(key, model.id, current, l, (err) => toast(errorText(t, errorOf(err)))))}
        </SettingRow>
      );
    }
    // 系统语音
    const mine = systemVoices.filter((v) => v.lang.replace("_", "-").toLowerCase().startsWith(l));
    if (!speechSupported() || mine.length === 0) {
      return (
        <SettingRow key={l} label={languageName(l, lang)} hint={t("voices.noSystemVoice")}>
          <span className="text-[12px] text-fg-3">—</span>
        </SettingRow>
      );
    }
    const picked = systemChosen[l] && mine.some((v) => v.voiceURI === systemChosen[l]) ? systemChosen[l] : "";
    return (
      <SettingRow key={l} label={languageName(l, lang)} hint={t("voices.systemHint")}>
        <Select
          className="w-44"
          aria-label={t("voices.voiceFor", { lang: languageName(l, lang) })}
          value={picked}
          onChange={(e) => {
            chooseSystem(l, e.target.value || null);
            const voice = mine.find((v) => v.voiceURI === e.target.value) ?? systemVoiceFor(l === "zh" ? "zh-CN" : "en-US");
            preview.system(key, voice, l);
          }}
        >
          <option value="">{t("voices.systemAuto")}</option>
          {mine.map((v) => (
            <option key={v.voiceURI} value={v.voiceURI}>
              {v.name}
            </option>
          ))}
        </Select>
        {previewButton(() =>
          preview.system(key, mine.find((v) => v.voiceURI === picked) ?? systemVoiceFor(l === "zh" ? "zh-CN" : "en-US"), l),
        )}
      </SettingRow>
    );
  };

  return (
    <div className="flex flex-col gap-4">
      <SettingRow label={t("voices.engine")} hint={t("voices.engineHint")}>
        <Select className="w-44" aria-label={t("voices.engine")} value={engine} onChange={(e) => setEngine(e.target.value)}>
          <option value="auto">{t("voices.auto")}</option>
          {speechSupported() && <option value="system">{t("voices.system")}</option>}
          {installed.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
          {/* 选过的模型被删了：列表里还留着这一项，下面的说明会说改用了系统语音 */}
          {engine !== "auto" && engine !== "system" && !installed.some((m) => m.id === engine) && (
            <option value={engine}>{engine}</option>
          )}
        </Select>
      </SettingRow>

      {LANGS.map(languageRow)}
      <p className="-mt-2 text-[12px] leading-relaxed text-fg-3">{t("voices.otherLanguages")}</p>

      <SettingsHeading>{t("listen.heading")}</SettingsHeading>
      <p className="-mt-2 text-[12px] leading-relaxed text-fg-3">{t("listen.headingHint")}</p>
      <SettingRow label={t("listen.content")} hint={t(CONTENT_HINT[content])}>
        <Select
          className="w-44"
          aria-label={t("listen.content")}
          value={content}
          onChange={(e) => setContent(e.target.value as ListenContent)}
        >
          {LISTEN_CONTENTS.map((c) => (
            <option key={c} value={c}>
              {t(CONTENT_LABEL[c])}
            </option>
          ))}
        </Select>
      </SettingRow>
      <SettingRow label={t("listen.autoNext")} hint={t("listen.autoNextHint")}>
        <Switch label={t("listen.autoNext")} checked={autoNext} onChange={setAutoNext} />
      </SettingRow>
      <SettingRow label={t("listen.markRead")} hint={t("listen.markReadHint")}>
        <Switch label={t("listen.markRead")} checked={markRead} onChange={setMarkRead} />
      </SettingRow>

      {overview.isPending && (
        <div className="flex justify-center py-6 text-fg-3">
          <Spinner />
        </div>
      )}
      {overview.isError && <p className="text-[12px] text-danger">{errorText(t, overview.error)}</p>}

      <SettingsHeading>{t("voices.models")}</SettingsHeading>
      <p className="-mt-2 text-[12px] leading-relaxed text-fg-3">{t("voices.modelsHint")}</p>
      <ModelList
        models={models}
        downloads={downloads}
        busy={busy}
        recommend={installed.length === 0}
        onDownload={download}
        onRemove={(m) => void run(`remove:${m.id}`, () => api.voicesRemove(m.id))}
        onImport={(m, path) => run(`import:${m.id}`, () => api.voicesImport(m.id, path))}
        errorOf={errorOf}
      />

      {(zipvoice || sampleList.length > 0) && (
        <VoiceSamples
          samples={sampleList}
          model={zipvoice?.id ?? null}
          onChanged={async () => {
            forgetVoiceOverview();
            await qc.invalidateQueries({ queryKey: ["voice-samples"] });
          }}
        />
      )}

      <div className="flex flex-wrap items-center gap-2">
        {isTauri && overview.data && (
          <Button onClick={() => void revealFolder(overview.data.dir)}>
            <FolderOpen size={14} />
            {t("subtitles.openFolder")}
          </Button>
        )}
      </div>
      {overview.data && (
        <p className="select-text truncate text-[11px] text-fg-3" title={overview.data.dir}>
          {t("subtitles.folder", { dir: overview.data.dir })}
        </p>
      )}
      <p className="text-[11px] leading-relaxed text-fg-3">{t("voices.privacy")}</p>
    </div>
  );
}
