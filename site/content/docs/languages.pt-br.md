+++
title = "Destaque de sintaxe"
description = "As gramáticas que vêm com o BIA, como adicionar uma linguagem e como o BIA escolhe a gramática."
weight = 7
+++

O BIA destaca o código com [definições de sintaxe do Sublime Text](https://www.sublimetext.com/docs/syntax.html) (arquivos `.sublime-syntax`). Cerca de 200 linguagens já vêm incluídas, a partir do conjunto mantido pelo projeto [`bat`](https://github.com/sharkdp/bat).

## Adicionando uma linguagem

Não é preciso recompilar nada.

1. Consiga ou escreva um arquivo `.sublime-syntax`. A maioria das linguagens já tem um, publicado em pacotes do Sublime Text ou no GitHub.
2. Coloque o arquivo na pasta `languages`, dentro da pasta de configuração (veja [Configurações](@/docs/settings.pt-br.md#local)).
3. Reinicie o BIA, ou abra o `settings.json` e salve.

Uma gramática personalizada tem prioridade sobre a embutida para a mesma extensão, o que também permite substituir uma gramática embutida. Um arquivo de gramática inválido é ignorado e o erro aparece na barra de status. Isso nunca impede o BIA de abrir.

### Exemplo

O arquivo `languages/todo.sublime-syntax` destaca listas de tarefas com a extensão `.todo`:

```yaml
%YAML 1.2
---
name: Todo
file_extensions: [todo]
scope: text.todo
contexts:
  main:
    - match: '^\s*\[x\].*$'
      scope: comment.done.todo
    - match: '^\s*\[ \]'
      scope: keyword.open.todo
    - match: '@\w+'
      scope: entity.name.tag.todo
```

As cores vêm do mapa `syntax` do tema ativo, de acordo com o escopo (veja [Temas](@/docs/themes.pt-br.md)).

## Como a gramática é escolhida

O BIA tenta, nesta ordem:

1. o nome exato do arquivo, como `Makefile`, `Dockerfile` ou `CMakeLists.txt`;
2. a extensão, como `.rs`, `.py` ou `.tsx`;
3. a primeira linha, como `#!/usr/bin/env python3` ou `<?xml`;
4. texto simples.

A barra de status mostra a gramática em uso.

## Comentários

<kbd>Ctrl+/</kbd> comenta linhas com o marcador da própria linguagem: `//` em Rust, `#` em Python, `<!-- -->` em HTML. As gramáticas embutidas não trazem esses marcadores, então o BIA os mantém numa tabela própria, para cerca de 100 linguagens. Uma gramática adicionada não está nela, e o <kbd>Ctrl+/</kbd> avisa isso na barra de status.

O nome da linguagem na barra de status também é a chave de `language_servers` (veja [Servidores de linguagem](@/docs/language-servers.pt-br.md)).

## Por que não tree-sitter

O tree-sitter analisa o código com mais precisão, mas cada gramática é código C que precisa ser compilado junto com o editor. As gramáticas do Sublime Text são arquivos de dados, então qualquer pessoa pode adicionar uma linguagem colocando um arquivo numa pasta.
