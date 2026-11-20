// 设置 → 朗读 里的「声音样本」：ZipVoice 照着念的录音。
//
// 添加：选一个录音文件（任何常见格式），核心解码、去掉前后的静音、只留开头 12 秒，存成 24 kHz 的 WAV；
// 装了语音识别模型的话自动识别出录音里说的话，用户再改。文字要和录音一字不差，不然念出来会走样。
// 样本只存在核心那台电脑上（本机目录/voices/samples），不同步。

import { Mic, Play, Square, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, type VoiceSample } from "../lib/api";
import { errorText, useT } from "../lib/i18n";
import { wavUrl } from "../lib/voices";
import { SettingsHeading } from "./SettingsParts";
import { toast } from "./Toast";
import { Button, IconButton, Spinner, TextInput } from "./ui";

/** 录音文件最大多少（核心那边同样限制）。 */
const MAX_BYTES = 30 * 1024 * 1024;

async function fileBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  }
  return btoa(binary);
}

function SampleRow({
  sample,
  playing,
  onPlay,
  onChanged,
}: {
  sample: VoiceSample;
  playing: boolean;
  onPlay: () => void;
  onChanged: () => Promise<void>;
}) {
  const t = useT();
  const [name, setName] = useState(sample.name);
  const [text, setText] = useState(sample.text);
  const [removing, setRemoving] = useState(false);
  useEffect(() => setName(sample.name), [sample.name]);
  useEffect(() => setText(sample.text), [sample.text]);

  const save = async (patch: { name?: string; text?: string }) => {
    try {
      await api.voicesSampleUpdate(sample.id, patch);
      await onChanged();
    } catch (err) {
      toast(errorText(t, err));
    }
  };

  return (
    <li className="rounded-md border border-line px-3 py-2">
      <div className="flex items-center gap-2">
        <IconButton label={playing ? t("voices.stopSample") : t("voices.playSample")} onClick={onPlay}>
          {playing ? <Square size={12} fill="currentColor" strokeWidth={0} /> : <Play size={14} fill="currentColor" strokeWidth={0} />}
        </IconButton>
        <TextInput
          className="h-7 flex-1"
          aria-label={t("voices.sampleName")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name.trim() && name.trim() !== sample.name && void save({ name })}
        />
        <span className="shrink-0 text-[12px] tabular-nums text-fg-3">{t("voices.sampleSeconds", { n: sample.seconds })}</span>
        <IconButton
          label={t("action.remove")}
          disabled={removing}
          onClick={async () => {
            setRemoving(true);
            try {
              await api.voicesSampleRemove(sample.id);
              await onChanged();
            } catch (err) {
              toast(errorText(t, err));
              setRemoving(false);
            }
          }}
        >
          <Trash2 size={14} strokeWidth={1.75} />
        </IconButton>
      </div>
      <textarea
        aria-label={t("voices.sampleText")}
        placeholder={t("voices.sampleText")}
        value={text}
        rows={2}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => text.trim() !== sample.text && void save({ text })}
        className="mt-2 w-full resize-y rounded-md border border-line-strong bg-bg px-2.5 py-1.5 text-[13px] leading-relaxed text-fg outline-none transition-colors placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent"
      />
      {!sample.text.trim() && <p className="mt-1 text-[12px] text-danger">{t("voices.sampleTextMissing")}</p>}
    </li>
  );
}

export function VoiceSamples({
  samples,
  model,
  onChanged,
}: {
  samples: VoiceSample[];
  /** 装着的 ZipVoice；没装的话列表照样显示，提示先装。 */
  model: string | null;
  onChanged: () => Promise<void>;
}) {
  const t = useT();
  const input = useRef<HTMLInputElement | null>(null);
  const audio = useRef<HTMLAudioElement | null>(null);
  const [adding, setAdding] = useState(false);
  const [playing, setPlaying] = useState<string | null>(null);

  useEffect(
    () => () => {
      audio.current?.pause();
    },
    [],
  );

  const play = async (id: string) => {
    audio.current?.pause();
    if (playing === id) {
      setPlaying(null);
      return;
    }
    const el = audio.current ?? new Audio();
    audio.current = el;
    setPlaying(id);
    try {
      const { audio: data } = await api.voicesSampleAudio(id);
      const url = wavUrl(data);
      el.src = url;
      el.onended = () => {
        URL.revokeObjectURL(url);
        setPlaying(null);
      };
      await el.play();
    } catch (err) {
      setPlaying(null);
      toast(errorText(t, err));
    }
  };

  const add = async (file: File) => {
    if (file.size > MAX_BYTES) {
      toast(errorText(t, { code: "voice_sample_large" }));
      return;
    }
    setAdding(true);
    try {
      const dot = file.name.lastIndexOf(".");
      const name = dot > 0 ? file.name.slice(0, dot) : file.name;
      const ext = dot > 0 ? file.name.slice(dot + 1) : "";
      const sample = await api.voicesSampleAdd(name, await fileBase64(file), ext);
      await onChanged();
      if (!sample.text.trim()) toast(t("voices.sampleNoTranscript"));
    } catch (err) {
      toast(errorText(t, err));
    } finally {
      setAdding(false);
    }
  };

  return (
    <>
      <SettingsHeading>{t("voices.samples")}</SettingsHeading>
      <p className="-mt-2 text-[12px] leading-relaxed text-fg-3">
        {t("voices.samplesHint")} {!model && t("voices.samplesNeedModel")}
      </p>
      {samples.length > 0 && (
        <ul className="flex flex-col gap-1.5">
          {samples.map((s) => (
            <SampleRow key={s.id} sample={s} playing={playing === s.id} onPlay={() => void play(s.id)} onChanged={onChanged} />
          ))}
        </ul>
      )}
      <div className="flex items-center gap-2">
        <input
          ref={input}
          type="file"
          accept="audio/*,.mp3,.m4a,.wav,.ogg,.opus,.flac,.aac"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) void add(file);
          }}
        />
        <Button disabled={adding} onClick={() => input.current?.click()}>
          {adding ? <Spinner size={13} /> : <Mic size={14} />}
          {adding ? t("voices.addingSample") : t("voices.addSample")}
        </Button>
      </div>
    </>
  );
}
