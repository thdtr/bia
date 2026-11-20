+++
title = "Configurações"
description = "O arquivo settings.json: onde ele fica, o que cada configuração faz e seus valores padrão."
weight = 4
+++

Todas as configurações do BIA ficam em um único arquivo, o `settings.json`. Não existe tela de configurações. <kbd>Ctrl+,</kbd> (<kbd>⌘,</kbd> no macOS) abre o arquivo no editor, e as alterações valem a partir do momento em que ele é salvo.

## Local

| Sistema | Pasta de configuração |
|---|---|
| Linux | `~/.config/bia/` (ou `$XDG_CONFIG_HOME/bia/`) |
| macOS | `~/Library/Application Support/bia/` |
| Windows | `%APPDATA%\bia\` |

A variável de ambiente `BIA_CONFIG_DIR` substitui essa pasta. Ela é útil para testes e instalações portáteis.

A pasta contém:

```text
bia/
├── settings.json       suas configurações
├── state.json          gravado pelo BIA: as pastas abertas e seus arquivos abertos
├── backups/            gravado pelo BIA: alterações não salvas, até salvar ou descartar
├── history/            gravado pelo BIA: as últimas 30 versões salvas de cada arquivo
├── themes/             temas personalizados (*.json)
└── languages/          gramáticas adicionais (*.sublime-syntax)
```

Se o `settings.json` não existir, o BIA o cria com os valores padrão.

## Exemplo

```json
{
  "font_family": "Fira Code",
  "font_size": 14,
  "line_height": 1.55,
  "tab_size": 4,
  "ui_font_family": "Fira Sans",
  "ui_font_size": 13,
  "theme": "noite",
  "language": "auto",
  "hidden": [".git", "node_modules", "target"],
  "auto_close": true,
  "soft_wrap": "off",
  "rulers": [100],
  "show_whitespace": "selection",
  "trim_trailing_whitespace_on_save": true,
  "languages": {
    "Python": { "rulers": [88] },
    "Go": { "insert_spaces": false }
  },
  "language_servers": {
    "Rust": ["rust-analyzer"]
  }
}
```

"Configurações padrão", no menu ou com <kbd>Ctrl+Shift+P</kbd>, abre todas as configurações com o valor padrão, para consultar ao lado das suas.

Uma configuração omitida mantém o valor padrão.

## Referência

| Configuração | Padrão | Descrição |
|---|---|---|
| `font_family` | `"Fira Code"` | Fonte do editor. Aceita qualquer fonte instalada. |
| `font_size` | `14` | Tamanho da fonte do editor, de 6 a 72. |
| `line_height` | `1.55` | Altura da linha no editor, de 1.0 a 3.0. |
| `tab_size` | `4` | Quantas colunas um tab ocupa na tela, de 1 a 8, e a largura da indentação para arquivos que ainda não têm indentação. Os que já têm mantêm a deles. |
| `ui_font_family` | `"Fira Sans"` | Fonte da interface. |
| `ui_font_size` | `13` | Tamanho da fonte da interface, de 8 a 32. |
| `theme` | `"noite"` | `noite`, `dia` ou o nome de um tema personalizado. Veja [Temas](@/docs/themes.pt-br.md). |
| `language` | `"auto"` | Idioma da interface: `auto` (segue o sistema), `en` ou `pt-BR`. |
| `hidden` | veja abaixo | Nomes que a árvore de arquivos não exibe. |
| `auto_close` | `true` | Digitar `(`, `[`, `{` ou aspas também digita o fechamento. |
| `soft_wrap` | `"off"` | Onde as linhas longas quebram em vez de rolar para o lado. `"edge"` quebra na borda do editor, `"ruler"` na primeira régua (ou na borda, se ela vier antes), e `"off"` não quebra. `true` e `false` continuam valendo. |
| `rulers` | `[]` | Colunas com uma linha fina ao longo do editor, como `[80, 120]`. |
| `indent_guides` | `true` | Linhas finas em cada nível de indentação. |
| `show_whitespace` | `"selection"` | Onde os espaços aparecem como pontos e os tabs como traços. `"none"`, `"selection"` ou `"all"`. |
| `line_numbers` | `true` | Os números das linhas ao lado do texto. |
| `relative_line_numbers` | `false` | Os números contam a partir da linha do cursor, o que ajuda em movimentos como `5↓`. |
| `cursor_blink` | `true` | O cursor pisca por alguns segundos depois de cada tecla. |
| `insert_spaces` | `true` | Arquivos que ainda não têm indentação são indentados com espaços. `false` usa tabs. |
| `autosave` | `"off"` | Salva sozinho. `"focus_change"` salva quando a janela perde o foco, e `"after_delay"` dois segundos depois que você para de digitar. Arquivos que nunca foram salvos esperam você dar um nome. |
| `format_on_save` | `false` | Pede ao servidor de linguagem que formate o arquivo antes de salvar. |
| `trim_trailing_whitespace_on_save` | `false` | Tira os espaços e tabs do fim de cada linha ao salvar. |
| `ensure_final_newline_on_save` | `false` | Põe uma quebra de linha no fim do arquivo ao salvar, se não houver. As quebras que já existem ficam. |
| `restore_on_startup` | `true` | Abre a última pasta quando o BIA inicia. |
| `file_types` | `{}` | Qual é a linguagem de um arquivo, pelo nome ou `*.ext`, quando o BIA erra, como `{ "*.env.local": "Bourne Again Shell (bash)" }`. O nome da linguagem é o da barra de status. |
| `languages` | `{}` | Configurações de uma linguagem só, pelo nome que aparece na barra de status. Veja abaixo. |
| `terminal` | `{}` | O shell e a fonte do terminal. Veja abaixo. |
| `language_servers` | `{}` | O servidor de linguagem de cada linguagem, como um comando: o programa e depois os argumentos. A chave é o nome da linguagem que aparece na barra de status. Veja [Servidores de linguagem](@/docs/language-servers.pt-br.md). |

O valor padrão de `hidden` é `[".git", ".hg", ".svn", ".DS_Store", "Thumbs.db", "node_modules", "target"]`.

Valores fora do intervalo aceito são ajustados para o limite mais próximo. As fontes Fira Sans e Fira Code vêm com o BIA e têm a mesma aparência em todos os sistemas.

## Uma linguagem por vez

`languages` muda algumas configurações para os arquivos de uma linguagem e deixa o resto como está. Aceita `tab_size`, `insert_spaces`, `rulers`, `soft_wrap`, `format_on_save`, `trim_trailing_whitespace_on_save` e `ensure_final_newline_on_save`.

```json
"languages": {
  "Python": { "rulers": [88], "format_on_save": true },
  "Makefile": { "insert_spaces": false }
}
```

## O terminal

```json
"terminal": {
  "shell": ["fish"],
  "font_family": "JetBrains Mono",
  "font_size": 13
}
```

`shell` é o programa e os argumentos dele. Sem ele, o terminal roda o seu shell de login, com a fonte e o tamanho do editor.

## .editorconfig

Quando um projeto tem arquivos `.editorconfig`, o BIA segue o que eles dizem, no lugar destas configurações, para os arquivos que eles cobrem. O BIA lê `indent_style`, `indent_size`, `tab_width`, `trim_trailing_whitespace` e `insert_final_newline`.

## Erros

Se o `settings.json` tiver um JSON inválido, o BIA continua funcionando com os valores padrão e mostra o erro na barra de status. Basta corrigir o arquivo e salvá-lo de novo para aplicar as configurações. Uma chave que o BIA não conhece, quase sempre um erro de digitação, também aparece na barra de status.
