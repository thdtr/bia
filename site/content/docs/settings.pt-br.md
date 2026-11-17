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
  "soft_wrap": false,
  "language_servers": {
    "Rust": ["rust-analyzer"]
  }
}
```

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
| `soft_wrap` | `false` | As linhas longas quebram na borda da janela em vez de rolar para o lado. |
| `language_servers` | `{}` | O servidor de linguagem de cada linguagem, como um comando: o programa e depois os argumentos. A chave é o nome da linguagem que aparece na barra de status. Veja [Servidores de linguagem](@/docs/language-servers.pt-br.md). |

O valor padrão de `hidden` é `[".git", ".hg", ".svn", ".DS_Store", "Thumbs.db", "node_modules", "target"]`.

Valores fora do intervalo aceito são ajustados para o limite mais próximo. As fontes Fira Sans e Fira Code vêm com o BIA e têm a mesma aparência em todos os sistemas.

## Erros

Se o `settings.json` tiver um JSON inválido, o BIA continua funcionando com os valores padrão e mostra o erro na barra de status. Basta corrigir o arquivo e salvá-lo de novo para aplicar as configurações.
