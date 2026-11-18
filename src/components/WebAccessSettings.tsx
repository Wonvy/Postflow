// 设置 → 网页访问：桌面应用在本机端口上提供同一套界面，浏览器、同一网络里的手机都能打开。
// 服务和设置在桌面端（src-tauri/src/web_access.rs），这里只是开关和显示链接。只有桌面应用里有这一页。

import * as Popover from "@radix-ui/react-popover";
import { Copy, ExternalLink, RefreshCw, Smartphone, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { renderSVG } from "uqr";
import { useT } from "../lib/i18n";
import { copyText, openExternal } from "../lib/platform";
import { useUpdateWebAccess, useWebAccess, type WebAccessUpdate } from "../lib/webAccess";
import { SettingRow } from "./SettingsParts";
import { toast } from "./Toast";
import { useUi } from "../lib/store";
import { Button, IconButton, Select, Spinner, Switch, TextInput, useHoverOpen } from "./ui";

/** 一个带访问密钥的地址：显示时把密钥藏起来，复制和打开时用完整的。 */
function UrlRow({ url }: { url: string }) {
  const t = useT();
  const shown = url.replace(/key=[^&]+/, "key=••••••");
  return (
    <div className="flex items-center gap-1 rounded-md border border-line bg-bg pl-2.5">
      <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-fg-2" title={shown}>
        {shown}
      </span>
      <IconButton
        label={t("webAccess.copy")}
        onClick={() => void copyText(url).then(() => toast(t("webAccess.copied")))}
      >
        <Copy size={14} strokeWidth={1.75} />
      </IconButton>
      <IconButton label={t("webAccess.open")} onClick={() => void openExternal(url)}>
        <ExternalLink size={14} strokeWidth={1.75} />
      </IconButton>
    </div>
  );
}

export function WebAccessSettings() {
  const t = useT();
  const query = useWebAccess();
  const save = useUpdateWebAccess();
  const status = query.data ?? null;
  const [port, setPort] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    if (status) setPort(String(status.port));
  }, [status?.port]);

  const update = async (change: WebAccessUpdate) => {
    setBusy(true);
    setProblem(null);
    try {
      const s = await save(change);
      setPort(String(s.port));
    } catch (err) {
      setProblem(String(err));
    } finally {
      setBusy(false);
    }
  };

  if (!status) {
    return (
      <div className="flex justify-center py-8 text-fg-3">
        <Spinner size={16} />
      </div>
    );
  }

  const portNumber = Number(port);
  const portValid = Number.isInteger(portNumber) && portNumber >= 1024 && portNumber <= 65535;

  return (
    <>
      <SettingRow label={t("webAccess.enable")} hint={t("webAccess.enableHint")}>
        {busy && <Spinner size={13} />}
        <Switch
          label={t("webAccess.enable")}
          checked={status.enabled}
          disabled={busy}
          onChange={(on) => void update({ enabled: on })}
        />
      </SettingRow>

      {status.enabled && (
        <>
          <SettingRow label={t("webAccess.port")} hint={t("webAccess.portHint")}>
            <TextInput
              className="w-24"
              inputMode="numeric"
              value={port}
              aria-label={t("webAccess.port")}
              onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))}
              onKeyDown={(e) => {
                if (e.key === "Enter" && portValid && portNumber !== status.port) void update({ port: portNumber });
              }}
            />
            <Button
              disabled={busy || !portValid || portNumber === status.port}
              onClick={() => void update({ port: portNumber })}
            >
              {t("action.save")}
            </Button>
          </SettingRow>

          {(status.error || problem) && (
            <p role="alert" className="flex items-start gap-2 text-[12px] leading-relaxed text-danger">
              <TriangleAlert size={14} strokeWidth={2} className="mt-0.5 shrink-0" />
              {t("webAccess.failed", { port: status.port, error: status.error ?? problem ?? "" })}
            </p>
          )}

          {status.local_url && (
            <div>
              <p className="mb-1.5 text-[12px] font-medium text-fg-2">{t("webAccess.thisComputer")}</p>
              <UrlRow url={status.local_url} />
            </div>
          )}

          <SettingRow label={t("webAccess.lan")} hint={t("webAccess.lanHint")}>
            <Switch
              label={t("webAccess.lan")}
              checked={status.lan}
              disabled={busy}
              onChange={(on) => void update({ lan: on })}
            />
          </SettingRow>

          {status.lan && status.running && status.lan_addresses.length > 1 && (
            <SettingRow label={t("webAccess.network")} hint={t("webAccess.networkHint")}>
              <Select
                className="w-56"
                aria-label={t("webAccess.network")}
                value={status.lan_interface ?? ""}
                disabled={busy}
                onChange={(e) => void update({ lan_interface: e.target.value })}
              >
                {status.lan_addresses.map((a) => (
                  <option key={`${a.name}|${a.ip}`} value={a.name}>
                    {a.name} · {a.ip}
                  </option>
                ))}
              </Select>
            </SettingRow>
          )}

          {status.lan && status.running && (
            status.lan_url ? (
              <div className="flex items-start gap-4">
                <div
                  className="size-[132px] shrink-0 rounded-md bg-white p-2 [&>svg]:size-full"
                  role="img"
                  aria-label={t("webAccess.qr")}
                  // 二维码是本地生成的 SVG，内容只是上面那个地址
                  // biome-ignore lint/security/noDangerouslySetInnerHtml: 本地生成的二维码
                  dangerouslySetInnerHTML={{ __html: renderSVG(status.lan_url, { border: 1 }) }}
                />
                <div className="min-w-0 flex-1">
                  <p className="mb-1.5 text-[12px] font-medium text-fg-2">{t("webAccess.otherDevices")}</p>
                  <UrlRow url={status.lan_url} />
                  <p className="mt-2 text-[12px] leading-relaxed text-fg-3">{t("webAccess.qrHint")}</p>
                  <p className="mt-1.5 text-[12px] leading-relaxed text-fg-3">{t("webAccess.httpNote")}</p>
                </div>
              </div>
            ) : (
              <p className="text-[12px] text-fg-3">{t("webAccess.noLanAddress")}</p>
            )
          )}

          <SettingRow label={t("webAccess.newKey")} hint={t("webAccess.newKeyHint")}>
            <Button disabled={busy} onClick={() => void update({ new_key: true })}>
              <RefreshCw size={13} strokeWidth={2} />
              {t("webAccess.newKeyButton")}
            </Button>
          </SettingRow>
        </>
      )}
    </>
  );
}

/**
 * 侧栏底部最右边的手机图标：鼠标移上去显示扫码打开的二维码，点一下打开「设置 → 网页访问」。
 * 局域网访问没开时，悬停说明怎么打开。只有桌面应用里有。
 */
export function PhoneAccessButton() {
  const t = useT();
  const setDialog = useUi((s) => s.setDialog);
  const query = useWebAccess();
  const status = query.data;
  const { open, setOpen, hover } = useHoverOpen();
  const ready = !!status?.lan_url;
  const chosen = status?.lan_addresses.find((a) => a.name === status.lan_interface);
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild {...hover}>
        <button
          type="button"
          aria-label={t("webAccess.phone")}
          onClick={(e) => {
            e.preventDefault();
            setOpen(false);
            setDialog({ type: "settings", page: "web" });
          }}
          className="relative inline-flex size-7 shrink-0 items-center justify-center rounded-md text-fg-3 transition-colors hover:bg-hover hover:text-fg touch:size-10"
        >
          <Smartphone size={16} strokeWidth={1.75} />
          {/* 局域网链接准备好了：右上角一个小点，图标本身不抢眼 */}
          {ready && <span aria-hidden className="absolute right-1 top-1 size-1.5 rounded-full bg-accent" />}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="end"
          sideOffset={6}
          {...hover}
          onOpenAutoFocus={(e) => e.preventDefault()}
          className="anim-pop z-50 w-[228px] rounded-lg bg-surface p-3 text-center shadow-pop outline-none"
        >
          {ready && status?.lan_url ? (
            <>
              <div
                className="mx-auto size-[180px] rounded-md bg-white p-2 [&>svg]:size-full"
                role="img"
                aria-label={t("webAccess.qr")}
                // biome-ignore lint/security/noDangerouslySetInnerHtml: 本地生成的二维码
                dangerouslySetInnerHTML={{ __html: renderSVG(status.lan_url, { border: 1 }) }}
              />
              <p className="mt-2 text-[12px] font-medium text-fg">{t("webAccess.phoneScan")}</p>
              {chosen && (
                <p className="mt-0.5 truncate text-[11px] text-fg-3">
                  {chosen.name} · {chosen.ip}
                </p>
              )}
            </>
          ) : (
            <p className="text-[12px] leading-relaxed text-fg-2">
              {status?.enabled && status.running ? t("webAccess.phoneNeedsLan") : t("webAccess.phoneOff")}
            </p>
          )}
          <p className="mt-2 text-[11px] text-fg-3">{t("webAccess.phoneClick")}</p>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
