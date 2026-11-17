import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, ExternalLink, Info, Mail, TriangleAlert } from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import {
  type Account,
  type AccountChanges,
  type AccountGuess,
  api,
  type Preset,
  type Security,
} from "../lib/api";
import { errorDetail, errorText, type MessageKey, type Translate, useLang, useT } from "../lib/i18n";
import { openExternal } from "../lib/platform";
import { keys, useNav, usePresets, useSettings } from "../lib/queries";
import { useUi } from "../lib/store";
import { shortTime } from "../lib/time";
import { ProviderIcon, providerName, providerOf } from "./ProviderIcon";
import { Button, cx, Field, Modal, Select, Spinner, Switch, TextInput } from "./ui";

/** 各服务商获取密码的说明。 */
function hintFor(preset: Preset | null, recognized: boolean): MessageKey | null {
  if (!preset) return recognized ? null : "hint.custom";
  switch (preset.id) {
    case "qq":
      return "hint.qq";
    case "exmail":
      return "hint.exmail";
    case "163":
    case "126":
    case "yeah":
    case "188":
      return "hint.netease";
    case "qiye163":
      return "hint.qiye163";
    case "qiye_aliyun":
      return "hint.qiye_aliyun";
    case "feishu":
    case "lark":
      return "hint.feishu";
    case "gmail":
    case "workspace":
      return "hint.gmail";
    case "icloud":
    case "icloud_custom":
      return "hint.icloud";
  }
  switch (preset.password_kind) {
    case "oauth":
      return "hint.oauth";
    case "auth_code":
      return "hint.auth_code";
    case "app_password":
      return "hint.app_password";
    default:
      return null;
  }
}

function passwordLabel(t: Translate, preset: Preset | null): string {
  return t(`account.${preset?.password_kind ?? "password"}` as MessageKey);
}

/** 生成专用密码、授权码的页面，提示里直接给链接。 */
const HELP_URL: Record<string, string> = {
  icloud: "https://account.apple.com/account/manage",
  icloud_custom: "https://account.apple.com/account/manage",
};

function Hint({
  text,
  tone = "info",
  link,
}: {
  text: string;
  tone?: "info" | "warning";
  link?: { label: string; url: string };
}) {
  const Icon = tone === "warning" ? TriangleAlert : Info;
  return (
    <p className="flex items-start gap-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] leading-relaxed text-fg-2">
      <Icon
        size={14}
        strokeWidth={2}
        className={cx("mt-px shrink-0", tone === "warning" ? "text-warning" : "text-fg-3")}
      />
      <span className="select-text">
        {text}
        {link && (
          <button
            type="button"
            onClick={() => void openExternal(link.url)}
            className="ml-1 text-accent hover:underline"
          >
            {link.label}
          </button>
        )}
      </span>
    </p>
  );
}

/** 选了服务商但只填了名字时，补上这个服务商的域名。 */
function completeAddress(input: string, preset: Preset | null | undefined): string {
  const s = input.trim();
  if (!s || s.includes("@") || !preset?.domains[0]) return s;
  return `${s}@${preset.domains[0]}`;
}

interface Failure {
  text: string;
  detail: string | null;
}

function failure(t: Translate, err: unknown): Failure {
  return { text: errorText(t, err), detail: errorDetail(err) };
}

function ErrorLine({ error }: { error: Failure }) {
  const t = useT();
  return (
    <div role="alert" className="flex items-start gap-2 text-[13px] leading-relaxed text-danger">
      <TriangleAlert size={14} strokeWidth={2} className="mt-0.5 shrink-0" />
      <div className="min-w-0">
        <p>{error.text}</p>
        {error.detail && (
          <p className="mt-1 select-text break-words text-[12px] text-fg-3">
            {t("account.serverSaid", { text: error.detail })}
          </p>
        )}
      </div>
    </div>
  );
}

/** 注册 Azure 应用的说明，在说明文档里。 */
const OAUTH_DOCS: Record<"zh" | "en", string> = {
  en: "https://github.com/Wonvy/Postflow#outlookcom-and-microsoft-365",
  zh: "https://github.com/Wonvy/Postflow/blob/main/README.zh-CN.md#outlookcom-和-microsoft-365",
};

/** Outlook、Microsoft 365 这类只能用微软账户登录的服务商。 */
const usesOAuth = (preset: Preset | null | undefined) => preset?.password_kind === "oauth";

/**
 * 用微软账户登录：在系统浏览器里打开微软的登录页，等它把授权送回本机，再交给 `onSignedIn`
 * 去添加或更新账户。还没设置 Azure 应用 ID 时，先让用户填一个。
 */
function MicrosoftSignIn({
  email,
  again,
  onSignedIn,
}: {
  email: string;
  again?: boolean;
  onSignedIn: (flowId: string) => Promise<void>;
}) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const config = useQuery({ queryKey: ["oauth-config"], queryFn: api.oauthConfig });
  const [phase, setPhase] = useState<"idle" | "waiting" | "finishing">("idle");
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<Failure | null>(null);
  const settings = useSettings();
  const savedId = settings.data?.["mail.microsoft_client_id"];
  const [clientId, setClientId] = useState("");
  /** 已经有应用 ID 时，用户点了「更换应用 ID」。填错了 ID 也能改回来。 */
  const [editingId, setEditingId] = useState(false);
  const [saving, setSaving] = useState(false);
  /** 当前这次登录。取消或者换了一次时，旧的结果作废。 */
  const flow = useRef<string | null>(null);

  // 对话框关了还在等的话，告诉核心别等了
  useEffect(
    () => () => {
      if (flow.current) void api.oauthCancel(flow.current).catch(() => {});
    },
    [],
  );

  const start = async () => {
    setError(null);
    try {
      const started = await api.oauthStart(email);
      flow.current = started.flow_id;
      setUrl(started.url);
      setPhase("waiting");
      void openExternal(started.url);
      await api.oauthWait(started.flow_id);
      if (flow.current !== started.flow_id) return;
      setPhase("finishing");
      await onSignedIn(started.flow_id);
      flow.current = null;
    } catch (err) {
      if (err && typeof err === "object" && "code" in err && err.code === "oauth_cancelled") return;
      flow.current = null;
      setError(failure(t, err));
      setPhase("idle");
    }
  };

  const cancel = () => {
    const id = flow.current;
    flow.current = null;
    setPhase("idle");
    if (id) void api.oauthCancel(id).catch(() => {});
  };

  const saveClientId = async () => {
    if (!clientId.trim()) return;
    setSaving(true);
    try {
      await api.setSetting("mail.microsoft_client_id", clientId.trim());
      await qc.invalidateQueries({ queryKey: ["oauth-config"] });
      void qc.invalidateQueries({ queryKey: keys.settings });
      setEditingId(false);
      setError(null);
    } catch (err) {
      setError(failure(t, err));
    } finally {
      setSaving(false);
    }
  };

  if ((config.data && !config.data.microsoft) || editingId) {
    return (
      <div className="flex flex-col gap-3">
        <Hint
          tone={editingId ? "info" : "warning"}
          text={t(editingId ? "oauth.changeClientHint" : "oauth.needClient")}
          link={{ label: t("oauth.howTo"), url: OAUTH_DOCS[lang] }}
        />
        <div className="flex items-end gap-2">
          <div className="min-w-0 flex-1">
            <Field label={t("oauth.clientId")}>
              <TextInput
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                placeholder="00000000-0000-0000-0000-000000000000"
              />
            </Field>
          </div>
          {editingId && (
            <Button variant="ghost" onClick={() => setEditingId(false)}>
              {t("action.cancel")}
            </Button>
          )}
          <Button busy={saving} disabled={!clientId.trim()} onClick={() => void saveClientId()}>
            {t("action.save")}
          </Button>
        </div>
        {error && <ErrorLine error={error} />}
      </div>
    );
  }

  // 微软登录按钮的四色方块
  const logo = (
    <svg viewBox="0 0 21 21" width={14} height={14} aria-hidden>
      <rect x="0" y="0" width="10" height="10" fill="#f25022" />
      <rect x="11" y="0" width="10" height="10" fill="#7fba00" />
      <rect x="0" y="11" width="10" height="10" fill="#00a4ef" />
      <rect x="11" y="11" width="10" height="10" fill="#ffb900" />
    </svg>
  );

  return (
    <div className="flex flex-col gap-3">
      {phase === "idle" && (
        <Button
          variant="primary"
          className="h-9 w-full"
          disabled={!email.includes("@") || config.isPending}
          onClick={() => void start()}
        >
          {logo}
          {again ? t("oauth.signInAgain") : t("oauth.signIn")}
        </Button>
      )}
      {phase === "idle" && (
        <button
          type="button"
          onClick={() => {
            setClientId(typeof savedId === "string" ? savedId : "");
            setEditingId(true);
          }}
          className="-mt-1 self-center text-[12px] text-fg-3 hover:text-accent hover:underline"
        >
          {t("oauth.changeClient")}
        </button>
      )}
      {phase !== "idle" && (
        <div className="flex items-start gap-3 rounded-md border border-line bg-hover px-3 py-2.5 text-[12px] leading-relaxed text-fg-2">
          <span className="mt-0.5 text-fg-3">
            <Spinner size={13} />
          </span>
          <div className="min-w-0 flex-1">
            <p>{phase === "waiting" ? t("oauth.waiting") : t("oauth.finishing")}</p>
            {phase === "waiting" && url && (
              <button
                type="button"
                onClick={() => void openExternal(url)}
                className="mt-1 inline-flex items-center gap-1 text-accent hover:underline"
              >
                <ExternalLink size={12} />
                {t("oauth.reopen")}
              </button>
            )}
          </div>
          {phase === "waiting" && (
            <Button variant="ghost" className="h-6 px-2 text-[12px]" onClick={cancel}>
              {t("action.cancel")}
            </Button>
          )}
        </div>
      )}
      {error && <ErrorLine error={error} />}
    </div>
  );
}

interface Server {
  host: string;
  port: string;
  security: Security;
  username: string;
}

function ServerFields({
  value,
  onChange,
  presets,
  presetId,
  onPreset,
}: {
  value: Server;
  onChange: (next: Server) => void;
  presets?: Preset[];
  presetId?: string;
  onPreset?: (id: string) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-3 rounded-md border border-line p-3">
      {presets && onPreset && (
        <Field label={t("account.provider")}>
          <Select value={presetId ?? ""} onChange={(e) => onPreset(e.target.value)}>
            <option value="">{t("account.providerAuto")}</option>
            {presets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </Select>
        </Field>
      )}
      <div className="grid grid-cols-[1fr_88px] gap-3">
        <Field label={t("account.host")}>
          <TextInput value={value.host} onChange={(e) => onChange({ ...value, host: e.target.value })} />
        </Field>
        <Field label={t("account.port")}>
          <TextInput
            inputMode="numeric"
            value={value.port}
            onChange={(e) => onChange({ ...value, port: e.target.value.replace(/\D/g, "") })}
          />
        </Field>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field label={t("account.security")}>
          <Select
            value={value.security}
            onChange={(e) => onChange({ ...value, security: e.target.value as Security })}
          >
            <option value="tls">{t("account.securityTls")}</option>
            <option value="starttls">{t("account.securityStarttls")}</option>
            <option value="none">{t("account.securityNone")}</option>
          </Select>
        </Field>
        <Field label={t("account.username")}>
          <TextInput value={value.username} onChange={(e) => onChange({ ...value, username: e.target.value })} />
        </Field>
      </div>
    </div>
  );
}

interface Smtp {
  custom: boolean;
  host: string;
  port: string;
  security: Security;
}

const SECURITY_LABEL: Record<Security, MessageKey> = {
  tls: "account.securityTls",
  starttls: "account.securityStarttls",
  none: "account.securityNone",
};

/** 发信服务器：默认自动（服务商预设或照收信服务器猜），也可以自己填。 */
function SmtpFields({
  accountId,
  value,
  onChange,
}: {
  accountId: string;
  value: Smtp;
  onChange: (next: Smtp) => void;
}) {
  const t = useT();
  const current = useQuery({
    queryKey: ["smtp", accountId],
    queryFn: () => api.smtpSettings(accountId),
  });
  const auto = current.data && !current.data.custom ? current.data : null;
  const toggle = (custom: boolean) => {
    // 打开时先填上现在自动用的，在它的基础上改
    if (custom && !value.host && current.data) {
      onChange({
        custom,
        host: current.data.host,
        port: String(current.data.port),
        security: current.data.security,
      });
    } else {
      onChange({ ...value, custom });
    }
  };
  return (
    <div className="flex flex-col gap-3 rounded-md border border-line p-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[12px] font-medium text-fg-2">{t("account.smtp")}</p>
          {!value.custom && auto && (
            <p className="mt-0.5 select-text truncate text-[12px] text-fg-3">
              {t("account.smtpAuto", {
                server: `${auto.host}:${auto.port} · ${t(SECURITY_LABEL[auto.security])}`,
              })}
            </p>
          )}
        </div>
        <label className="flex shrink-0 items-center gap-2 text-[12px] text-fg-2">
          {t("account.smtpCustom")}
          <Switch checked={value.custom} onChange={toggle} label={t("account.smtpCustom")} />
        </label>
      </div>
      {value.custom && (
        <>
          <div className="grid grid-cols-[1fr_88px] gap-3">
            <Field label={t("account.smtpHost")}>
              <TextInput value={value.host} onChange={(e) => onChange({ ...value, host: e.target.value })} />
            </Field>
            <Field label={t("account.port")}>
              <TextInput
                inputMode="numeric"
                value={value.port}
                onChange={(e) => onChange({ ...value, port: e.target.value.replace(/\D/g, "") })}
              />
            </Field>
          </div>
          <Field label={t("account.security")}>
            <Select
              value={value.security}
              onChange={(e) => onChange({ ...value, security: e.target.value as Security })}
            >
              <option value="tls">{t("account.securityTls")}</option>
              <option value="starttls">{t("account.securityStarttls")}</option>
              <option value="none">{t("account.securityNone")}</option>
            </Select>
          </Field>
        </>
      )}
    </div>
  );
}

function Disclosure({ open, onToggle, label }: { open: boolean; onToggle: () => void; label: string }) {
  return (
    <button
      type="button"
      aria-expanded={open}
      onClick={onToggle}
      className="-ml-1 flex h-6 items-center gap-1 rounded px-1 text-[12px] font-medium text-fg-2 hover:text-fg"
    >
      <ChevronRight size={14} strokeWidth={2} className={cx("transition-transform", open && "rotate-90")} />
      {label}
    </button>
  );
}

/** 第一步里的服务商，按常用程度排。只用来自动识别的预设（比如 iCloud 自定域名）不出现在这里。 */
const PERSONAL = [
  "gmail", "outlook", "icloud", "yahoo", "qq", "163", "126", "yeah", "188", "sina",
  "sohu", "139", "aliyun", "fastmail", "zoho", "yandex", "gmx", "mailru", "aol",
];
const BUSINESS = ["exmail", "qiye_aliyun", "qiye163", "feishu", "lark", "workspace", "m365", "263"];

function ProviderTile({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-[68px] min-w-0 flex-col items-center justify-center gap-1.5 rounded-lg border border-line px-1 transition-colors hover:border-line-strong hover:bg-hover"
    >
      {children}
      <span className="w-full truncate text-center text-[12px] text-fg-2">{label}</span>
    </button>
  );
}

/** 第一步：选服务商。下面列出已经添加的邮箱。 */
function ProviderGrid({ presets, onPick }: { presets: Preset[]; onPick: (preset: Preset | null) => void }) {
  const t = useT();
  const lang = useLang();
  const nav = useNav();
  const setDialog = useUi((s) => s.setDialog);
  const byId = new Map(presets.map((p) => [p.id, p]));
  const section = (title: string, ids: string[]) => (
    <section>
      <h3 className="mb-2 text-[12px] font-medium text-fg-3">{title}</h3>
      <div className="grid grid-cols-4 gap-2">
        {ids
          .map((id) => byId.get(id))
          .filter((p): p is Preset => p !== undefined)
          .map((p) => (
            <ProviderTile key={p.id} label={providerName(p, lang)} onClick={() => onPick(p)}>
              <ProviderIcon id={p.id} size={28} />
            </ProviderTile>
          ))}
        {title === t("account.business") && (
          <ProviderTile label={t("account.other")} onClick={() => onPick(null)}>
            <span className="inline-flex size-7 items-center justify-center rounded-[7px] bg-active text-fg-2">
              <Mail size={17} strokeWidth={1.75} />
            </span>
          </ProviderTile>
        )}
      </div>
    </section>
  );
  const accounts = nav.data?.accounts ?? [];

  return (
    <div className="flex flex-col gap-5 p-4">
      {section(t("account.personal"), PERSONAL)}
      {section(t("account.business"), BUSINESS)}
      {accounts.length > 0 && (
        <section>
          <h3 className="mb-2 text-[12px] font-medium text-fg-3">
            {t("account.added")} · {accounts.length}
          </h3>
          <ul className="flex flex-col gap-px rounded-lg border border-line p-1">
            {accounts.map((a) => (
              <li key={a.id}>
                <button
                  type="button"
                  onClick={() => setDialog({ type: "edit-account", accountId: a.id })}
                  className="flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left hover:bg-hover"
                >
                  <ProviderIcon id={providerOf(a, presets)} size={16} />
                  <span className="min-w-0 truncate text-fg">{a.name || a.email}</span>
                  {a.name && <span className="min-w-0 truncate text-[12px] text-fg-3">{a.email}</span>}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

const EMPTY_SERVER: Server = { host: "", port: "993", security: "tls", username: "" };

export function AddAccountDialog() {
  const t = useT();
  const qc = useQueryClient();
  const open = useUi((s) => s.dialog?.type === "add-account");
  const setDialog = useUi((s) => s.setDialog);
  const setScope = useUi((s) => s.setScope);
  const presets = usePresets();

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [server, setServer] = useState<Server>(EMPTY_SERVER);
  const [presetId, setPresetId] = useState("");
  const [guess, setGuess] = useState<AccountGuess | null>(null);
  const [showServer, setShowServer] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Failure | null>(null);
  const [step, setStep] = useState<"choose" | "form">("choose");
  const lang = useLang();
  // 用户自己改过服务器设置之后，就不再用推断的结果覆盖
  const touched = useRef(false);

  useEffect(() => {
    if (!open) return;
    setEmail("");
    setPassword("");
    setName("");
    setServer(EMPTY_SERVER);
    setPresetId("");
    setGuess(null);
    setShowServer(false);
    setBusy(false);
    setError(null);
    setStep("choose");
    touched.current = false;
  }, [open]);

  // 第一步选的服务商。只填了名字时用它的域名补全地址
  const chosen = presetId ? presets.data?.find((p) => p.id === presetId) : undefined;
  const address = completeAddress(email, chosen);

  // 输入邮箱地址时推断服务器设置
  useEffect(() => {
    if (!open || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(address)) {
      setGuess(null);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      api
        .guessAccount(address)
        .then((g) => {
          if (cancelled) return;
          setGuess(g);
          // 地址认得出服务商时以它为准（选了 Gmail 却填了 QQ 地址，就换成 QQ）；
          // 认不出时保留在第一步选的服务商，比如用自己域名的企业邮箱
          setPresetId((chosen) => {
            if (touched.current || (!g.preset && chosen)) return chosen;
            setServer({
              host: g.imap_host,
              port: String(g.imap_port),
              security: g.imap_security,
              username: g.username,
            });
            return g.preset?.id ?? "";
          });
        })
        .catch(() => {
          if (!cancelled) setGuess(null);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [address, open]);

  const preset = useMemo(() => {
    if (presetId) return presets.data?.find((p) => p.id === presetId) ?? null;
    return guess?.preset ?? null;
  }, [guess, presetId, presets.data]);
  const hint = guess || presetId ? hintFor(preset, Boolean(presetId)) : null;

  /** 第一步选了服务商：带入它的服务器设置，进入填写地址和密码。 */
  const choose = (p: Preset | null) => {
    touched.current = false;
    setPresetId(p?.id ?? "");
    setServer(
      p
        ? { host: p.imap_host, port: String(p.imap_port), security: p.imap_security, username: "" }
        : EMPTY_SERVER,
    );
    setError(null);
    setStep("form");
  };

  const pickPreset = (id: string) => {
    setPresetId(id);
    const p = presets.data?.find((x) => x.id === id);
    if (p) {
      touched.current = true;
      setServer((s) => ({
        host: p.imap_host,
        port: String(p.imap_port),
        security: p.imap_security,
        username: s.username || address,
      }));
    } else {
      touched.current = false;
      if (guess) {
        setServer({
          host: guess.imap_host,
          port: String(guess.imap_port),
          security: guess.imap_security,
          username: guess.username,
        });
      }
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!address || !password || busy) return;
    setBusy(true);
    setError(null);
    try {
      const port = Number(server.port);
      const account = await api.addAccount({
        email: address,
        password,
        name: name.trim(),
        imap_host: server.host.trim() || undefined,
        imap_port: Number.isFinite(port) && port > 0 ? port : undefined,
        imap_security: server.host.trim() ? server.security : undefined,
        username: server.username.trim() || undefined,
      });
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: ["items"] });
      setScope({ kind: "account", accountId: account.id });
      setDialog(null);
    } catch (err) {
      setError(failure(t, err));
      // 连不上服务器时，多半是设置要改，把它展开
      if (err && typeof err === "object" && "code" in err && err.code !== "mail_auth") setShowServer(true);
      setBusy(false);
    }
  };

  /** 微软账户登录完成：用换来的授权添加邮箱。出错时抛给登录按钮去显示。 */
  const addWithOAuth = async (flow: string) => {
    setError(null);
    const port = Number(server.port);
    try {
      const account = await api.addAccount({
        email: address,
        oauth: flow,
        name: name.trim(),
        imap_host: server.host.trim() || undefined,
        imap_port: Number.isFinite(port) && port > 0 ? port : undefined,
        imap_security: server.host.trim() ? server.security : undefined,
        username: server.username.trim() || undefined,
      });
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: ["items"] });
      setScope({ kind: "account", accountId: account.id });
      setDialog(null);
    } catch (err) {
      if (err && typeof err === "object" && "code" in err && err.code !== "mail_auth") setShowServer(true);
      throw err;
    }
  };

  const placeholder = preset?.domains[0] ? `name@${preset.domains[0]}` : "name@example.com";

  return (
    <Modal
      open={open}
      onClose={() => setDialog(null)}
      title={step === "choose" ? t("account.choose") : t("account.addTitle")}
      width={step === "choose" ? 540 : 460}
    >
      {step === "choose" ? (
        <ProviderGrid presets={presets.data ?? []} onPick={choose} />
      ) : (
      <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
        <div className="-mt-1 flex items-center gap-2">
          <button
            type="button"
            onClick={() => setStep("choose")}
            disabled={busy}
            aria-label={t("account.back")}
            className="-ml-1 inline-flex size-7 items-center justify-center rounded-md text-fg-3 hover:bg-hover hover:text-fg"
          >
            <ChevronLeft size={16} strokeWidth={2} />
          </button>
          <ProviderIcon id={preset?.id} size={22} />
          <span className="font-medium">{preset ? providerName(preset, lang) : t("account.other")}</span>
        </div>
        <Field label={t("account.email")}>
          {/* 不用 type="email"：浏览器会拦下只填了名字的输入，而这里会自动补全 */}
          <TextInput
            autoFocus
            type="text"
            inputMode="email"
            autoComplete="email"
            spellCheck={false}
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder={placeholder}
            disabled={busy}
          />
        </Field>
        {address !== email.trim() && (
          <p className="-mt-2 text-[12px] text-fg-3">{t("account.willUse", { address })}</p>
        )}
        {preset && (
          <p className="-mt-2 text-[12px] text-fg-3">
            {t("account.detected", { name: providerName(preset, lang) })} · {preset.imap_host}
          </p>
        )}
        {!usesOAuth(preset) && (
          <Field label={passwordLabel(t, preset)}>
            <TextInput
              type="password"
              autoComplete="new-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              disabled={busy}
            />
          </Field>
        )}
        {hint && (
          <Hint
            text={t(hint)}
            tone="info"
            link={preset && HELP_URL[preset.id] ? { label: t("account.makePassword"), url: HELP_URL[preset.id] ?? "" } : undefined}
          />
        )}
        <Field label={t("account.name")}>
          <TextInput
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("account.namePlaceholder")}
            disabled={busy}
          />
        </Field>

        <div className="flex flex-col gap-2">
          <Disclosure open={showServer} onToggle={() => setShowServer((v) => !v)} label={t("account.server")} />
          {showServer && (
            <ServerFields
              value={server}
              onChange={(next) => {
                touched.current = true;
                setServer(next);
              }}
              presets={presets.data}
              presetId={presetId}
              onPreset={pickPreset}
            />
          )}
        </div>

        {usesOAuth(preset) ? (
          <MicrosoftSignIn email={address} onSignedIn={(flow) => addWithOAuth(flow)} />
        ) : (
          <p className="text-[12px] leading-relaxed text-fg-3">{t("account.passwordStored")}</p>
        )}
        {error && <ErrorLine error={error} />}

        <div className="flex items-center justify-end gap-2">
          {busy && (
            <span className="mr-auto flex items-center gap-2 text-[12px] text-fg-3">
              <Spinner size={12} />
              {t("account.connecting")}
            </span>
          )}
          <Button variant="ghost" onClick={() => setDialog(null)}>
            {t("action.cancel")}
          </Button>
          {!usesOAuth(preset) && (
            <Button variant="primary" type="submit" disabled={!address || !password || busy}>
              {t("action.add")}
            </Button>
          )}
        </div>
      </form>
      )}
    </Modal>
  );
}

export function EditAccountDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const nav = useNav();
  const accountId = dialog?.type === "edit-account" ? dialog.accountId : null;
  const account = nav.data?.accounts.find((a) => a.id === accountId);

  return (
    <Modal open={account !== undefined} onClose={() => setDialog(null)} title={t("account.editTitle")} width={460}>
      {account && <EditAccountForm key={account.id} account={account} onDone={() => setDialog(null)} />}
    </Modal>
  );
}

function EditAccountForm({ account, onDone }: { account: Account; onDone: () => void }) {
  const t = useT();
  const lang = useLang();
  const qc = useQueryClient();
  const setDialog = useUi((s) => s.setDialog);
  const presets = usePresets();
  const needsPassword = account.error_code === "mail_no_password";
  const connectionTrouble = Boolean(account.error_code) && !needsPassword && account.error_code !== "mail_auth";

  const [name, setName] = useState(account.name);
  const [senderName, setSenderName] = useState(account.sender_name);
  const [signature, setSignature] = useState(account.signature);
  const [password, setPassword] = useState("");
  const initial: Server = {
    host: account.imap_host,
    port: String(account.imap_port),
    security: account.imap_security,
    username: account.username,
  };
  const [server, setServer] = useState<Server>(initial);
  const [showServer, setShowServer] = useState(connectionTrouble);
  const [smtp, setSmtp] = useState<Smtp>({
    custom: Boolean(account.smtp_host),
    host: account.smtp_host ?? "",
    port: String(account.smtp_port ?? 465),
    security: account.smtp_security ?? "tls",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Failure | null>(null);

  const preset = presets.data?.find((p) => p.imap_host === account.imap_host) ?? null;
  const oauth = usesOAuth(preset);

  /** 重新用微软账户登录，顺便保存改过的名字。 */
  const signInAgain = async (flow: string) => {
    const changes: AccountChanges = { oauth: flow };
    if (name.trim() !== account.name) changes.name = name.trim();
    await api.updateAccount(account.id, changes);
    void qc.invalidateQueries({ queryKey: keys.nav });
    onDone();
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    // 只提交改过的字段，免得覆盖另一台设备上刚做的修改
    const changes: AccountChanges = {};
    if (name.trim() !== account.name) changes.name = name.trim();
    if (senderName.trim() !== account.sender_name) changes.sender_name = senderName.trim();
    if (signature.trimEnd() !== account.signature) changes.signature = signature;
    if (password) changes.password = password;
    if (server.host.trim() !== account.imap_host) changes.imap_host = server.host.trim();
    if (Number(server.port) !== account.imap_port) changes.imap_port = Number(server.port);
    if (server.security !== account.imap_security) changes.imap_security = server.security;
    if (server.username.trim() !== account.username) changes.username = server.username.trim();
    if (smtp.custom && smtp.host.trim()) {
      if (
        smtp.host.trim() !== account.smtp_host ||
        Number(smtp.port) !== account.smtp_port ||
        smtp.security !== account.smtp_security
      ) {
        changes.smtp_host = smtp.host.trim();
        changes.smtp_port = Number(smtp.port) || 465;
        changes.smtp_security = smtp.security;
      }
    } else if (account.smtp_host) {
      // 改回自动
      changes.smtp_host = "";
    }
    try {
      if (Object.keys(changes).length > 0) {
        await api.updateAccount(account.id, changes);
        void qc.invalidateQueries({ queryKey: keys.nav });
        void qc.invalidateQueries({ queryKey: ["smtp", account.id] });
      }
      onDone();
    } catch (err) {
      setError(failure(t, err));
      setBusy(false);
    }
  };

  const status = account.last_sync_at
    ? t("account.lastSync", { time: shortTime(account.last_sync_at, lang) })
    : t("account.never");

  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
      <div className="flex items-baseline justify-between gap-3">
        <span className="select-text truncate font-medium">{account.email}</span>
        <span className="shrink-0 text-[12px] text-fg-3">{status}</span>
      </div>
      {needsPassword ? (
        <Hint text={oauth ? t("oauth.needsSignIn") : t("account.needsPassword")} />
      ) : (
        account.error_code && (
          <Hint
            tone="warning"
            text={[
              errorText(t, { code: account.error_code }),
              account.error ? t("account.serverSaid", { text: account.error }) : "",
            ]
              .filter(Boolean)
              .join(" ")}
          />
        )
      )}
      <Field label={t("account.name")}>
        <TextInput
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("account.namePlaceholder")}
        />
      </Field>
      <Field label={t("account.senderName")} hint={t("account.senderNameHint")}>
        <TextInput
          value={senderName}
          onChange={(e) => setSenderName(e.target.value)}
          placeholder={t("account.senderNamePlaceholder")}
        />
      </Field>
      <Field label={t("account.signature")} hint={t("account.signatureHint")}>
        <textarea
          value={signature}
          onChange={(e) => setSignature(e.target.value)}
          rows={3}
          placeholder={t("account.signaturePlaceholder")}
          className="w-full resize-y rounded-md border border-line-strong bg-bg px-2.5 py-1.5 text-[13px] leading-relaxed text-fg outline-none transition-colors placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent"
        />
      </Field>
      {oauth ? (
        <MicrosoftSignIn email={account.email} again={!needsPassword} onSignedIn={signInAgain} />
      ) : (
        <Field label={passwordLabel(t, preset)}>
          <TextInput
            autoFocus={needsPassword || account.error_code === "mail_auth"}
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={needsPassword ? "" : t("account.passwordKeep")}
          />
        </Field>
      )}
      <div className="flex flex-col gap-2">
        <Disclosure open={showServer} onToggle={() => setShowServer((v) => !v)} label={t("account.server")} />
        {showServer && (
          <>
            <ServerFields value={server} onChange={setServer} />
            <SmtpFields accountId={account.id} value={smtp} onChange={setSmtp} />
          </>
        )}
      </div>
      {error && <ErrorLine error={error} />}
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          className="mr-auto text-danger hover:text-danger"
          onClick={() => setDialog({ type: "remove-account", accountId: account.id })}
        >
          {t("action.removeAccount")}
        </Button>
        {busy && <Spinner size={12} />}
        <Button variant="ghost" onClick={onDone}>
          {t("action.cancel")}
        </Button>
        <Button variant="primary" type="submit" busy={busy} disabled={needsPassword && (oauth || !password)}>
          {t("action.save")}
        </Button>
      </div>
    </form>
  );
}

export function RemoveAccountDialog() {
  const t = useT();
  const qc = useQueryClient();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const scope = useUi((s) => s.scope);
  const setScope = useUi((s) => s.setScope);
  const nav = useNav();
  const accountId = dialog?.type === "remove-account" ? dialog.accountId : null;
  const account = nav.data?.accounts.find((a) => a.id === accountId);
  const [busy, setBusy] = useState(false);

  useEffect(() => setBusy(false), [accountId]);

  const remove = async () => {
    if (!account) return;
    setBusy(true);
    try {
      await api.removeAccount(account.id);
      if (scope.kind === "account" && scope.accountId === account.id) {
        setScope({ kind: "view", view: "inbox" });
      }
      void qc.invalidateQueries({ queryKey: keys.nav });
      void qc.invalidateQueries({ queryKey: ["items"] });
      setDialog(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal open={account !== undefined} onClose={() => setDialog(null)} title={t("account.removeTitle")} width={420}>
      {account && (
        <div className="flex flex-col gap-4 p-4">
          <p className="leading-relaxed text-fg-2">{t("account.removeBody", { email: account.email })}</p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setDialog(null)}>
              {t("action.cancel")}
            </Button>
            <Button variant="danger" busy={busy} onClick={() => void remove()}>
              {t("action.remove")}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
