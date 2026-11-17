// 条目来源的图标：文章是订阅源的网站图标，邮件是收到它的邮箱（服务商图标，认不出时用名字的首字）。

import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, type Item } from "../lib/api";
import { useNav, usePresets } from "../lib/queries";
import { logoDomain } from "../lib/senderLogo";
import { useUi } from "../lib/store";
import { hasProviderIcon, ProviderIcon, providerOf } from "./ProviderIcon";
import { Avatar, cx, Favicon } from "./ui";

type Source = Pick<Item, "kind" | "source_id" | "source_icon" | "source_title">;

/** 邮箱账户的图标，和侧栏里的一样。 */
export function AccountIcon({ accountId, fallback, size = 16 }: { accountId: string; fallback: string; size?: number }) {
  const nav = useNav();
  const presets = usePresets();
  const account = nav.data?.accounts.find((a) => a.id === accountId);
  const provider = account ? providerOf(account, presets.data) : null;
  if (hasProviderIcon(provider)) return <ProviderIcon id={provider} size={size} />;
  return <Avatar name={account?.name || account?.email || fallback || "?"} seed={account?.email} size={size} />;
}

export function SourceIcon({ item, size = 16 }: { item: Source; size?: number }) {
  if (item.kind === "mail") return <AccountIcon accountId={item.source_id} fallback={item.source_title} size={size} />;
  return <Favicon src={item.source_icon} size={size} />;
}

/**
 * 发件人头像：公司发来的信用发件人网站的图标，个人邮箱、找不到图标、图标加载失败时显示首字母。
 * 图标加载出来之前先显示首字母，免得一片空白。
 */
export function SenderAvatar({
  name,
  address,
  size = 16,
}: {
  name: string;
  address: string | null | undefined;
  size?: number;
}) {
  const enabled = useUi((s) => s.senderLogos);
  const domain = enabled ? logoDomain(address) : null;
  const icon = useQuery({
    queryKey: ["senderIcon", domain],
    queryFn: () => api.senderIcon(domain ?? ""),
    enabled: domain !== null,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 60 * 60 * 1000,
    retry: false,
  });
  const src = icon.data?.url ?? null;
  const [state, setState] = useState<"loading" | "ok" | "failed">("loading");
  useEffect(() => setState("loading"), [src]);

  const letter = <Avatar name={name} seed={address} size={size} />;
  if (!src || state === "failed") return letter;
  return (
    <span className="relative inline-flex shrink-0" style={{ width: size, height: size }}>
      {state === "loading" && letter}
      <img
        src={src}
        alt=""
        width={size}
        height={size}
        loading="lazy"
        decoding="async"
        referrerPolicy="no-referrer"
        draggable={false}
        onLoad={(e) => setState(e.currentTarget.naturalWidth > 1 ? "ok" : "failed")}
        onError={() => setState("failed")}
        className={cx(
          "rounded-full bg-white object-cover ring-1 ring-line",
          state === "loading" ? "absolute inset-0 opacity-0" : "",
        )}
        style={{ width: size, height: size }}
      />
    </span>
  );
}

/** 有不止一个邮箱、又不是只看一个邮箱时，邮件要看得出是哪个邮箱收到的。 */
export function useMixedAccounts(): boolean {
  const nav = useNav();
  return (nav.data?.accounts.length ?? 0) > 1;
}

/**
 * 邮件发件人的头像，右下角带一个小小的邮箱图标，混在一起的列表里能分清是哪个邮箱的信。
 * 文章直接是订阅源的图标。
 */
export function SenderIcon({ item, size = 16, badge }: { item: Item; size?: number; badge: boolean }) {
  if (item.kind !== "mail") return <Favicon src={item.source_icon} size={size} />;
  const avatar = <SenderAvatar name={item.author || item.sender_address || "?"} address={item.sender_address} size={size} />;
  if (!badge) return avatar;
  const mark = Math.max(8, Math.round(size * 0.56));
  return (
    <span className="relative inline-flex shrink-0" style={{ width: size, height: size }}>
      {avatar}
      <span
        className={cx("absolute flex items-center justify-center rounded-[3px] bg-bg ring-1 ring-bg")}
        style={{ width: mark, height: mark, right: -Math.round(mark / 3), bottom: -Math.round(mark / 3) }}
      >
        <AccountIcon accountId={item.source_id} fallback={item.source_title} size={mark} />
      </span>
    </span>
  );
}
