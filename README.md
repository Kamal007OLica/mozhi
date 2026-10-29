<p align="center">
  <img src="brand/logo/mozhi-app-icon-1024.png" width="120" alt="Mozhi — ஃ">
</p>

<h1 align="center">மொழி · Mozhi</h1>

<p align="center"><b>பேசுங்க. எழுத்தாகும்.</b><br>
Open-source Tamil voice typing for macOS and Windows. Runs entirely on your computer.</p>

---

Double-tap **Control**, speak Tamil, Tanglish or English, then double-tap again (or press **Enter**).
Mozhi writes what you said into whatever app you're in: Slack, WhatsApp Desktop, Notes, Word, a browser tab.

- **On-device.** Speech recognition runs locally with [whisper.cpp](https://github.com/ggml-org/whisper.cpp), using Metal on Apple Silicon. No account, no cloud, no telemetry.
- **Three ways to write.** தமிழ் script, Tanglish (`naalaikku meeting irukku`), or English. Auto mode picks per sentence.
- **Never pastes into the wrong place.** The overlay always shows which app the text is going into, and follows you if you switch apps while speaking.
- **Optional local AI polish.** A free LLM running in [Ollama](https://ollama.com) (default `gemma3:4b`) fixes punctuation and keeps English words in English. If it is slow or offline, Mozhi pastes the raw transcript instead.
- **Fast.** Rust core, a ~10 MB app, and the model is preloaded so the first dictation starts instantly.

## Using it

| Do this | To |
| --- | --- |
| <kbd>⌃</kbd> <kbd>⌃</kbd> (Ctrl Ctrl on Windows) | Start listening |
| <kbd>⌃</kbd> <kbd>⌃</kbd> or <kbd>↵</kbd> | Insert into the current app |
| <kbd>esc</kbd> | Cancel and discard the audio |
| Click the mode chip in the overlay | Switch between Auto / தமிழ் / Tanglish / English |
| Drag the overlay | Move it; Mozhi remembers the spot |

Only a clean double-tap counts: Ctrl+C and other shortcuts never start Mozhi. Enter and Escape are
captured only while Mozhi is listening.

### Permissions (macOS)

- **Microphone**: to hear you, and only while you dictate.
- **Accessibility**: to notice the double-tap, read which app and field you're in, and paste.

## Models

Mozhi downloads a speech model on first run (from Hugging Face, verified by SHA-256). The default
is **Whisper large-v3-turbo (Q5)**, a good balance of Tamil accuracy and speed. Smaller and larger
models are available in **Models**. You can also drop any ggml Whisper model (`.bin`), including Tamil
fine-tunes converted with [`scripts/convert-tamil-model.sh`](scripts/convert-tamil-model.sh), into the
models folder.

## Building from source

Requirements: Rust (stable), Node 20+, CMake, and Xcode Command Line Tools on macOS or the MSVC build tools on Windows.

```bash
npm install
```

```bash
npm run desktop:dev
```

```bash
npm run desktop:build
```

The landing page lives in `apps/web` (`npm run web:dev`), and the brand system in `brand/`
(`npm run brand` regenerates every pattern; `python3 brand/scripts/build-book.py` rebuilds the brand book).

## Repository

```
apps/desktop/          Tauri 2 app
  src-tauri/src/       Rust core: controller, engine (audio + whisper), platform (hotkeys, paste)
  src/overlay/         the floating ஃ overlay
  src/app/             settings, models, history, onboarding
apps/web/              landing page (static)
brand/                 tokens, fonts, logo, generated Chola-era patterns, brand book
docs/ARCHITECTURE.md   how the pieces fit together
```

## The name and the mark

**மொழி** means *language*. Its ழ is the sound only Tamil has. The mark is **ஃ** (āytham), a letter unique
to Tamil. Its three dots are the three crowned kingdoms (Chera, Chola, Pandya), the pulli of a kolam,
and the three dots that move with your voice while Mozhi listens. The patterns come from the Chola
century: the Thanjavur vimana (1010 CE), the kingdoms' emblems, Nataraja's ring of fire and the sikku kolam.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Tamil speakers who can help test accuracy across dialects
(Chennai, Kongu, Madurai, Jaffna, Malaysian and Singaporean Tamil) are especially welcome.

## License

[Apache-2.0](LICENSE). Fonts are SIL Open Font License 1.1. Whisper models are MIT-licensed by OpenAI.
