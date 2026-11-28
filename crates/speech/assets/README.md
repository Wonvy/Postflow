# Bundled assets

## silero_vad.onnx

The Silero voice activity detection model. It finds where someone is speaking, so that only speech is sent to the
speech recognition model. It is compiled into ShibaPost (about 640 KB); the speech recognition models themselves are
not bundled and are downloaded by the user.

- Source: <https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx>
- SHA-256: `9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6`
- Upstream: <https://github.com/snakers4/silero-vad>
- License: MIT, Copyright (c) 2020-present Silero Team
