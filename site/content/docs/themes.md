+++
title = "Themes"
description = "The built-in themes and how to write your own."
weight = 5
+++

A theme is a JSON file with nine interface colors and a map of syntax colors. BIA includes two:

| Theme | Kind | Description |
|---|---|---|
| `noite` | dark | Warm text on a near-black, violet-tinted background, with a green accent. This is the default. |
| `dia` | light | Dark text on warm paper, with a deeper green accent. |

Both use a single accent color, and only to show where you are: the active tab, the open file in the tree, the selection and the focused field. Custom themes that follow the same rule fit in with the rest of the interface.

## Creating a theme

1. Copy [`noite.json`](https://github.com/thdtr/bia/blob/main/assets/themes/noite.json) to the `themes` folder inside the configuration folder (see [Settings](@/docs/settings.md#location)), for example as `ember.json`.
2. Open the file in BIA and edit the colors.
3. Set `"theme": "ember"` in `settings.json` and save it.

A theme saved from inside BIA is applied immediately, so colors can be adjusted while looking at the result. A custom theme with the same name as a built-in one replaces it.

## Format

```json
{
  "name": "Ember",
  "kind": "dark",
  "ui": {
    "background": "#16151b",
    "panel": "#121116",
    "border": "#232129",
    "text": "#dcd6cb",
    "muted": "#77717f",
    "accent": "#3fb59b",
    "selection": "#3fb59b33",
    "hover": "#1e1c24",
    "danger": "#e5736b"
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

### Interface colors

| Key | Used for |
|---|---|
| `background` | The area where text is edited. |
| `panel` | Sidebar, tabs and status bar. |
| `border` | One-pixel lines between areas. |
| `text` | Text. |
| `muted` | Hints, inactive tabs, line numbers and file icons. |
| `accent` | The active tab, the open file, the focused field. |
| `selection` | Selected text. |
| `hover` | Rows under the pointer or selected, and code blocks in the Markdown preview. |
| `danger` | Errors and the confirmation to move a file to the trash. |

`kind` is `dark` or `light`. Colors are written as `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.

### Syntax colors

The keys of `syntax` are [TextMate scope selectors](https://macromates.com/manual/en/scope_selectors). The most specific match wins, so `keyword.operator` takes precedence over `keyword`. A value is either a color or an object with `color`, `bold` and `italic`. Text without a matching scope uses `ui.text`.

Frequently used scopes: `comment`, `string`, `constant.numeric`, `constant.language`, `keyword`, `keyword.operator`, `storage`, `storage.type`, `entity.name.function`, `entity.name.type`, `support.type`, `support.function`, `variable.parameter`, `entity.name.tag`, `entity.other.attribute-name`, `punctuation`, `markup.heading`, `markup.bold`, `markup.italic`, `markup.underline.link`, `markup.raw` and `invalid`.

## Contributing a theme

A built-in theme should work well for most people and use a single accent color. To propose one, add the file to `assets/themes/`, register it in `BUILT_IN` in `crates/bia-core/src/theme.rs` and include screenshots in the pull request.
