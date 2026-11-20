+++
title = "Limitações conhecidas"
description = "Limitações atuais do BIA e recursos que ele ainda não tem."
weight = 9
+++

## Limitações atuais

- **Quebras de linha mistas não são preservadas.** Um arquivo que mistura LF e CRLF é salvo com a mais frequente das duas. A barra de status avisa isso antes do salvamento.
- **Quebras de linha só com CR não são suportadas.** Arquivos que as usam não são abertos.
- **A busca nos arquivos não substitui.** Cada resultado abre o arquivo com o trecho selecionado, onde o <kbd>Ctrl+H</kbd> funciona.
- **O destaque de sintaxe pode errar longe no arquivo.** Saltar mais de 5.000 linhas além do que já foi destacado faz o destaque começar um pouco acima da tela, o que só erra dentro de um comentário ou de uma string mais longa que isso.
- **Os servidores de linguagem recebem o arquivo inteiro a cada pausa na digitação.** É simples e confiável, mas mais lento que mandar só as mudanças em arquivos muito grandes.
- **O texto da prévia de Markdown não pode ser selecionado.** A biblioteca de interface que o BIA usa ainda não faz isso, então copie da aba do próprio arquivo.
- **No Windows, fechar um terminal ocupado não avisa.** O Windows não informa se tem um programa rodando na frente do shell, então o BIA não tem como perguntar antes.
- **Métodos de entrada não foram testados.** Caracteres compostos e métodos de entrada para línguas como o japonês devem funcionar, mas ainda não foram testados.

## Ainda não disponível

O BIA ainda não tem depurador, plugins nem recursos de IA. Alguns deles podem ser adicionados no futuro. Como cada um aumenta bastante o código, uma proposta deve começar por uma [issue](https://github.com/thdtr/bia/issues), para ser discutida antes de qualquer código.

Recursos que dependem de rede, como telemetria, atualização automática, contas ou sincronização, não se encaixam na política de rede do BIA. Eles podem funcionar melhor como uma ferramenta à parte. Veja [Sem código de rede](@/docs/network.pt-br.md).
