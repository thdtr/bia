+++
title = "Primeiros passos"
description = "Como abrir pastas e arquivos, usar a árvore de arquivos e salvar com segurança."
weight = 2
+++

## Abrindo uma pasta

O BIA trabalha com uma pasta por vez, exibida na árvore de arquivos à esquerda. Há três formas de abrir uma pasta:

- com <kbd>Ctrl+O</kbd> (<kbd>⌘O</kbd> no macOS), que mostra o seletor de pastas do sistema;
- pela linha de comando, com `bia ~/codigo/projeto`;
- iniciando o BIA sem argumentos, o que reabre a última pasta usada.

A última pasta fica registrada em `state.json`, na pasta de configuração. O BIA nunca altera o `settings.json` por conta própria.

## Linha de comando

```text
bia [CAMINHO]...
```

| Argumento | Efeito |
|---|---|
| `bia ~/codigo/projeto` | Abre a pasta. |
| `bia main.rs lib.rs` | Abre os arquivos em abas. A pasta do primeiro arquivo passa a ser o espaço de trabalho. |
| `bia src/main.rs:42` | Abre o arquivo na linha 42. |
| `bia src/main.rs:42:7` | Abre o arquivo na linha 42, coluna 7. |
| `bia --version` | Mostra a versão. |
| `bia --help` | Mostra um resumo de uso. |

## Árvore de arquivos

As pastas aparecem primeiro e depois os arquivos, um por linha. Os nomes listados na configuração `hidden` não são exibidos.

| Ação | Teclas |
|---|---|
| Novo arquivo na pasta selecionada | <kbd>Ctrl+N</kbd> |
| Nova pasta | <kbd>Ctrl+Shift+N</kbd> |
| Renomear | <kbd>F2</kbd> |
| Mover para a lixeira | <kbd>Delete</kbd> |
| Mostrar ou esconder a barra lateral | <kbd>Ctrl+B</kbd> |

Os arquivos apagados vão para a lixeira do sistema operacional e podem ser restaurados de lá. Um arquivo com alterações não salvas não pode ser enviado para a lixeira.

## Localizando arquivos e linhas

<kbd>Ctrl+P</kbd> abre um arquivo pelo nome. Ao digitar parte do nome, a lista é filtrada. Acrescentar `:linha` ao final, como em `main:42`, abre o arquivo nessa linha.

<kbd>Ctrl+G</kbd> leva o cursor a uma linha do arquivo atual. Também é aceito um valor no formato `linha:coluna`.

## Buscar e substituir

<kbd>Ctrl+F</kbd> busca no arquivo atual. <kbd>F3</kbd> e <kbd>Shift+F3</kbd> vão para o resultado seguinte e para o anterior.

<kbd>Ctrl+H</kbd> abre a substituição (<kbd>⌘⌥F</kbd> no macOS, onde <kbd>⌘H</kbd> oculta o aplicativo). É possível substituir um resultado por vez ou todos de uma vez.

## Indentação

O BIA identifica se cada arquivo usa tabs ou espaços e qual é a largura de um nível. Com base nisso, ele:

- mantém a indentação atual ao pressionar <kbd>Enter</kbd>;
- acrescenta um nível depois de `{`, `[` ou `(`;
- indenta ou remove a indentação das linhas selecionadas com <kbd>Tab</kbd> e <kbd>Shift+Tab</kbd>;
- remove um nível com <kbd>Backspace</kbd> dentro dos espaços iniciais.

Arquivos que ainda não têm indentação usam o `tab_size` das configurações. Arquivos Go e Makefiles usam tabs. A barra de status mostra a indentação do arquivo atual.

## Prévia de Markdown

<kbd>Ctrl+Shift+V</kbd> mostra a prévia do arquivo Markdown atual ao lado do texto. A prévia segue o CommonMark, com as tabelas, listas de tarefas e o tachado do GitHub. Ela não carrega imagens, e os links são copiados para a área de transferência em vez de abertos.

## Salvando

<kbd>Ctrl+S</kbd> salva o arquivo atual. Cada vez que um arquivo é salvo, o BIA:

1. grava o novo conteúdo em um arquivo temporário na mesma pasta, com as mesmas permissões;
2. força a gravação do arquivo temporário no disco;
3. renomeia o arquivo temporário por cima do original.

Por isso, um salvamento interrompido nunca deixa um arquivo gravado pela metade. Arquivos com mais de um hard link, e arquivos em pastas onde o BIA não pode criar arquivos novos, são gravados diretamente.

As quebras de linha (LF ou CRLF) e a marca de ordem de bytes do UTF-8 são gravadas como foram lidas. Um arquivo que mistura LF e CRLF é salvo com a mais frequente das duas, e a barra de status avisa isso antes do salvamento.

Se o arquivo foi alterado no disco depois de aberto, o BIA pergunta antes de sobrescrevê-lo. Alterações feitas fora do BIA são detectadas quando a janela volta a ficar em foco.

Um ponto na aba indica alterações não salvas. Fechar a aba ou sair do BIA nessa situação pede confirmação.

## Arquivos que o BIA não abre

Para não danificá-los, o BIA se recusa a abrir:

- arquivos binários;
- arquivos que não estão em UTF-8 válido;
- arquivos maiores que 16 MB;
- arquivos que usam apenas CR como quebra de linha.

No lugar do editor, aparece uma explicação.
