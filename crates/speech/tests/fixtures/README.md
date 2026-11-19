# Test fixtures

- `tone-440hz.caf`: a 1.5-second 440 Hz sine wave in stereo, encoded as Opus in a CAF container with macOS
  `afconvert -f caff -d opus@48000 -b 32000`. Generated, so there are no rights to worry about. Used to check
  that Opus audio decodes to 16 kHz mono.
- `nightfall-opus.caf`: 6.6 seconds of English speech, "After early nightfall the yellow lamps would light up
  here and there the squalid quarter of the brothels". It is utterance `0.wav` from the test recordings in
  [sherpa-onnx's Whisper models](https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/tree/main/test_wavs),
  taken from the [LibriSpeech](https://www.openslr.org/12) corpus (CC BY 4.0, Vassil Panayotov et al., from
  LibriVox public-domain audiobooks). Re-encoded as 32 kbit/s Opus in CAF with
  `afconvert -f caff -d opus -b 32000`. Used by `recognize.rs` to run real recognition end to end.
