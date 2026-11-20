# apps/web — Joona website

A standalone, dependency-free static web page for Joona, the free voice dictation app.
Its styling mirrors the Joona desktop app (`apps/desktop`): the design tokens are
copied from `apps/desktop/src/styles/globals.css` and the UI patterns (cards,
buttons, text sizes, radii, shadows) from `apps/desktop/src/components/ui`.

## Files

- `index.html` — the page (hero, feature cards, footer; dark mode follows the system).
- `styles.css` — design tokens plus page styles.
- `icons.svg` — sprite of the lucide icons used by the page.
- `fonts/` — the app's fonts (Inter Variable, Instrument Serif), self-hosted.

## Preview

```sh
python3 -m http.server -d apps/web   # then open http://localhost:8000/
```

(or open `apps/web/index.html` directly).

## Deploy on GitHub Pages

The site is a single folder, so it can be published with GitHub Pages as-is:

1. Commit `apps/web` to your branch.
2. On GitHub, enable Pages with the source pointed at your branch and the
   `apps/web` folder (GitHub Pages supports choosing the folder).
   Alternatively run the deploy-pages Action flow with `path: apps/web`.
3. GitHub assigns the Pages URL; the page adapts to light/dark mode.
