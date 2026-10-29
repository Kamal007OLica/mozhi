# Contributing to Mozhi

Thanks for helping. Mozhi is small on purpose, so please keep changes focused.

## Ground rules

- **Privacy is the product.** Nothing may send audio, text or usage data off the device. New network
  calls are limited to model downloads the user starts and the user's own local LLM endpoint.
- **Never paste into the wrong place.** Any change to insertion must keep the target app visible in the
  overlay and paste only into the app that is frontmost when the user commits.
- **Tamil first.** UI strings live in `apps/desktop/src/shared/i18n.ts`, Tamil then English. Write the way
  people speak: "மைக்ரோஃபோன்", not a formal coinage.

## Where things go

| Change | Files |
| --- | --- |
| Types shared with the UI | `src-tauri/src/types.rs` **and** `src/shared/ipc.ts` |
| Dictation flow | `src-tauri/src/controller.rs` |
| Audio, models, transliteration, polish | `src-tauri/src/engine/` |
| Hotkeys, focus, paste, permissions, overlay window | `src-tauri/src/platform/` (`macos.rs`, `windows.rs`) |
| Overlay / settings UI | `src/overlay/`, `src/app/` |
| Colours, type, patterns | `brand/` — then run `npm run brand` |

## Before opening a PR

```bash
cd apps/desktop/src-tauri && cargo fmt && cargo clippy && cargo test
```

```bash
cd apps/desktop && npm run build
```

For accuracy changes, include before/after transcripts for the same audio. Add the audio to
`apps/desktop/src-tauri/tests/audio/` only if you recorded it yourself and agree to license it under Apache-2.0.
