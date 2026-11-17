+++
title = "Servidores de linguagem"
description = "Erros enquanto digita, completar, ir para a definição, hover, formatar e renomear, com um servidor de linguagem que você escolhe."
weight = 5
+++

Um servidor de linguagem é um programa que entende uma linguagem: encontra erros, sabe a que cada nome se refere e sugere como completar. O BIA conversa com ele pelo [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

O BIA não inclui, não baixa e não procura nenhum servidor. Ele só inicia os programas indicados na configuração `language_servers`.

## Configurando um servidor

1. Instale o servidor, por exemplo `rust-analyzer` para Rust (`rustup component add rust-analyzer`) ou `pyright` para Python.
2. No `settings.json`, acrescente o comando dele com o nome da linguagem, como aparece na barra de status:

   ```json
   "language_servers": {
     "Rust": ["rust-analyzer"],
     "Python": ["pyright-langserver", "--stdio"],
     "Go": ["gopls"]
   }
   ```

3. Salve o `settings.json`. O servidor inicia na próxima vez que um arquivo dessa linguagem estiver aberto.

O comando é uma lista: o programa e depois os argumentos. Se o programa não estiver no `PATH` da sessão que iniciou o BIA, use o caminho completo. Se ele não conseguir iniciar, a barra de status avisa.

## O que ele faz

| Recurso | Como |
|---|---|
| Erros e avisos | Sublinhados enquanto você digita. A barra de status mostra quantos são e a mensagem do que está sob o cursor. |
| Hover | Pare o mouse sobre um nome para ver o tipo e a documentação, com o problema daquele ponto primeiro. |
| Completar | Abre enquanto você digita uma palavra ou depois de `.`, ou com <kbd>Ctrl+Space</kbd>. <kbd>↑</kbd> e <kbd>↓</kbd> escolhem, <kbd>Enter</kbd> ou <kbd>Tab</kbd> aceitam, <kbd>Esc</kbd> fecha. |
| Ir para a definição | <kbd>F12</kbd>, ou <kbd>Ctrl</kbd>+clique. |
| Formatar o arquivo | <kbd>Shift+Alt+F</kbd>. |
| Renomear | <kbd>F2</kbd> no editor pede o novo nome e renomeia em todo lugar que o servidor encontrar. Arquivos que não estavam abertos abrem com a mudança, sem salvar. |
| Próximo problema | <kbd>F8</kbd>, e <kbd>Shift+F8</kbd> para o anterior. |

Roda um servidor por linguagem, para a pasta aberta. Mudar `language_servers` e salvar reinicia os servidores.

## A rede

Servidores de linguagem são programas à parte, e alguns acessam a rede por conta própria: o rust-analyzer roda o `cargo`, que pode baixar dependências, e o gopls roda o `go`. O BIA inicia todo servidor com estas variáveis, que desligam os downloads que essas ferramentas conhecem:

| Variável | Efeito |
|---|---|
| `CARGO_NET_OFFLINE=true` | O cargo usa só o que já foi baixado. |
| `GOPROXY=off`, `GOTOOLCHAIN=local` | O go não baixa módulos nem toolchains. |
| `PIP_NO_INDEX=1` | O pip não usa o índice de pacotes. |
| `NPM_CONFIG_OFFLINE=true` | O npm usa só o cache. |

Com dependências que ainda não foram baixadas, o servidor sabe menos sobre elas, em vez de buscá-las. No Linux, `unshare -rn bia` tira a rede do BIA e de todo programa que ele inicia. Veja [Sem código de rede](@/docs/network.pt-br.md).
