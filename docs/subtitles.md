# Subtitles for podcasts

[中文](subtitles.zh-CN.md)

Postflow can show what is being said while a podcast plays. Turn on the **CC** button in the player (in the reader, or
in the small player under the list); a line of text follows the audio.

- If the show publishes a transcript with timestamps (Podcasting 2.0), Postflow uses it.
- Otherwise Postflow transcribes the episode **on your own computer**. The audio is not uploaded anywhere.

## How it works

Postflow doesn't listen to the sound coming out of your speakers. It downloads the episode to this device (the same
download as "Download for offline listening"), then reads the file from where you're listening onward. Recognition is
several times to tens of times faster than playback, so the first line appears a few seconds after you press play and
the transcription then stays ahead of you. When it reaches the end it goes back and fills in the part you skipped. If
you jump to a part that hasn't been transcribed yet, it continues from there.

The result is saved on this device and reused next time. It is also listed under the play bar as a transcript: click a
line to play from there. Transcripts made on your device are not synced.

Only one episode is transcribed at a time. Switching to another episode pauses the first one; what was done is kept.

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

### If the download is slow or fails

Every file is tried from more than one address and a broken download resumes where it stopped. If it still doesn't
work, download the files yourself and choose **Import from folder**:

1. Open `public/catalog/speech.json` and find the model; each file lists its name and addresses.
2. Download those files into one folder, keeping the file names.
3. In Settings → Subtitles, choose Import from folder next to that model and pick the folder.

Postflow checks each file's SHA-256 before installing it, wherever it came from.

### Switching models

Choose **Use** next to an installed model. Episodes transcribed from then on use it. Existing transcripts are kept;
to redo one, open the transcript under the play bar and choose **Transcribe again**.

## Where things are kept

Everything is in the `speech` folder of Postflow's local data directory (Settings → Subtitles shows the path): the
models, and one small JSON file per transcribed episode. Settings → Storage shows how much space they use, and
Settings → Subtitles can clear all saved subtitles.

## When Postflow is used from a browser

With web access turned on, the phone or browser shows the same subtitles. The models live on, and the work is done by,
the computer running Postflow.

## Limits

- Audio in MP3, AAC (M4A/MP4), Vorbis, FLAC and WAV is supported. Opus is not yet.
- Recognition makes mistakes, especially with names, music and people talking over each other.
- The first line of a part you jumped to may start mid-sentence.
- Speakers are not told apart.
