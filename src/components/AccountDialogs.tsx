import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronRight, Info, TriangleAlert } from "lucide-react";
import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";
import {
  type Account,
  type AccountChanges,
  type AccountGuess,
  api,
  type Preset,
  type Security,
} from "../lib/api";
import { errorDetail, errorText, type MessageKey, type Translate, useLang, useT } from "../lib/i18n";
import { keys, useNav } from "../lib/queries";
import { useUi } from "../lib/store";
import { shortTime } from "../lib/time";
import { Button, cx, Field, Modal, Select, Spinner, TextInput } from "./ui";

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

function Hint({ text, tone = "info" }: { text: string; tone?: "info" | "warning" }) {
  const Icon = tone === "warning" ? TriangleAlert : Info;
  return (
    <p className="flex items-start gap-2 rounded-md border border-line bg-hover px-3 py-2 text-[12px] leading-relaxed text-fg-2">
      <Icon
        size={14}
        strokeWidth={2}
        className={cx("mt-px shrink-0", tone === "warning" ? "text-warning" : "text-fg-3")}
      />
      <span className="select-text">{text}</span>
    </p>
  );
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

function usePresets() {
  return useQuery({ queryKey: ["presets"], queryFn: () => api.presets().then((r) => r.presets) });
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
    touched.current = false;
  }, [open]);

  // 输入邮箱地址时推断服务器设置
  useEffect(() => {
    const address = email.trim();
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
          if (!touched.current) {
            setServer({
              host: g.imap_host,
              port: String(g.imap_port),
              security: g.imap_security,
              username: g.username,
            });
          }
        })
        .catch(() => {
          if (!cancelled) setGuess(null);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [email, open]);

  const preset = useMemo(() => {
    if (presetId) return presets.data?.find((p) => p.id === presetId) ?? null;
    return guess?.preset ?? null;
  }, [guess, presetId, presets.data]);
  const hint = guess || presetId ? hintFor(preset, Boolean(presetId)) : null;

  const pickPreset = (id: string) => {
    setPresetId(id);
    const p = presets.data?.find((x) => x.id === id);
    if (p) {
      touched.current = true;
      setServer((s) => ({
        host: p.imap_host,
        port: String(p.imap_port),
        security: p.imap_security,
        username: s.username || email.trim(),
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
    if (!email.trim() || !password || busy) return;
    setBusy(true);
    setError(null);
    try {
      const port = Number(server.port);
      const account = await api.addAccount({
        email: email.trim(),
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

  return (
    <Modal open={open} onClose={() => setDialog(null)} title={t("account.addTitle")} width={460}>
      <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-4 p-4">
        <Field label={t("account.email")}>
          <TextInput
            autoFocus
            type="email"
            inputMode="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="name@example.com"
            disabled={busy}
          />
        </Field>
        {preset && (
          <p className="-mt-2 text-[12px] text-fg-3">
            {t("account.detected", { name: preset.name })} · {preset.imap_host}
          </p>
        )}
        <Field label={passwordLabel(t, preset)}>
          <TextInput
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            disabled={busy}
          />
        </Field>
        {hint && <Hint text={t(hint)} tone={preset?.password_kind === "oauth" ? "warning" : "info"} />}
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

        <p className="text-[12px] leading-relaxed text-fg-3">{t("account.passwordStored")}</p>
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
          <Button variant="primary" type="submit" disabled={!email.trim() || !password || busy}>
            {t("action.add")}
          </Button>
        </div>
      </form>
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
  const [password, setPassword] = useState("");
  const initial: Server = {
    host: account.imap_host,
    port: String(account.imap_port),
    security: account.imap_security,
    username: account.username,
  };
  const [server, setServer] = useState<Server>(initial);
  const [showServer, setShowServer] = useState(connectionTrouble);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Failure | null>(null);

  const preset = presets.data?.find((p) => p.imap_host === account.imap_host) ?? null;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    // 只提交改过的字段，免得覆盖另一台设备上刚做的修改
    const changes: AccountChanges = {};
    if (name.trim() !== account.name) changes.name = name.trim();
    if (password) changes.password = password;
    if (server.host.trim() !== account.imap_host) changes.imap_host = server.host.trim();
    if (Number(server.port) !== account.imap_port) changes.imap_port = Number(server.port);
    if (server.security !== account.imap_security) changes.imap_security = server.security;
    if (server.username.trim() !== account.username) changes.username = server.username.trim();
    try {
      if (Object.keys(changes).length > 0) {
        await api.updateAccount(account.id, changes);
        void qc.invalidateQueries({ queryKey: keys.nav });
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
        <Hint text={t("account.needsPassword")} />
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
      <div className="flex flex-col gap-2">
        <Disclosure open={showServer} onToggle={() => setShowServer((v) => !v)} label={t("account.server")} />
        {showServer && <ServerFields value={server} onChange={setServer} />}
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
        <Button variant="primary" type="submit" busy={busy} disabled={needsPassword && !password}>
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
