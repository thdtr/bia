+++
title = "Primeiros passos"
description = "Como abrir pastas e arquivos, editar com vários cursores, buscar, usar o git e o terminal, e salvar com segurança."
weight = 2
+++

## Abrindo uma pasta

O BIA trabalha com uma pasta por vez, exibida na árvore de arquivos à esquerda. Há três formas de abrir uma pasta:

- com <kbd>Ctrl+O</kbd> (<kbd>⌘O</kbd> no macOS), que mostra o seletor de pastas do sistema;
- pela linha de comando, com `bia ~/codigo/projeto`;
- iniciando o BIA sem argumentos, o que reabre a última pasta usada.

Cada pasta guarda os arquivos que estavam abertos nela e onde estava o cursor. Eles reabrem junto com a pasta. A tela inicial lista as pastas recentes.

Isso fica registrado em `state.json`, na pasta de configuração. O BIA nunca altera o `settings.json` por conta própria.

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
| `bia --new` | Abre uma janela nova, vazia. |
| `bia --version` | Mostra a versão. |
| `bia --help` | Mostra um resumo de uso. |

## Menu e comandos

O botão ☰ à esquerda da barra de status abre um menu com os comandos de sempre, como novo arquivo, nova janela, abrir, salvar e fechar, além das configurações, dos atalhos de teclado e do Sobre o BIA. <kbd>Ctrl+Shift+P</kbd> lista todos os comandos. Digite parte do nome para achar, e <kbd>Enter</kbd> executa.

"Abrir recente…" lista as pastas que você já abriu. <kbd>Ctrl+Shift+N</kbd> abre outra janela do BIA, vazia, e `bia --new` também. No Linux, clicar com o botão direito no ícone do BIA no menu de aplicativos ou na dock também oferece uma nova janela. "Fechar pasta" volta para a tela inicial. Clicar com o botão direito numa aba mostra formas de fechar abas e de copiar o caminho do arquivo.

<kbd>Ctrl+Shift+T</kbd> reabre a última aba fechada, na linha em que ela estava. Soltar um arquivo ou uma pasta na janela abre o que foi solto. O BIA lembra o tamanho e o lugar da janela, e se ela estava maximizada.

## Arquivos novos

<kbd>Ctrl+N</kbd> abre uma aba vazia chamada "Sem título 1". Ao salvar, o BIA pergunta onde guardar. Até lá, o que você digita fica num backup, como qualquer alteração não salva, e volta quando você abre o BIA de novo, mesmo depois de sair. <kbd>Ctrl+Shift+S</kbd> salva qualquer arquivo com outro nome, e "Salvar tudo", no menu, salva todos os arquivos com alterações.

Para criar um arquivo numa pasta, use o botão no alto da barra lateral ou clique com o botão direito na árvore.

## Árvore de arquivos

As pastas aparecem primeiro e depois os arquivos, um por linha. Os nomes listados na configuração `hidden` não são exibidos. O arquivo que você está editando fica selecionado na árvore, e a árvore rola até ele.

Os botões no alto da barra lateral criam um arquivo ou uma pasta, recolhem todas as pastas e escondem a barra. Clicar com o botão direito num arquivo ou numa pasta abre um menu com o resto, como recortar, copiar, colar, duplicar, copiar o caminho, renomear, mover para a lixeira, mostrar no gerenciador de arquivos ou abrir um terminal ali.

Os nomes são digitados direto na árvore. Enter confirma e Esc cancela. Um nome com pastas, como `src/ui/menu.rs`, cria as pastas também. Quando você renomeia um arquivo, o nome já vem selecionado sem a extensão.

Arraste um arquivo ou uma pasta para outra pasta para movê-lo, ou segure Alt para copiar. Arquivos copiados no gerenciador de arquivos podem ser colados na árvore. Uma cópia que ficaria com um nome já usado ganha outro, como `notes copy.md`, e o campo do nome abre para você trocar. O BIA nunca substitui um arquivo ao criar, copiar ou mover.

| Ação | Teclas |
|---|---|
| Renomear | <kbd>F2</kbd> |
| Mover para a lixeira | <kbd>Delete</kbd> |
| Recortar, copiar, colar | <kbd>Ctrl+X</kbd>, <kbd>Ctrl+C</kbd>, <kbd>Ctrl+V</kbd> |
| Mostrar ou esconder a barra lateral | <kbd>Ctrl+B</kbd> |

Recortar, copiar e colar funcionam na árvore depois que você clica numa pasta dela, já que clicar num arquivo leva você para o editor. Clicar com o botão direito no caminho da barra de status copia o caminho.

Arraste a borda da barra lateral para mudar a largura. O BIA lembra a largura e se você escondeu a barra.

Os arquivos apagados vão para a lixeira do sistema operacional e podem ser restaurados de lá. Um arquivo com alterações não salvas não pode ser enviado para a lixeira.

Num repositório git, os arquivos alterados aparecem numa cor e os novos em outra, assim como as pastas que os contêm.

## Localizando arquivos e linhas

<kbd>Ctrl+P</kbd> abre um arquivo pelo nome. Ao digitar parte do nome, a lista é filtrada. Acrescentar `:linha` ao final, como em `main:42`, abre o arquivo nessa linha. Os arquivos listados no `.gitignore` ficam de fora.

<kbd>Ctrl+G</kbd> leva o cursor a uma linha do arquivo atual. Também é aceito um valor no formato `linha:coluna`.

<kbd>Alt+←</kbd> volta para onde o cursor estava antes de um salto (abrir um arquivo, ir para uma linha ou uma definição, um resultado de busca), e <kbd>Alt+→</kbd> avança de novo. No Mac, são <kbd>⌃-</kbd> e <kbd>⌃⇧-</kbd>, porque lá <kbd>⌥←</kbd> anda de palavra em palavra.

A palavra sob o cursor ganha uma marca suave nos outros lugares em que aparece na tela. Nos comentários, `TODO`, `FIXME`, `HACK` e `XXX` se destacam. Réguas, guias de indentação e espaços visíveis podem ser ligados nas [configurações](@/docs/settings.pt-br.md).

## Vários cursores

Todos os comandos de edição agem em cada cursor ao mesmo tempo.

| Ação | Teclas |
|---|---|
| Selecionar a palavra e depois a próxima ocorrência | <kbd>Ctrl+D</kbd> |
| Selecionar todas as ocorrências | <kbd>Ctrl+Shift+L</kbd> |
| Pôr um cursor acima ou abaixo | <kbd>Ctrl+Alt+↑</kbd> / <kbd>Ctrl+Alt+↓</kbd> |
| Pôr ou tirar um cursor | <kbd>Alt</kbd>+clique |
| Voltar a um cursor só | <kbd>Esc</kbd> |

Quando a área de transferência tem tantas linhas quanto há cursores, colar dá uma linha a cada um.

## Linhas

| Ação | Teclas |
|---|---|
| Comentar ou descomentar | <kbd>Ctrl+/</kbd> |
| Mover para cima ou para baixo | <kbd>Alt+↑</kbd> / <kbd>Alt+↓</kbd> |
| Duplicar para cima ou para baixo | <kbd>Shift+Alt+↑</kbd> / <kbd>Shift+Alt+↓</kbd> |
| Apagar | <kbd>Ctrl+Shift+K</kbd> |
| Selecionar a linha (de novo para a próxima) | <kbd>Ctrl+L</kbd> |

<kbd>Ctrl+C</kbd> e <kbd>Ctrl+X</kbd> sem nada selecionado levam a linha inteira, e colar a põe acima da linha do cursor.

Digitar `(`, `[`, `{` ou aspas também digita o fechamento, e envolve a seleção se houver uma. A configuração `auto_close` desliga isso. O parêntese que fecha o que está no cursor fica destacado.

Com `soft_wrap` ligado, as linhas longas quebram na borda da janela, e as setas andam pelas linhas da tela. <kbd>Ctrl+=</kbd> e <kbd>Ctrl+-</kbd> mudam o tamanho da fonte até o BIA fechar; <kbd>Ctrl+0</kbd> volta ao normal.

## Buscar e substituir

<kbd>Ctrl+F</kbd> busca no arquivo atual e destaca todos os resultados. <kbd>Enter</kbd>, <kbd>F3</kbd> e <kbd>Shift+F3</kbd> vão para o resultado seguinte e para o anterior.

<kbd>Ctrl+H</kbd> abre a substituição (<kbd>⌘⌥F</kbd> no macOS, onde <kbd>⌘H</kbd> oculta o aplicativo). É possível substituir um resultado por vez ou todos de uma vez.

Três botões ao lado do campo diferenciam maiúsculas, buscam só palavras inteiras e leem o texto como expressão regular. Numa expressão regular, a substituição aceita `$1` para um grupo e `\n` para uma quebra de linha.

## Buscar nos arquivos

<kbd>Ctrl+Shift+F</kbd> busca em todos os arquivos da pasta, na barra lateral, com as mesmas três opções. <kbd>Enter</kbd> inicia a busca. Ficam de fora os arquivos listados no `.gitignore`, os binários e os maiores que 16 MB. Os arquivos abertos são buscados com as alterações ainda não salvas. Clicar num resultado abre o arquivo com o trecho selecionado.

Os dois campos abaixo das opções limitam a busca. O primeiro recebe os arquivos em que buscar, e o segundo os que ficam de fora, como padrões separados por vírgula, por exemplo `src, *.rs` ou `*.lock, tests`. Um padrão vale para um caminho a partir da pasta aberta, para o nome de um arquivo ou de uma pasta.

<kbd>Ctrl+Shift+E</kbd> volta para a árvore de arquivos.

## Git

Num repositório git, barras ao lado dos números mostram as linhas adicionadas e alteradas desde o último commit, e uma marca mostra onde houve linhas apagadas. A barra de status mostra a branch. Tudo isso é atualizado ao salvar e quando a janela volta a ter foco.

O BIA usa o seu `git` para isso, só para ler o repositório. Sem git, ou fora de um repositório, nada aparece.

## Terminal

<kbd>Ctrl+`</kbd>, ou "Terminal" na barra de status, mostra o terminal abaixo do editor, com o seu shell na pasta aberta. Apertar de novo esconde o painel, e os shells continuam rodando.

Cada terminal tem uma aba no alto do painel. <kbd>Ctrl+Shift+`</kbd> ou o botão + abre outro, e o × fecha. Digitar `exit` também fecha. No Linux e no macOS, o BIA pergunta antes quando ainda tem um programa rodando no terminal que você vai fechar, ou quando você sai. Arraste a borda de cima do painel para deixá-lo mais alto ou mais baixo.

Segure <kbd>Ctrl</kbd> e clique num caminho que o terminal mostrou, como `src/main.rs:42:7` ou `src/app.ts(12,5)`, para abrir o arquivo nessa linha.

<kbd>Ctrl+Shift+C</kbd> e <kbd>Ctrl+Shift+V</kbd> copiam e colam. <kbd>Ctrl+C</kbd> vai para o programa, como em qualquer terminal. Atalhos como <kbd>Ctrl+P</kbd> e <kbd>Ctrl+Tab</kbd> continuam funcionando com o terminal em foco.

## Servidores de linguagem

Com um servidor de linguagem configurado, o BIA sublinha erros enquanto você digita, completa código e pode ir para definições, mostrar o que está sob o mouse, formatar e renomear. Veja [Servidores de linguagem](@/docs/language-servers.pt-br.md).

## Indentação

O BIA identifica se cada arquivo usa tabs ou espaços e qual é a largura de um nível. Com base nisso, ele:

- mantém a indentação atual ao pressionar <kbd>Enter</kbd>;
- acrescenta um nível depois de `{`, `[` ou `(`;
- indenta ou remove a indentação das linhas selecionadas com <kbd>Tab</kbd> e <kbd>Shift+Tab</kbd>;
- remove um nível com <kbd>Backspace</kbd> dentro dos espaços iniciais.

Arquivos que ainda não têm indentação usam o `tab_size` das configurações. Arquivos Go e Makefiles usam tabs. A barra de status mostra a indentação do arquivo atual.

## Dois arquivos lado a lado

<kbd>Ctrl+\</kbd>, ou o botão à direita das abas, leva a aba atual para o outro lado da janela, e assim dá para ver dois arquivos ao mesmo tempo. Cada lado tem as suas abas, e os arquivos que você abre vão para o lado em que você está trabalhando. Quando a última aba da direita fecha, a janela volta a ter um lado só.

## Prévia de Markdown

<kbd>Ctrl+Shift+V</kbd> abre a prévia do arquivo Markdown atual numa aba própria, logo depois da aba do arquivo, e a prévia acompanha o que você digita. Na prévia, <kbd>Ctrl+Shift+V</kbd> volta para o arquivo, então o mesmo atalho vai e volta. A prévia fecha como qualquer outra aba. Para ver o texto e a prévia lado a lado, leve um dos dois para o outro lado com <kbd>Ctrl+\</kbd>. A prévia segue o CommonMark, com as tabelas, listas de tarefas e o tachado do GitHub. Ela não carrega imagens. Links para um título, como `#instalação`, rolam a prévia até ele. Links para outros arquivos abrem no BIA, e links da web são copiados para a área de transferência em vez de abertos.

## Salvando

<kbd>Ctrl+S</kbd> salva o arquivo atual. Cada vez que um arquivo é salvo, o BIA:

1. grava o novo conteúdo em um arquivo temporário na mesma pasta, com as mesmas permissões;
2. força a gravação do arquivo temporário no disco;
3. renomeia o arquivo temporário por cima do original.

Por isso, um salvamento interrompido nunca deixa um arquivo gravado pela metade. Arquivos com mais de um hard link, e arquivos em pastas onde o BIA não pode criar arquivos novos, são gravados diretamente.

As quebras de linha (LF ou CRLF) e a marca de ordem de bytes do UTF-8 são gravadas como foram lidas. Um arquivo que mistura LF e CRLF é salvo com a mais frequente das duas, e a barra de status avisa isso antes do salvamento.

Se o arquivo foi alterado no disco depois de aberto, o BIA pergunta antes de sobrescrevê-lo. Alterações feitas fora do BIA são detectadas quando a janela volta a ficar em foco.

Um ponto na aba indica alterações não salvas. Fechar a aba ou sair do BIA nessa situação pede confirmação.

As alterações não salvas também são copiadas para a pasta `backups`, dentro da pasta de configuração, dois segundos depois da última edição. Se o BIA ou o sistema parar antes de salvar, abrir o arquivo de novo traz as alterações de volta. A cópia é apagada quando o arquivo é salvo ou as alterações são descartadas.

Cada salvamento também guarda o texto salvo na pasta `history`, com as últimas 30 versões de cada arquivo. "Histórico local", no menu do botão direito de uma aba ou com <kbd>Ctrl+Shift+P</kbd>, lista as versões por quando foram salvas. Escolher uma põe o texto dela na aba como uma edição, que o <kbd>Ctrl+Z</kbd> desfaz, e nada é gravado até você salvar. Arquivos maiores que 1 MB não entram no histórico.

As configurações podem tirar os espaços do fim das linhas, pôr a última quebra de linha, formatar o arquivo ou salvar sozinhas. Veja [Configurações](@/docs/settings.pt-br.md).

## Arquivos que o BIA não abre

Para não danificá-los, o BIA se recusa a abrir:

- arquivos binários;
- arquivos que não estão em UTF-8 válido;
- arquivos maiores que 16 MB;
- arquivos que usam apenas CR como quebra de linha.

No lugar do editor, aparece uma explicação.
