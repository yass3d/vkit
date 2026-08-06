# Embedded semantic models

Vkit embeds two files extracted from the official Google MediaPipe Face
Landmarker bundle.  They are compressed with deterministic gzip (`mtime=0`)
only to reduce the final executable size; inference uses the exact decompressed
bytes identified below.

Bundle URL:

`https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/latest/face_landmarker.task`

Bundle SHA-256:

`64184E229B263107BC2B804C6625DB1341FF2BB731874B0BCC2FE6544E0BC9FF`

Extraction command used on the downloaded task bundle:

```powershell
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($task, $out)
```

## face_detector.tflite

- Decompressed bytes: 229,746
- Decompressed SHA-256: `B4578F35940BF5A1A655214A1CCE5CAB13EBA73C1297CD78E1A04C2380B0152F`
- Embedded gzip SHA-256: `C76EADC6A5F1B3E8DEB806FFC2336ACD98739F8647B5B95084EC8A13DBBE7D59`
- Input: RGB float32 `[1,128,128,3]`, normalized to `[-1,1]`

## face_landmarks_detector.tflite

- Decompressed bytes: 2,553,590
- Decompressed SHA-256: `C7D54204CE0448474C7F3FA9AF494787C0965CBDD6F20FC72867E43046BD43D5`
- Embedded gzip SHA-256: `1F98ABB3BA01A392FF899F013007D665DE023EF7E1B3C22BE644F8FA13430D0E`
- Input: RGB float32 `[1,256,256,3]`, normalized to `[0,1]`
- Outputs: 478 XYZ landmarks, face-presence logit, tongue-out score

The [MediaPipe Face Mesh V2 model
card](https://storage.googleapis.com/mediapipe-assets/Model%20Card%20MediaPipe%20Face%20Mesh%20V2.pdf)
and [BlazeFace model
card](https://storage.googleapis.com/mediapipe-assets/MediaPipe%20BlazeFace%20Model%20Card%20%28Short%20Range%29.pdf)
state Apache License 2.0. Release packaging must retain the model notices and
the Apache-2.0 text at `../vendor/tract-tflite/LICENSE-APACHE`.
