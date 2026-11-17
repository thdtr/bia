+++
title = "Temas"
description = "Os temas que vêm com o BIA e como criar o seu."
weight = 5
+++

Um tema é um arquivo JSON com nove cores de interface e um mapa de cores de sintaxe. O BIA vem com dois:

| Tema | Tipo | Descrição |
|---|---|---|
| `noite` | escuro | Texto em tom quente sobre um fundo quase preto, levemente violeta, com destaque verde. É o tema padrão. |
| `dia` | claro | Texto escuro sobre um fundo de papel, com um verde mais fechado como destaque. |

Os dois usam uma única cor de destaque, e só para indicar onde você está: a aba ativa, o arquivo aberto na árvore, a seleção e o campo em foco. Temas personalizados que seguem a mesma regra combinam com o resto da interface.

## Criando um tema

1. Copie o [`noite.json`](https://github.com/thdtr/bia/blob/main/assets/themes/noite.json) para a pasta `themes`, dentro da pasta de configuração (veja [Configurações](@/docs/settings.pt-br.md#local)), por exemplo como `ember.json`.
2. Abra o arquivo no BIA e altere as cores.
3. Defina `"theme": "ember"` no `settings.json` e salve.

Um tema salvo de dentro do BIA é aplicado na hora, o que permite ajustar as cores vendo o resultado. Um tema personalizado com o mesmo nome de um tema embutido o substitui.

## Formato

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

### Cores da interface

| Chave | Uso |
|---|---|
| `background` | Área onde o texto é editado. |
| `panel` | Barra lateral, abas e barra de status. |
| `border` | Linhas de um pixel entre as áreas. |
| `text` | Texto. |
| `muted` | Dicas, abas inativas, números de linha e ícones de arquivo. |
| `accent` | A aba ativa, o arquivo aberto, o campo em foco. |
| `selection` | Texto selecionado. |
| `hover` | Linhas sob o ponteiro ou selecionadas, e blocos de código na prévia de Markdown. |
| `danger` | Erros e a confirmação para mandar um arquivo para a lixeira. |

`kind` pode ser `dark` ou `light`. As cores são escritas como `#rgb`, `#rgba`, `#rrggbb` ou `#rrggbbaa`.

### Cores de sintaxe

As chaves de `syntax` são [seletores de escopo do TextMate](https://macromates.com/manual/en/scope_selectors). Vale a correspondência mais específica, então `keyword.operator` tem prioridade sobre `keyword`. Cada valor é uma cor ou um objeto com `color`, `bold` e `italic`. Um texto sem escopo correspondente usa `ui.text`.

Escopos mais usados: `comment`, `string`, `constant.numeric`, `constant.language`, `keyword`, `keyword.operator`, `storage`, `storage.type`, `entity.name.function`, `entity.name.type`, `support.type`, `support.function`, `variable.parameter`, `entity.name.tag`, `entity.other.attribute-name`, `punctuation`, `markup.heading`, `markup.bold`, `markup.italic`, `markup.underline.link`, `markup.raw` e `invalid`.

## Contribuindo com um tema

Um tema embutido deve funcionar bem para a maioria das pessoas e usar uma única cor de destaque. Para propor um, adicione o arquivo em `assets/themes/`, registre-o em `BUILT_IN` no `crates/bia-core/src/theme.rs` e inclua capturas de tela no pull request.
