// 发件人头像用哪个网站的图标：从发件人地址取出域名，去掉子域名，个人邮箱服务不算。
// 这里只有纯函数，方便测试：npm run test:ui

/** 个人邮箱服务：发件人是某个人，不是这家公司，显示公司图标会误导。 */
const PERSONAL = new Set([
  "gmail.com",
  "googlemail.com",
  "outlook.com",
  "hotmail.com",
  "live.com",
  "msn.com",
  "yahoo.com",
  "ymail.com",
  "icloud.com",
  "me.com",
  "mac.com",
  "aol.com",
  "proton.me",
  "protonmail.com",
  "pm.me",
  "gmx.com",
  "gmx.de",
  "gmx.net",
  "web.de",
  "mail.ru",
  "yandex.ru",
  "yandex.com",
  "zoho.com",
  "fastmail.com",
  "hey.com",
  "tutanota.com",
  "tuta.io",
  "qq.com",
  "foxmail.com",
  "163.com",
  "126.com",
  "yeah.net",
  "sina.com",
  "sina.cn",
  "sohu.com",
  "aliyun.com",
  "139.com",
  "189.cn",
  "naver.com",
  "daum.net",
  "hanmail.net",
]);

/** 国家域名下常见的二级域名，例如 example.co.uk、example.com.cn 要保留三段。 */
const SECOND_LEVEL = new Set(["co", "com", "net", "org", "gov", "edu", "ac", "ne", "or", "go"]);

/** 去掉子域名：mail.notion.so → notion.so，em.example.co.uk → example.co.uk。 */
export function siteDomain(host: string): string {
  const labels = host.toLowerCase().replace(/\.$/, "").split(".");
  if (labels.length <= 2) return labels.join(".");
  const tld = labels[labels.length - 1] ?? "";
  const second = labels[labels.length - 2] ?? "";
  const keep = tld.length === 2 && SECOND_LEVEL.has(second) ? 3 : 2;
  return labels.slice(-keep).join(".");
}

/** 发件人地址对应的网站域名；个人邮箱、格式不对的地址返回 null。 */
export function logoDomain(address: string | null | undefined): string | null {
  const at = address?.trim().lastIndexOf("@") ?? -1;
  if (!address || at < 0) return null;
  const host = address.trim().slice(at + 1).toLowerCase();
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(host) || /^[0-9.]+$/.test(host)) return null;
  const domain = siteDomain(host);
  if (PERSONAL.has(host) || PERSONAL.has(domain) || domain.startsWith("yahoo.") || domain.startsWith("outlook.")) {
    return null;
  }
  return domain;
}
