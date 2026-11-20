+++
title = "Sem código de rede"
description = "O BIA não tem nenhum código que abra uma conexão de rede. Esta página explica como isso é verificado e o que essa garantia cobre."
weight = 8
+++

O BIA não tem nenhum código que abra uma conexão de rede. Ele não envia telemetria, não busca atualizações e não se comunica com nenhum serviço online. No código-fonte, essa característica é chamada de *sealed* (selado).

## Motivação

Um editor tem acesso a tudo o que se escreve nele, inclusive código privado, arquivos `.env` e dados de teste. A maioria dos editores se conecta a vários serviços e permite desativar alguns deles nas configurações. O BIA não depende de uma configuração, porque o código simplesmente não existe.

## Verificação

Duas verificações rodam no CI a cada pull request:

1. O [`deny.toml`](https://github.com/thdtr/bia/blob/main/deny.toml) faz o `cargo deny check bans` falhar se um cliente ou servidor HTTP, uma biblioteca de TLS, um crate de WebSocket, QUIC ou gRPC, ou um SDK de telemetria aparecer em qualquer ponto da árvore de dependências, inclusive nas indiretas.
2. O [`crates/bia-core/tests/sealed.rs`](https://github.com/thdtr/bia/blob/main/crates/bia-core/tests/sealed.rs) procura em todos os arquivos `.rs` do repositório por `std::net`, `TcpStream`, `UdpSocket`, módulos de rede de runtimes assíncronos e nomes de crates de rede. Ele também verifica o `Cargo.lock`.

Por isso, pull requests que adicionam acesso à rede não podem ser incorporados. Propostas que envolvem rede devem ser discutidas antes numa [issue](https://github.com/thdtr/bia/issues), onde é possível buscar alternativas.

### Executando as verificações

```sh
cargo install cargo-deny
cargo deny check bans
cargo test -p bia-core --test sealed
```

No Linux, também é possível iniciar o BIA sem nenhum acesso à rede, e ele funciona da mesma forma:

```sh
unshare -rn bia ~/codigo/projeto
```

## Abrangência

O código do BIA nunca abre uma conexão de rede, nunca faz uma requisição HTTP e nunca consulta um DNS. Os casos abaixo ficam fora dessa garantia:

- **O seletor de pastas.** <kbd>Ctrl+O</kbd> pede ao sistema operacional que mostre a própria janela de seleção (o portal XDG no Linux, o AppKit no macOS, o shell no Windows). Essa janela faz parte do sistema. Abrir pastas pela linha de comando não a utiliza.
- **Comunicação local.** No Linux, o pedido dessa janela passa pelo D-Bus, que usa um socket Unix. Trata-se de comunicação entre programas da mesma máquina, não de rede.
- **Sessões remotas de X11 ou D-Bus.** Se `DISPLAY` ou `DBUS_SESSION_BUS_ADDRESS` apontarem para outra máquina, as bibliotecas que conversam com o ambiente gráfico seguem esse endereço por TCP. Em um desktop comum, os dois são locais.
- **Links no Markdown.** Clicar em um link da web na prévia copia o endereço. O BIA não abre o navegador. Links para arquivos do seu disco abrem no BIA.
- **Imagens no Markdown.** A prévia não carrega imagens, locais ou remotas. Ela mostra o texto alternativo.
- **Unidades de rede.** Ao abrir uma pasta em NFS ou SMB, quem faz o acesso à rede é o sistema operacional. O BIA apenas lê os arquivos.
- **Programas que o BIA executa.** Alguns recursos executam outros programas, que são separados do BIA:
  - **git**, para mostrar linhas e arquivos alterados e a branch. O BIA só pede que ele leia o repositório, e desliga o que poderia fazê-lo iniciar outros programas ou acessar um servidor: hooks, o daemon fsmonitor e todo transporte (`protocol.allow=never`).
  - **Servidores de linguagem**, só os indicados em `language_servers`. O BIA não baixa nenhum, e os inicia com os downloads do cargo, go, pip e npm desligados. Veja [Servidores de linguagem](@/docs/language-servers.pt-br.md#a-rede).
  - **O seu gerenciador de arquivos**, quando você escolhe "Mostrar no gerenciador de arquivos". O BIA só pede que ele mostre o arquivo.
  - **O seu shell**, no terminal. O que roda nele é escolha sua, inclusive acessar a rede.

  Para uma garantia total no Linux, `unshare -rn bia` também tira a rede de todo programa que o BIA inicia.
- **Binários de outras origens.** A garantia vale para binários gerados a partir deste repositório. Compile o BIA você mesmo, ou confirme que o arquivo de uma versão foi gerado pelo CI do repositório:

  ```sh
  gh attestation verify bia-x86_64-unknown-linux-gnu.tar.gz --repo thdtr/bia
  ```

## Recursos que precisam de rede

A ausência de código de rede é um princípio central do BIA, por isso recursos que dependem de rede não entram no editor. Muitos deles funcionam bem como ferramentas à parte, usadas junto com o BIA, e ideias assim são bem-vindas numa [issue](https://github.com/thdtr/bia/issues).
