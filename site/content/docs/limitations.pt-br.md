+++
title = "Limitações conhecidas"
description = "O que o BIA ainda não faz e o que ele não vai fazer."
weight = 8
+++

## Limitações atuais

- **Linhas longas não quebram.** Os números de linha e o texto dividem a mesma área de rolagem para ficarem sempre alinhados, e o componente de edição só permite isso sem quebra de linha. Linhas longas rolam na horizontal.
- **Arquivos muito longos demoram para abrir.** O arquivo inteiro é diagramado na abertura: cerca de 0,3 segundo para 3.000 linhas e 0,8 segundo para 10.000. Arquivos com mais de 10.000 linhas abrem sem números de linha, o que mantém a rolagem e a digitação rápidas.
- **Quebras de linha mistas não são preservadas.** Um arquivo que mistura LF e CRLF é salvo com a mais frequente das duas. A barra de status avisa isso antes do salvamento.
- **Tabs ocupam oito colunas na tela.** O componente de edição não permite alterar isso. O `tab_size` define apenas o que a tecla <kbd>Tab</kbd> insere.

## Fora do escopo

O BIA não tem, nem pretende ter, terminal, integração com git, depurador, plugins, recursos de IA, telemetria, atualização automática, contas ou sincronização. Esses recursos precisam de rede (veja [Sem código de rede](@/docs/network.pt-br.md)) ou funcionam melhor como ferramentas à parte.

O suporte ao Language Server Protocol (LSP) é a única exceção que pode ser reconsiderada. Ele executa programas locais e, por isso, não entra em conflito com a política de rede, mas dobraria o tamanho do código, aproximadamente. Antes de qualquer código, é preciso abrir uma issue para discutir o assunto.
