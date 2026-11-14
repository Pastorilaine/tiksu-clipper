# Local changes to windows-capture 2.0.1 (MIT, https://github.com/NiiightmareXD/windows-capture)

`src/encoder.rs`, `VideoSettingsBuilder`:
- `output_size(w, h)`: encode at a smaller size than the captured frames (the transcoder scales).
- `profile_id(id)`: H.264 profile (100 = High).
