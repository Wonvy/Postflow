# Read aloud

[中文](read-aloud.zh-CN.md)

Click **Read aloud** next to the date under an article's title. Postflow reads what the reader is showing (the full
text when that's open, the translation when that's on), marks the sentence it's on, and keeps going when you open
another article, with small controls under the list. Pause, skip sentences and change the speed from the bar that
replaces the button. When it finishes it moves on to the next article in the list; see
[Listening to the news](#listening-to-the-news).

## Listening to the news

Click the **headphones** above the list (Listen to this list) and Postflow reads one article after another, starting
from the article that's open, or the first one. **Read aloud** under a title does the same from that article. Each
article starts with its source and headline. If the reader is showing the article that just finished, it opens the
next one so you can follow along; otherwise reading continues in the background, with controls under the list.

Choose in **Settings → Read aloud → Listening**, or from the options button on the playback bar:

- **Read for each article:** the full text, the feed's **summary** (the first few sentences when the feed has none),
  an **AI summary**, or **headlines only**.
- **Continue with the next article** (on by default): follows the list as it was when reading started. Podcasts and
  videos are skipped.
- **Mark as read when finished** (on by default). Articles you skip aren't marked.
- **Sleep timer:** stop after 15, 30, 45 or 60 minutes, or after the current article.

The playback bar has a **next article** button, and the play/pause and next keys on your keyboard or headphones work
while a local voice is reading.

### AI summaries and cost

The AI summary is the same one the AI panel shows, saved on this device, so an article that already has one costs
nothing. Postflow only asks AI to write one for the article being read and the next one (prepared in advance so there
is no wait between articles), never for the whole list. The options menu shows how many summaries AI wrote during the
current session. If AI isn't available (no service set up, out of credit, no network), Postflow says why once and reads the
feed's summaries for the rest of the session instead of retrying. Mail is only sent to a cloud AI service if you
allowed that in Settings → AI; otherwise that message's own summary is read.

## Voices

Choose the voice in **Settings → Read aloud**:

- **Local voices** are generated on your own computer by open-source [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M)
  models, run by [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), the same engine as podcast subtitles. Nothing
  is sent online. They sound much more natural than most system voices and work the same on macOS, Windows and Linux.
- **System voices** are whatever your operating system offers. Nothing to download; quality depends on the system.
  Most Linux desktops have none.

**Automatic** (the default) uses a local voice when one is downloaded, and the system voice otherwise. For each
language there is a voice menu with a preview button; choosing a voice plays a sample.

Local voices read Chinese and English. Articles in other languages (Japanese, Korean…) are read with the system voice.

## Voice models

Models aren't bundled with the app. Download one in **Settings → Read aloud**:

| Model | Size | Voices | Best for |
| --- | --- | --- | --- |
| Kokoro v1.1-zh | 370 MB | 55 female and 45 male Mandarin voices, 3 English | Chinese |
| Kokoro v1.0 | 345 MB | 28 American and British English voices, 8 Mandarin | English |
| ZipVoice | 183 MB | The voice of a recording you add (see below) | Sounding like a real person |

With both installed, Automatic reads Chinese with v1.1-zh and English with v1.0. Each file is checked against its
SHA-256 after download; a copy on ModelScope is used where Hugging Face is slow or blocked. If neither downloads,
fetch the files yourself and use **Import from folder**. The list comes from
[`public/catalog/voices.json`](../public/catalog/voices.json), which the app checks once a day, so new models can be
added without a new release.

The default Chinese voice (female 94) was chosen because speech recognition understood it best across a few news
passages, out of all 100 Chinese voices. That measures clarity, not taste: try a few in the voice menu.

## Your own voice (ZipVoice)

[ZipVoice](https://github.com/k2-fsa/ZipVoice) doesn't come with voices. It reads in the voice of a short recording
you add under **Voice samples**, copying its timbre and tone, which makes it sound the most like a real person.

1. Download ZipVoice in Settings → Read aloud.
2. Click **Add a recording…** and choose an audio file (MP3, M4A, WAV, Ogg, FLAC…). Use 3–10 seconds of one person
   speaking clearly, with no music or background noise. Postflow keeps the first 12 seconds and trims silence at the
   ends.
3. Check the text under the recording: it must match what is said, word for word. If a speech recognition model is
   installed (Settings → Subtitles), it's filled in for you; otherwise type it.
4. Pick the recording for Chinese and/or English at the top of the page. A recording in the same language works best:
   with a Chinese recording, English comes out with an accent and some words get lost.

Only use your own voice, or a recording the speaker agreed to. Recordings stay on this computer and aren't synced.

ZipVoice is slower than Kokoro (about 1.5× real time on an Apple M4, against 4× for Kokoro), so on older computers it
may not keep up and there will be pauses between sentences. Its dictionary has no English words, so Postflow also
downloads Kokoro's English dictionary and converts it to standard IPA the first time ZipVoice loads. In this version of
sherpa-onnx ZipVoice ignores the Chinese number rules, so Postflow writes numbers in Chinese sentences as characters
itself. Very short sentences can't be generated fast; those are generated at normal speed and played faster.

## How it works

The reader splits the text into sentences. Each sentence is sent to the core, which generates it and returns the
audio. While one sentence plays, the next two are being prepared, so there is no pause between sentences. On an Apple
M4 a sentence is generated about four times faster than it takes to say; the first one takes a second or two longer
because the model is loaded then. Changing the speed regenerates from the current sentence.

The model needs about 700 MB of memory while it's reading. It's released after ten minutes without use.

When you use Postflow in a browser on another device (Settings → Web access), the computer running Postflow does the
reading and the browser plays it, so a phone gets the same voice.

## Before reading

Kokoro reads words from a dictionary and skips words it doesn't know, which in news means most abbreviations and brand
names. Postflow tidies each sentence first:

- Links aren't read.
- Abbreviations are spelled out (`GPU` → G P U), joined-up names are split (`OpenAI` → Open AI, `MacBook` → Mac Book),
  and other unknown words are pieced together from dictionary words where possible (`Tencent` → ten cent).
- In English sentences, numbers, years, ordinals, percentages, money, times and dates become words
  ("$3.2 billion in 2026" → "three point two billion dollars in twenty twenty-six").
- In Chinese sentences, `5.2%`, `1,234` and `10:30` are rewritten so the model's own Chinese number rules read them
  correctly.

Names it can't piece together (many Chinese names written in pinyin, such as Huawei) are still skipped.

## Trying it from the command line

```sh
cargo run --release -p postflow-speech --example speak -- models .postflow-dev/local/voices
cargo run --release -p postflow-speech --example speak -- download .postflow-dev/local/voices kokoro-v1.1-zh
cargo run --release -p postflow-speech --example speak -- prepare .postflow-dev/local/voices kokoro-v1.1-zh "OpenAI 发布了 GPT-5"
cargo run --release -p postflow-speech --example speak -- say .postflow-dev/local/voices kokoro-v1.1-zh zf_094 "你好" out.wav
```

`prepare` prints the sentence as it will be read, which helps when a word comes out wrong.
