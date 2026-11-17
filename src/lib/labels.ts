// 分类：设置里定义的类别，以及每个类别的颜色。

import type { Category } from "./api";
import { useSettings } from "./queries";
import type { Lang } from "./time";

const DEFAULTS: Record<Lang, Category[]> = {
  zh: [
    { name: "工作", description: "工作相关的邮件、讨论和安排" },
    { name: "个人", description: "亲友之间的往来" },
    { name: "通知", description: "系统通知、提醒、验证码、构建结果" },
    { name: "账单", description: "账单、收据、订单和付款" },
    { name: "资讯", description: "新闻、博客文章、周刊" },
    { name: "推广", description: "广告、营销和促销" },
  ],
  en: [
    { name: "Work", description: "Work email, discussions and plans" },
    { name: "Personal", description: "Friends and family" },
    { name: "Notifications", description: "System notices, reminders, codes, build results" },
    { name: "Bills", description: "Bills, receipts, orders and payments" },
    { name: "Reading", description: "News, blog posts, newsletters" },
    { name: "Promotions", description: "Ads, marketing and deals" },
  ],
};

export function defaultCategories(lang: Lang): Category[] {
  return DEFAULTS[lang].map((c) => ({ ...c }));
}

/** 设置里的类别。还没设置过时为空。 */
export function useCategories(): Category[] | null {
  const settings = useSettings();
  const value = settings.data?.["ai.categories"];
  if (!Array.isArray(value)) return null;
  return value.filter(
    (c): c is Category => !!c && typeof c === "object" && typeof (c as Category).name === "string" && (c as Category).name.trim() !== "",
  );
}

/** 彼此区分得开的几种颜色，深浅主题下都看得清。 */
const PALETTE = ["#3b82f6", "#10b981", "#f59e0b", "#8b5cf6", "#ef4444", "#06b6d4", "#ec4899", "#84cc16"];

/** 不在设置列表里的分类（比如改名前留下的），按名字挑一个颜色。 */
function hashColor(name: string): string {
  let hash = 0;
  for (const c of name) hash = (hash * 31 + (c.codePointAt(0) ?? 0)) >>> 0;
  return PALETTE[hash % PALETTE.length] ?? "#888";
}

/** 分类的颜色：按它在设置里的位置依次取色，相邻的类别颜色一定不同。 */
export function useLabelColor(): (name: string) => string {
  const categories = useCategories();
  return (name: string) => {
    const i = categories?.findIndex((c) => c.name === name) ?? -1;
    return i >= 0 ? (PALETTE[i % PALETTE.length] ?? "#888") : hashColor(name);
  };
}
