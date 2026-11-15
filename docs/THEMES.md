# Themes

A theme is one JSON file with nine interface colors and a map of syntax colors.

BIA ships two:

* **`noite`** (night): warm ink on a near-black, violet-tinted background, hummingbird green accent.
* **`dia`** (day): dark ink on warm paper, a deeper green accent.

Both use a single accent color, and only for things that show where you are: the
active tab, the open file in the tree, the selection and the focused input. If your
theme does the same, it will fit in with the rest of BIA.

## Making your own

1. Copy [`assets/themes/noite.json`](../assets/themes/noite.json) to your themes
   folder, for example as `ember.json`:
   * Linux: `~/.config/bia/themes/`
   * macOS: `~/Library/Application Support/bia/themes/`
   * Windows: `%APPDATA%\bia\themes\`
2. Open it in BIA and edit the colors.
3. Set `"theme": "ember"` in `settings.json` (`Ctrl+,`) and save.

If you save the theme from inside BIA, it's applied right away, so you can adjust
colors and see the result. A user theme with the same name as a built-in one replaces
it.

## Format

```jsonc
{
  "name": "Ember",          // a readable name
  "kind": "dark",           // "dark" or "light"
  "ui": {
    "background": "#16151b", // where you write
    "panel":      "#121116", // sidebar, tabs, status bar
    "border":     "#232129", // 1px hairlines between areas
    "text":       "#dcd6cb",
    "muted":      "#77717f", // hints, inactive tabs, line numbers, explorer icons
    "accent":     "#3fb59b", // the one accent
    "selection":  "#3fb59b33",
    "hover":      "#1e1c24", // hovered/selected rows, code blocks
    "danger":     "#e5736b"  // errors, "move to trash"
  },
  "syntax": {
    "comment": { "color": "#6a6474", "italic": true },
    "string": "#b4c99a",
    "keyword": "#e3967a",
    "entity.name.function": "#ebc77f",
    "markup.heading": { "color": "#3fb59b", "bold": true }
  }
}
```

(The real file is plain JSON. The comments above are only for explanation.)

* Colors are `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
* `syntax` keys are [TextMate scope selectors](https://macromates.com/manual/en/scope_selectors).
  The most specific match wins, so `keyword.operator` overrides `keyword`.
* A syntax value is either a color string or `{ "color", "bold", "italic" }`.
* Text with no matching scope uses `ui.text`.

Useful scopes: `comment`, `string`, `constant.numeric`, `constant.language`, `keyword`,
`keyword.operator`, `storage`, `storage.type`, `entity.name.function`,
`entity.name.type`, `support.type`, `support.function`, `variable.parameter`,
`entity.name.tag`, `entity.other.attribute-name`, `punctuation`, `markup.heading`,
`markup.bold`, `markup.italic`, `markup.underline.link`, `markup.raw`, `invalid`.

## Contributing a theme

A built-in theme should work for most people and use a single accent color. Add the
file to `assets/themes/`, register it in `BUILT_IN` in
[`crates/bia-core/src/theme.rs`](../crates/bia-core/src/theme.rs) (the test suite
checks that it parses), and include screenshots in the pull request.
