# Mozhi · landing page

Static site for Mozhi: Vite and TypeScript with no client framework, about 4 kB of JS gzipped.

```bash
npm run web:dev      # from the repo root, http://localhost:4321
npm run web:build    # → apps/web/dist (static files)
npm run art --workspace apps/web   # regenerate public/art + public/fonts from brand/ (needs Python 3; PyMuPDF for the touch icon)
apps/web/scripts/og.sh             # re-render the 1200×630 share image (public/og.jpg) with headless Chrome
```

- **Links**: the site URL and every GitHub URL come from `src/site.config.ts` (`SITE.url`, `REPO_URL`). In `index.html` they're written as `%SITE.key%`, and the build fails on an unknown key.
- **Brand**: `scripts/art.py` copies `brand/tokens.css` to `src/tokens.css` and puts the fonts and artwork the page uses into `public/`. The output is committed, so `apps/web` builds on its own with no Python.
- **Motion**: `src/dots.ts` implements the shared ஃ motion spec (idle, listening, transcribing, done, error), the same states as the desktop overlay.
- **Deploy (Vercel)**: deploy from `apps/web` (or set it as the project's Root Directory). Vercel detects Vite, needs no `vercel.json`, and outputs `dist/`.
