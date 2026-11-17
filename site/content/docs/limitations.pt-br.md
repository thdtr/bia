+++
title = "Limitações conhecidas"
description = "Limitações atuais do BIA e recursos que ele ainda não tem."
weight = 8
+++

## Limitações atuais

- **Linhas longas não quebram.** Os números de linha e o texto dividem a mesma área de rolagem para ficarem sempre alinhados, e o componente de edição só permite isso sem quebra de linha. Linhas longas rolam na horizontal.
- **Arquivos muito longos demoram para abrir.** O arquivo inteiro é diagramado na abertura: cerca de 0,3 segundo para 3.000 linhas e 0,8 segundo para 10.000. Arquivos com mais de 10.000 linhas abrem sem números de linha, o que mantém a rolagem e a digitação rápidas.
- **Quebras de linha mistas não são preservadas.** Um arquivo que mistura LF e CRLF é salvo com a mais frequente das duas. A barra de status avisa isso antes do salvamento.
- **Tabs ocupam oito colunas na tela.** O componente de edição não permite alterar isso. O `tab_size` define apenas o que a tecla <kbd>Tab</kbd> insere.

## Ainda não disponível

O BIA ainda não tem terminal, integração com git, depurador, suporte ao Language Server Protocol (LSP), plugins nem recursos de IA. Alguns deles podem ser adicionados no futuro. Como cada um aumentaria bastante o código (só o suporte a LSP dobraria seu tamanho, aproximadamente), uma proposta deve começar por uma [issue](https://github.com/thdtr/bia/issues), para ser discutida antes de qualquer código.

Recursos que dependem de rede, como telemetria, atualização automática, contas ou sincronização, não serão adicionados. Veja [Sem código de rede](@/docs/network.pt-br.md).
