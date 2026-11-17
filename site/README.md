# Website

The site at https://bia-editor.netlify.app, built with [Zola](https://www.getzola.org)
(one binary, no Node). English is the default language and Brazilian Portuguese lives
under `/pt-br/`.

```sh
cd site
zola serve          # http://127.0.0.1:1111
zola build          # writes public/
netlify deploy --prod --no-build --dir public
```

- `content/`: the home page copy (`_index.md`, in `[extra]`) and the docs. Every file
  has a `.pt-br.md` twin.
- `templates/`: Tera templates. Interface strings are in `config.toml`.
- `static/css/tokens.css`: colours and type. The colours come from BIA's own themes
  (`assets/themes/`).
- `static/fonts/`: Fira Sans and Fira Code, subset from `assets/fonts/` with
  `pyftsubset` (fonttools).
- `static/img/social-*.png`: the cards shown when a page is shared, one per language.
  `python3 tools/make_social_preview.py` (from the repository root) regenerates them.
- `static/img/bia-macos.webp` (2x) and `bia-macos-1x.webp`: the screenshot, BIA on a real macOS desktop. Run the
  Screenshot workflow (`.github/workflows/screenshot.yml`) from the Actions tab and
  convert the `screenshot` artifact to WebP.

The site loads nothing from other servers. `netlify.toml` sets a Content Security
Policy that keeps it that way.
