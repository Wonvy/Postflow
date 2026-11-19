# Subtitles for podcasts

[中文](subtitles.zh-CN.md)

Postflow can show what is being said while a podcast plays. Turn on the **CC** button in the player (in the reader, or
in the small player under the list); a line of text follows the audio.

- If the show publishes a transcript with timestamps (Podcasting 2.0), Postflow uses it.
- Otherwise Postflow transcribes the episode **on your own computer**. The audio is not uploaded anywhere.

## How it works

Postflow doesn't listen to the sound coming out of your speakers. It downloads the episode to this device (the same
download as "Download for offline listening") and reads the file from where you're listening onward, starting as soon
as the first part has arrived rather than waiting for the whole download. Recognition is several times to tens of
times faster than playback, so the first line appears a few seconds after you press play and the transcription then
stays ahead of you. If the download hasn't reached the part you're listening to yet, the player says how far it has
got. When it reaches the end it goes back and fills in the part you skipped. If
you jump to a part that hasn't been transcribed yet, it continues from there.

The result is saved on this device and reused next time. It is also listed under the play bar as a transcript: click a
line to play from there. Transcripts made on your device are not synced.

Only one episode is transcribed at a time. Switching to another episode pauses the first one; what was done is kept.

## Styles

Choose how subtitles look in **Settings → Subtitles → Subtitle style**, where every style plays a live preview, or
from the small arrow next to the CC button in the play bar.

| Style | What it does |
| --- | --- |
| Karaoke (default) | Each word lights up as it is spoken |
| Typewriter | Letters appear as they're spoken, with a blinking cursor |
| Bounce | Each word pops in as it's spoken |
| Glass | A frosted, see-through panel with a progress line |
| Bilingual | The original above, the translation below (turns on translation) |
| Emphasis | Big type; what has been said gets a highlighter sweep |
| Scroll | The previous and next lines too, gliding up as the talk moves on |
| Particles | Words drift in from a soft glow |
| Speaker labels | A colored name tag for whoever is talking |
| Waves | A gently flowing background |
| Minimal | Quiet and simple |

The word timing comes from the recognizer: SenseVoice and Parakeet give the time of every word or character.
Whisper and shows' own transcripts don't, so the line is spread evenly over the time it takes. If your system has
Reduce Motion turned on, the styles keep their colors and drop the movement.

## Telling speakers apart

With **Tell speakers apart** on, each person gets a color, in every style, and the transcript under the play bar
names them (Speaker 1, Speaker 2, … in the order they first speak). When a show's transcript names its speakers,
those names are used.

For speech recognized on your device, Postflow needs a second, small model that recognizes voices: **3D-Speaker
CAM++** (27 MB, in Settings → Subtitles under Voice model). With it installed, every few seconds of speech are
compared with the voices heard so far in that episode. It works alongside whichever recognition model you use and
runs only on your device. Episodes transcribed before you installed it don't have speakers; choose **Transcribe
again** in their transcript.

## Translating subtitles

Turn on **Translate subtitles** (in Settings → Subtitles or the menu next to the CC button). When an episode isn't
in your language, a translation appears under each line. It uses what you chose in **Settings → Translation**: the
target language, and either the free offline models (Postflow asks before downloading one) or your AI service.

Only the lines around where you're listening are translated, a minute or so ahead. Translations are kept on this
device with the subtitles, so listening again, or reopening Postflow, doesn't translate (or, with an AI service,
pay for) anything twice.

## Speech recognition models

Models are not part of the installer. Download one in **Settings → Subtitles**; you can keep several and switch at any
time.

| Model | Languages | Download | Notes |
| --- | --- | --- | --- |
| SenseVoice Small | Mandarin, Cantonese, English, Japanese, Korean | 229 MB | Best for Chinese; adds punctuation; the fastest |
| Whisper Small | about a hundred languages | 358 MB | A good default for English and most other languages |
| Whisper Base | about a hundred languages | 153 MB | Smaller and faster than Small, less accurate |
| Parakeet TDT 0.6B v3 | English and 24 other European languages | 640 MB | High accuracy with punctuation and capitalization; no Chinese |

The list comes from [`public/catalog/speech.json`](../public/catalog/speech.json) in this repository. Postflow checks
it online once a day, so new models can be added without a new release, as long as the bundled speech runtime
([sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)) already supports that kind of model. Models that need a newer
runtime are listed as "Needs a newer Postflow".

Each model is published by its own authors under its own license, shown next to it in Settings.

### Where models are downloaded from

Every file has more than one address: Hugging Face, its mirror hf-mirror.com, and for SenseVoice and Whisper also
[ModelScope](https://modelscope.cn) (魔搭), which is reachable in mainland China. Before downloading a model,
Postflow fetches the first megabyte from each of them and downloads from the fastest; sources that can't be reached
are only tried last. So you don't need to choose anything, whether or not Hugging Face works from your network.
Parakeet is only on Hugging Face so far.

The copies on ModelScope were uploaded by members of the community, not by Postflow. They are only listed when every
file is byte-for-byte the same as the original (same size and SHA-256), and Postflow checks the SHA-256 again after
downloading.

### If the download is slow or fails

A broken download resumes where it stopped, and a file that fails from one address is tried from the next. If it
still doesn't work, download the files yourself and choose **Import from folder**:

1. Open `public/catalog/speech.json` and find the model; each file lists its name and addresses. From mainland
   China, use the `modelscope.cn` addresses.
2. Download those files into one folder, keeping the file names.
3. In Settings → Subtitles, choose Import from folder next to that model and pick the folder.

Postflow checks each file's SHA-256 before installing it, wherever it came from.

### Voice model

**3D-Speaker CAM++** (27 MB, Apache-2.0, from Alibaba's [3D-Speaker](https://github.com/modelscope/3D-Speaker))
is listed separately under Voice model, shown while Tell speakers apart is on. It isn't a recognition model and
has no Use button: once installed it's used together with whichever recognition model you choose.

### Switching models

Choose **Use** next to an installed model. Episodes transcribed from then on use it. Existing transcripts are kept;
to redo one, open the transcript under the play bar and choose **Transcribe again**.

## Where things are kept

Everything is in the `speech` folder of Postflow's local data directory (Settings → Subtitles shows the path): the
models, one small JSON file per transcribed episode, and one per episode and language for translations. Settings → Storage shows how much space they use, and
Settings → Subtitles can clear all saved subtitles.

## When Postflow is used from a browser

With web access turned on, the phone or browser shows the same subtitles. The models live on, and the work is done by,
the computer running Postflow.

## For plugin authors

Plugins can read the subtitles, their speakers and translations as they play, and control the player: see
[`api.player` and `api.subtitles`](plugins.md#apiplayer).

## Limits

- Audio in MP3, AAC (M4A/MP4), Opus (in Ogg or CAF), Vorbis, FLAC and WAV is supported.
- Recognition makes mistakes, especially with names, music and people talking over each other.
- The first line of a part you jumped to may start mid-sentence.
- Telling speakers apart is a best guess: two similar voices can be taken for one person, and short interjections
  or people talking over each other may go unlabeled. At most eight speakers are told apart per episode.
