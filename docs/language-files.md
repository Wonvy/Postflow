# Language files

Postflow's interface comes in Chinese and English. You can add another language yourself: export the
interface text, translate it, and import the file back. [中文说明](#中文说明) is below.

## Steps

1. Open **Settings → Appearance → Language files → Export**, and pick the language you want to translate
   from (Chinese or English).
2. Edit the file (see the format below). At minimum, change `code` and `name`.
3. **Import…** the file. Postflow switches to it right away, and it shows up in the **Language** menu.

To update a translation, import the new file again. A file with the same `code` replaces the old one.
To pick up texts added in a newer version of Postflow, export your language again: untranslated entries
come out in the original language, so you can see what's left.

Imported languages are stored on this device only. They aren't synced, so import the file on each device.

## Format

```json
{
  "postflow_language": 1,
  "code": "ja",
  "name": "日本語",
  "base": "en",
  "app_version": "0.1.13",
  "messages": {
    "nav.inbox": "受信トレイ",
    "group.count": "{n} 件",
    "update.available": "Postflow {version} が公開されました"
  }
}
```

| Field | Meaning |
| --- | --- |
| `postflow_language` | Format version. Leave it at `1`. |
| `code` | Language code, such as `ja`, `fr` or `zh-TW`. `zh` and `en` are taken by the built-in languages. |
| `name` | Name shown in the Language menu, written in the language itself. Up to 40 characters. |
| `base` | `zh` or `en`. Used for any text you haven't translated, and for how dates and numbers are written. |
| `app_version` | The Postflow version the file was exported from. For your reference only. |
| `messages` | The texts. Translate the values; never change the keys. |

## Rules

- **Placeholders** in braces, like `{n}`, `{name}` or `{version}`, are filled in by the app. Keep every
  one of them, spelled exactly the same. They can move anywhere in the sentence. If a translation is
  missing a placeholder or adds a new one, it's skipped and the original is shown instead.
- **Missing entries** are fine. They show in the `base` language.
- **Unknown keys** (for example, from an older version) are ignored.
- Don't translate the brand names Postflow, Gmail, Outlook and so on unless your language usually does.
- The import tells you how many texts were used and which ones were skipped.

## 中文说明

Postflow 的界面自带中文和英文。你可以自己加一种语言：导出界面文字，翻译后再导入。

1. 打开 **设置 → 外观 → 语言文件 → 导出**，选择从哪种语言翻译（中文或英文）。
2. 编辑文件（格式见上面的例子）。至少要改 `code` 和 `name`。
3. 点 **导入…** 选这个文件。界面会马上换成这种语言，**语言** 菜单里也会出现它。

更新翻译时再导入一次新文件，`code` 相同的会替换旧的。Postflow 新版本加了文字后，把你的语言再导出一次：
没翻译的条目会以原文出现，一眼能看出还差哪些。

导入的语言只存在这台设备上，不同步，每台设备都要导入一次。

字段：

- `postflow_language`：格式版本，保持 `1`。
- `code`：语言代码，如 `ja`、`fr`、`zh-TW`。`zh` 和 `en` 是内置语言，不能用。
- `name`：语言菜单里显示的名字，用这种语言本身来写，最多 40 个字。
- `base`：`zh` 或 `en`。没翻译的条目用它，日期和数字的写法也跟着它。
- `app_version`：导出时的 Postflow 版本，只是给你参考。
- `messages`：界面文字。只翻译值，不要改键。

规则：

- 花括号里的**占位符**（`{n}`、`{name}`、`{version}` 等）由应用填进去，每个都要保留、拼写不变，可以放在句子任何位置。
  少了或多了占位符的那一条会被跳过，显示原文。
- **缺的条目**没关系，会用 `base` 指定的语言显示。
- 界面里**没有的键**（比如旧版本留下的）会被忽略。
- 导入后会告诉你用上了多少条、跳过了哪些。
