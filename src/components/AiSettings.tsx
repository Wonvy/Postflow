// 设置页里的「AI」：接入的服务、主要和快速任务各用哪个、邮件能不能发给云端、用量。

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ExternalLink, Pencil, Plus, Sparkles, Trash2 } from "lucide-react";
import {
  siAlibabacloud,
  siDeepseek,
  siGooglegemini,
  siKimi,
  siLmstudio,
  siOllama,
  siOpenrouter,
} from "simple-icons";
import { type FormEvent, useEffect, useId, useState } from "react";
import { type AiPreset, type AiProviderView, api, type Category } from "../lib/api";
import { errorDetail, errorText, useLang, useT } from "../lib/i18n";
import { defaultCategories, useCategories, useLabelColor } from "../lib/labels";
import { openExternal } from "../lib/platform";
import { keys, useSettings } from "../lib/queries";
import { type Glyph, GlyphIcon, logo, text } from "./ProviderIcon";
import { SettingRow } from "./SettingsParts";
import { Button, Field, Modal, Select, Spinner, Switch, TextInput } from "./ui";

const AI_GLYPHS: Record<string, Glyph> = {
  deepseek: logo(siDeepseek),
  qwen: logo(siAlibabacloud),
  qwen_intl: logo(siAlibabacloud),
  gemini: logo(siGooglegemini),
  openai: text("AI", "#10A37F"),
  moonshot: logo(siKimi),
  zhipu: text("智", "#3859FF"),
  siliconflow: text("硅", "#7C3AED"),
  openrouter: logo(siOpenrouter),
  ollama: logo(siOllama),
  lmstudio: logo(siLmstudio),
};

export function AiIcon({ preset, size = 16 }: { preset: string; size?: number }) {
  return <GlyphIcon glyph={AI_GLYPHS[preset]} size={size} fallback={Sparkles} />;
}

export const aiKeys = {
  providers: ["ai", "providers"] as const,
  presets: ["ai", "presets"] as const,
  usage: ["ai", "usage"] as const,
};

export function useAiProviders(enabled = true) {
  return useQuery({ queryKey: aiKeys.providers, queryFn: api.aiProviders, enabled });
}

function formatTokens(n: number): string {
  return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n);
}

export function AiSettings({ open }: { open: boolean }) {
  const t = useT();
  const qc = useQueryClient();
  const providers = useAiProviders(open);
  const usage = useQuery({ queryKey: aiKeys.usage, queryFn: api.aiUsage, enabled: open });
  const settings = useSettings();
  const [editing, setEditing] = useState<AiProviderView | "new" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const data = providers.data;
  const list = data?.providers ?? [];
  const nameOf = (id: string) => list.find((p) => p.id === id)?.name ?? id;

  const set = async (key: string, value: unknown) => {
    setError(null);
    try {
      await api.setSetting(key, value);
      await qc.invalidateQueries({ queryKey: aiKeys.providers });
      void qc.invalidateQueries({ queryKey: keys.settings });
    } catch (err) {
      setError(errorText(t, err));
    }
  };

  const remove = async (id: string) => {
    setError(null);
    try {
      await api.removeAiProvider(id);
      await qc.invalidateQueries({ queryKey: aiKeys.providers });
    } catch (err) {
      setError(errorText(t, err));
    }
  };

  return (
    <>
      <div>
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-fg">{t("ai.services")}</div>
            <div className="mt-0.5 text-[12px] leading-relaxed text-fg-3">{t("ai.servicesHint")}</div>
          </div>
          <Button onClick={() => setEditing("new")}>
            <Plus size={14} />
            {t("ai.add")}
          </Button>
        </div>
        {list.length > 0 && (
          <ul className="mt-2 flex flex-col">
            {list.map((p) => (
              <li key={p.id} className="flex h-9 items-center gap-2.5">
                <AiIcon preset={p.preset} size={18} />
                <span className="min-w-0 flex-1 truncate">
                  <span className="text-fg">{p.name}</span>
                  <span className="text-[12px] text-fg-3"> · {p.model || t("ai.noModel")}</span>
                </span>
                {p.local && <Badge>{t("ai.local")}</Badge>}
                {!p.local && !p.has_key && <Badge warn>{t("ai.noKey")}</Badge>}
                <button
                  type="button"
                  aria-label={t("action.edit")}
                  title={t("action.edit")}
                  onClick={() => setEditing(p)}
                  className="flex size-6 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
                >
                  <Pencil size={13} />
                </button>
                <button
                  type="button"
                  aria-label={t("action.remove")}
                  title={t("action.remove")}
                  onClick={() => void remove(p.id)}
                  className="flex size-6 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-danger"
                >
                  <Trash2 size={13} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {list.length > 0 && (
        <>
          <SettingRow label={t("ai.main")} hint={t("ai.mainHint")}>
            <Select className="w-44" value={data?.default ?? ""} onChange={(e) => void set("ai.default", e.target.value)}>
              {list.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </SettingRow>
          <SettingRow label={t("ai.fast")} hint={t("ai.fastHint")}>
            <Select
              className="w-44"
              value={data?.fast ?? ""}
              onChange={(e) => void set("ai.fast", e.target.value || null)}
            >
              <option value="">{t("ai.sameAsMain")}</option>
              {list.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </SettingRow>
          <SettingRow label={t("ai.listAbstracts")} hint={t("ai.listAbstractsHint")}>
            <Switch
              label={t("ai.listAbstracts")}
              checked={settings.data?.["ai.list_abstracts"] === true}
              onChange={(v) => void set("ai.list_abstracts", v)}
            />
          </SettingRow>
          <SettingRow label={t("ai.allowMail")} hint={t("ai.allowMailHint")}>
            <Switch
              label={t("ai.allowMail")}
              checked={data?.allow_mail ?? false}
              onChange={(v) => void set("ai.allow_mail", v)}
            />
          </SettingRow>
          {usage.data && usage.data.last30.length > 0 && (
            <SettingRow label={t("ai.usage")}>
              <span className="max-w-[300px] truncate text-[12px] text-fg-2">
                {usage.data.last30
                  .map((u) =>
                    t("ai.usageLine", {
                      name: nameOf(u.provider),
                      n: u.requests,
                      tokens: formatTokens(u.input + u.output),
                    }),
                  )
                  .join(" · ")}
              </span>
            </SettingRow>
          )}
        </>
      )}
      {error && <div className="text-[12px] text-danger">{error}</div>}
      <ProviderDialog editing={editing} onClose={() => setEditing(null)} />
    </>
  );
}

/** 分类的类别：名称和一句说明。改完离开输入框时保存。标题和说明由设置页给出。 */
export function CategorySettings() {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const saved = useCategories();
  const labelColor = useLabelColor();
  const settings = useSettings();
  const [draft, setDraft] = useState<Category[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const list = draft ?? saved;
  const auto = settings.data?.["ai.auto_classify"] === true;

  const save = (next: Category[]) => {
    const clean = next.map((c) => ({ name: c.name.trim(), description: c.description.trim() })).filter((c) => c.name);
    qc.setQueryData(keys.settings, (old: Record<string, unknown> | undefined) => ({ ...old, "ai.categories": clean }));
    void api.setSetting("ai.categories", clean);
    setDraft(null);
  };
  const change = (i: number, patch: Partial<Category>) =>
    setDraft((list ?? []).map((c, j) => (j === i ? { ...c, ...patch } : c)));
  const setAuto = (v: boolean) => {
    qc.setQueryData(keys.settings, (old: Record<string, unknown> | undefined) => ({ ...old, "ai.auto_classify": v }));
    void api.setSetting("ai.auto_classify", v);
  };
  const classifyNow = async () => {
    setBusy(true);
    setNote(null);
    try {
      const { labels } = await api.classify();
      setNote(t("ai.classified", { n: Object.keys(labels).length }));
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: ["items"] });
    } catch (err) {
      setNote(errorText(t, err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div>
        {list === null ? (
          <Button onClick={() => save(defaultCategories(lang))}>{t("ai.useDefaults")}</Button>
        ) : (
          <div className="flex flex-col gap-1.5">
            {list.map((c, i) => (
              <div key={i} className="flex items-center gap-2">
                <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: labelColor(c.name) }} />
                <div className="w-28 shrink-0">
                  <TextInput
                    className="h-7"
                    aria-label={t("ai.categoryName")}
                    value={c.name}
                    onChange={(e) => change(i, { name: e.target.value })}
                    onBlur={() => draft && save(draft)}
                  />
                </div>
                <div className="min-w-0 flex-1">
                  <TextInput
                    className="h-7"
                    aria-label={t("ai.categoryDescription")}
                    placeholder={t("ai.categoryDescription")}
                    value={c.description}
                    onChange={(e) => change(i, { description: e.target.value })}
                    onBlur={() => draft && save(draft)}
                  />
                </div>
                <button
                  type="button"
                  aria-label={t("action.remove")}
                  onClick={() => save(list.filter((_, j) => j !== i))}
                  className="flex size-6 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-danger"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
            <div>
              <Button variant="ghost" className="h-7 px-2 text-[12px]" onClick={() => setDraft([...list, { name: "", description: "" }])}>
                <Plus size={13} />
                {t("ai.addCategory")}
              </Button>
            </div>
          </div>
        )}
      </div>
      <SettingRow label={t("ai.autoClassify")} hint={t("ai.autoClassifyHint")}>
        <Switch label={t("ai.autoClassify")} checked={auto} disabled={!list?.length} onChange={setAuto} />
      </SettingRow>
      <SettingRow label={t("ai.classifyNow")} hint={note ?? undefined}>
        <Button busy={busy} disabled={!list?.length} onClick={() => void classifyNow()}>
          {t("ai.classifyNow")}
        </Button>
      </SettingRow>
    </>
  );
}

function Badge({ children, warn = false }: { children: string; warn?: boolean }) {
  return (
    <span
      className={
        "shrink-0 rounded px-1.5 py-px text-[11px] " + (warn ? "bg-warning/15 text-warning" : "bg-active text-fg-3")
      }
    >
      {children}
    </span>
  );
}

/** 添加或修改一个服务：先选服务商，再填密钥、模型。 */
function ProviderDialog({ editing, onClose }: { editing: AiProviderView | "new" | null; onClose: () => void }) {
  const t = useT();
  const presets = useQuery({ queryKey: aiKeys.presets, queryFn: api.aiPresets, enabled: editing !== null });
  const [preset, setPreset] = useState<AiPreset | null>(null);

  useEffect(() => {
    if (editing === null) setPreset(null);
  }, [editing]);

  const existing = editing !== null && editing !== "new" ? editing : null;
  const chosen =
    existing ??
    (preset && {
      id: "",
      preset: preset.id,
      name: preset.name,
      base_url: preset.base_url,
      model: preset.model,
      proxy: null,
      has_key: false,
      local: preset.local,
    });
  const keyUrl = presets.data?.presets.find((p) => p.id === chosen?.preset)?.key_url;

  return (
    <Modal open={editing !== null} onClose={onClose} title={existing ? t("ai.editTitle") : t("ai.addTitle")} width={460}>
      {chosen ? (
        <ProviderForm
          key={chosen.id || chosen.preset}
          initial={chosen}
          keyUrl={keyUrl}
          onBack={existing ? undefined : () => setPreset(null)}
          onDone={onClose}
        />
      ) : (
        <div className="p-4">
          <p className="mb-3 text-[12px] leading-relaxed text-fg-3">{t("ai.chooseHint")}</p>
          <div className="grid grid-cols-3 gap-2">
            {(presets.data?.presets ?? []).map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => setPreset(p)}
                className="flex h-16 flex-col items-center justify-center gap-1.5 rounded-lg border border-line bg-surface px-2 text-[12px] text-fg-2 transition-colors hover:border-line-strong hover:bg-hover hover:text-fg"
              >
                <AiIcon preset={p.id} size={22} />
                <span className="max-w-full truncate">{p.name}</span>
              </button>
            ))}
            <button
              type="button"
              onClick={() =>
                setPreset({ id: "custom", name: "", base_url: "https://", model: "", key_url: "", local: false })
              }
              className="flex h-16 flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed border-line-strong px-2 text-[12px] text-fg-2 hover:bg-hover hover:text-fg"
            >
              <AiIcon preset="custom" size={22} />
              <span>{t("ai.custom")}</span>
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}

function ProviderForm({
  initial,
  keyUrl,
  onBack,
  onDone,
}: {
  initial: AiProviderView;
  keyUrl?: string;
  onBack?: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const qc = useQueryClient();
  const listId = useId();
  const [name, setName] = useState(initial.name);
  const [baseUrl, setBaseUrl] = useState(initial.base_url);
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState(initial.model);
  const [proxy, setProxy] = useState(initial.proxy ?? "");
  const [models, setModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<"models" | "test" | "save" | null>(null);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  const draft = () => ({
    id: initial.id,
    preset: initial.preset,
    name: name.trim(),
    base_url: baseUrl.trim(),
    model: model.trim(),
    proxy: proxy.trim() || null,
  });
  const fail = (err: unknown) => {
    const detail = errorDetail(err);
    setNote({ ok: false, text: detail ? `${errorText(t, err)}（${detail}）` : errorText(t, err) });
  };

  const loadModels = async (quiet = false) => {
    setBusy("models");
    if (!quiet) setNote(null);
    try {
      const { models } = await api.aiModels(draft(), apiKey || undefined);
      setModels(models);
      if (!model.trim() && models[0]) setModel(models[0]);
    } catch (err) {
      if (!quiet) fail(err);
    } finally {
      setBusy(null);
    }
  };

  // 本机服务、或者已经存过密钥的，打开就拉一次模型列表
  useEffect(() => {
    if (initial.local || initial.has_key) void loadModels(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const test = async () => {
    setBusy("test");
    setNote(null);
    try {
      await api.testAiProvider(draft(), apiKey || undefined);
      setNote({ ok: true, text: t("ai.testOk") });
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy("save");
    setNote(null);
    try {
      await api.saveAiProvider(draft(), apiKey || undefined);
      await qc.invalidateQueries({ queryKey: aiKeys.providers });
      onDone();
    } catch (err) {
      fail(err);
      setBusy(null);
    }
  };

  return (
    <form onSubmit={(e) => void save(e)} className="flex flex-col gap-4 p-4">
      <div className="flex items-center gap-2.5">
        {onBack && (
          <button
            type="button"
            aria-label={t("account.back")}
            onClick={onBack}
            className="flex size-6 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
          >
            <ArrowLeft size={14} />
          </button>
        )}
        <AiIcon preset={initial.preset} size={22} />
        <span className="font-medium">{initial.name || t("ai.custom")}</span>
      </div>
      <Field label={t("ai.name")}>
        <TextInput value={name} onChange={(e) => setName(e.target.value)} placeholder={initial.name} />
      </Field>
      <Field label={t("ai.baseUrl")}>
        <TextInput value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} spellCheck={false} />
      </Field>
      {!initial.local && (
        <Field label={t("ai.apiKey")}>
          <TextInput
            type="password"
            autoFocus={!initial.has_key}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={initial.has_key ? t("ai.keyKept") : "sk-…"}
            autoComplete="off"
            spellCheck={false}
          />
          {keyUrl && (
            <button
              type="button"
              onClick={() => void openExternal(keyUrl)}
              className="mt-1 inline-flex items-center gap-1 text-[12px] text-accent hover:underline"
            >
              {t("ai.getKey")}
              <ExternalLink size={11} />
            </button>
          )}
        </Field>
      )}
      <Field label={t("ai.model")}>
        <div className="flex gap-2">
          <TextInput
            list={listId}
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder={t("ai.modelPlaceholder")}
            spellCheck={false}
          />
          <Button busy={busy === "models"} onClick={() => void loadModels()}>
            {t("ai.loadModels")}
          </Button>
        </div>
        <datalist id={listId}>
          {models.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </Field>
      {!initial.local && (
        <Field label={t("ai.proxy")}>
          <TextInput
            value={proxy}
            onChange={(e) => setProxy(e.target.value)}
            placeholder="http://127.0.0.1:7890"
            spellCheck={false}
          />
          <p className="mt-1 text-[12px] text-fg-3">{t("ai.proxyHint")}</p>
        </Field>
      )}
      {note && (
        <p role="status" className={"text-[12px] " + (note.ok ? "text-accent" : "text-danger")}>
          {note.text}
        </p>
      )}
      <div className="flex items-center justify-between gap-2">
        <Button busy={busy === "test"} onClick={() => void test()}>
          {busy === "test" ? <Spinner size={13} /> : null}
          {t("ai.test")}
        </Button>
        <div className="flex gap-2">
          <Button variant="ghost" onClick={onDone}>
            {t("action.cancel")}
          </Button>
          <Button variant="primary" type="submit" busy={busy === "save"} disabled={!baseUrl.trim() || !model.trim()}>
            {t("action.save")}
          </Button>
        </div>
      </div>
    </form>
  );
}
