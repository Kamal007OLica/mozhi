# Mozhi architecture

Mozhi is a menu-bar app (macOS) / tray app (Windows) built with **Tauri 2**: a Rust core plus two
small webviews. Speech recognition runs on-device with **whisper.cpp** (via `whisper-rs`); an
optional clean-up pass uses a **local LLM through Ollama**. Nothing leaves the machine.

```
 ┌──────────── Rust core (apps/desktop/src-tauri) ─────────────┐        ┌──── webviews (apps/desktop/src) ────┐
 │                                                              │        │                                      │
 │ platform/  global key hook ──KeyEvent──▶ controller.rs ──────┼─emit──▶│ overlay.html  floating ஃ pill        │
 │            frontmost app / focused field ◀──── poll 250 ms   │        │               live transcript, target │
 │            insert_text (clipboard + ⌘V)  ◀──── commit        │        │                                      │
 │                                                              │◀invoke─│ index.html    settings, models,       │
 │ engine/    cpal mic → 16 kHz → whisper.cpp ──partial/level──▶│        │               history, onboarding     │
 │            models (download/verify), translit, polish        │        │                                      │
 └──────────────────────────────────────────────────────────────┘        └──────────────────────────────────────┘
```

## The dictation loop

1. **⌃⌃** (double-tap Control; Ctrl on Windows): `platform` reports `KeyEvent::DoubleTap`.
2. `controller` captures the **frontmost app** (always shown in the overlay) and the **focused field**
   (shown when available), shows the overlay *without* taking focus, and starts `engine`.
3. While listening, `engine` streams `level` (drives the three dots) and `partial` text; the
   controller polls the frontmost app every 250 ms so the overlay follows the user if they switch apps.
   Enter and Escape are **captured** (swallowed) by the key hook only during this phase.
4. **⌃⌃ or ↵** commits: final whisper pass → optional LLM polish (timeout → raw text) → paste into
   the app the user is in *now* → restore the clipboard → history.
5. **Esc** cancels and discards the audio.
6. **Click the transcript** (or turn on *Pause to edit before inserting*) to stop listening and edit
   the final text in the overlay. The panel takes keyboard focus (`platform::focus_overlay`; on macOS
   a non-activating panel becoming key, so the user's app stays frontmost). **↵** hands focus back
   (`platform::unfocus_overlay`) and pastes into the app captured at ⌃⌃; **⇧↵** is a new line.
7. The **clipboard button** saves the transcript to History and the clipboard without pasting, and
   opens History, where **Copy all** copies every dictation.

## Modules and ownership

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/types.rs` | Serialisable types shared with the UI (mirrored in `src/shared/ipc.ts`). |
| `src-tauri/src/controller.rs` | State machine, event emission, insertion, history. |
| `src-tauri/src/commands.rs` | IPC commands. |
| `src-tauri/src/engine/` | Audio capture, VAD/trim, whisper inference, model catalog + download, Tamil→Tanglish, Ollama polish. |
| `src-tauri/src/platform/` | Key hook, frontmost app + icon, focused element, paste, permissions, overlay panel. |
| `src/shared/` | IPC bridge, brand tokens, shared UI pieces (ஃ dots). |
| `src/overlay/` | The floating pill. |
| `src/app/` | Settings / models / history / onboarding window. |

## Events (Rust → UI)

| Event | Payload |
| --- | --- |
| `mozhi:state` | `{ phase, message, mode }` — idle · starting · listening · transcribing · inserting · done · error |
| `mozhi:level` | `number` 0–1, ~30 Hz, overlay only |
| `mozhi:partial` | `string` rolling transcript, overlay only |
| `mozhi:target` | `{ app: FrontApp \| null, element: FocusedElement \| null }` |
| `mozhi:final` | `string` |
| `mozhi:settings` · `mozhi:history` · `mozhi:navigate` · `mozhi:download` | see `ipc.ts` |

## Models

Default: **Tamil Small** — Whisper small fine-tuned on Tamil by IIT Madras SPRING Lab
(vasista22/whisper-tamil-small, q8_0 ggml, ~265 MB). It is the most accurate Tamil model in the
catalog but Tamil-only: English words come out in Tamil script. **whisper large-v3-turbo (q5_0)**,
~575 MB, is the pick for heavy code-switching and English. With no model chosen, Mozhi uses the
default if it is downloaded, otherwise any installed model. See `engine/models.rs`.

## Language modes

| Mode | Whisper | Post-processing |
| --- | --- | --- |
| Auto | language auto-detect, constrained to ta/en | — |
| தமிழ் | `ta` | optional polish keeps English words in English |
| Tanglish | `ta` | Tamil script → casual Latin (`engine/translit.rs`) |
| English | `en` | — |
