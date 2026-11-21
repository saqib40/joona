# apps/web — Joona website

A standalone, dependency-free static website for Joona, the free voice dictation app.
Its styling mirrors the Joona desktop app (`apps/desktop`): the design tokens are
copied from `apps/desktop/src/styles/globals.css` and the UI patterns (cards,
buttons, text sizes, radii, shadows) from `apps/desktop/src/components/ui`.

## Files

- `index.html` — the home page (hero, feature cards, footer; dark mode follows the system).
- `commands.html` — the command and keyboard shortcut reference for Windows and
  macOS, with the visitor's platform highlighted.
- `styles.css` — design tokens plus page styles.
- `download.js` — detects Windows or macOS and points the download buttons at the
  matching installer in the latest published GitHub release. It caches the
  release for an hour and falls back to the release page if the API is
  unavailable or the release has no asset for that platform. It also marks the
  visitor's platform for `commands.html`.
- `icons.svg` — sprite of the lucide icons used by the page.
- `fonts/` — the app's fonts (Inter Variable, Instrument Serif), self-hosted.

## Preview

```sh
python3 -m http.server -d apps/web   # then open http://localhost:8000/
```

(or open `apps/web/index.html` directly, then use the **Commands** link).

## Deploy on GitHub Pages

The site is a single folder, so it can be published with GitHub Pages as-is:

1. Commit `apps/web` to your branch.
2. On GitHub, enable Pages with the source pointed at your branch and the
   `apps/web` folder (GitHub Pages supports choosing the folder).
   Alternatively enable Pages with the source set to **GitHub Actions** — the
   new `.github/workflows/pages.yml` publishes `apps/web/` on every push to
   main.
3. GitHub assigns the Pages URL; the pages adapt to light/dark mode.
