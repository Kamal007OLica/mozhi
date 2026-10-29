# README media

Everything under `.github/readme/` is recorded or rendered from the real app and the live site, so the
README never drifts from what Mozhi actually looks like.

| File | What | How |
| --- | --- | --- |
| `banner.png` | Top banner | `stage/banner.html` |
| `demo.gif`, `demo.mp4` | The real overlay dictating into a chat window | `stage/hero.html` wraps `apps/desktop/overlay.html`, whose browser demo cycles every state |
| `voice.gif`, `voice.mp4` | The voice indicator close up | `stage/voice.html` drives `apps/desktop/src/shared/voice.ts` |
| `app.png` | Home, Models and History | real screenshots of the settings app (mock data) in `stage/windows.html` |
| `plate-*.png` | Brand plates | sections of the live landing page, captured with reduced motion |

```bash
npx vite --port 1430 --config apps/desktop/vite.config.ts apps/desktop   # the app UI in a browser; set UI=… if not on 1430
node scripts/readme/record.mjs                          # all tasks, or e.g. `hero voice`
python3 scripts/readme/optimize.py                      # palette PNGs (photos become JPEG), drops raw captures
```

`record.mjs` talks to Chrome over the DevTools protocol with no npm dependencies (`cdp.mjs`), records with
`Page.startScreencast`, and encodes with ffmpeg: H.264 MP4 plus a palette-optimised looping GIF. The hero and
voice recordings start on a state change and stop on the next one, so each GIF is exactly one seamless loop.
