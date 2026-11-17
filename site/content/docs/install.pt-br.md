+++
title = "Instalação"
description = "Como instalar, atualizar e remover o BIA no Linux, no macOS e no Windows."
weight = 1
+++

O BIA funciona no Linux (x86_64 e ARM64), no macOS (Apple silicon e Intel) e no Windows (x86_64). A instalação não exige permissão de administrador.

## Script de instalação

No Linux e no macOS, execute o comando abaixo no terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh
```

No Windows, execute o comando abaixo no PowerShell:

```powershell
irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
```

O script baixa a versão mais recente para o seu sistema, confere o checksum SHA-256 e instala o BIA na sua pasta de usuário:

| Sistema | Local |
|---|---|
| Linux | `~/.local/bin/bia`, com uma entrada no menu de aplicativos |
| macOS | `~/Applications/BIA.app`, com o comando `bia` em `~/.local/bin` |
| Windows | `%LOCALAPPDATA%\Programs\bia`, com um atalho no menu Iniciar |

Se `~/.local/bin` não estiver no `PATH`, o script mostra a linha que deve ser adicionada à configuração do seu shell.

### Opções

No Linux e no macOS, as opções vêm depois de `sh -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh -s -- --version v0.3.0
```

| Opção | Efeito |
|---|---|
| `--version vX.Y.Z` | Instala uma versão específica em vez da mais recente. |
| `--uninstall` | Remove o BIA. |
| `BIA_PREFIX` (variável de ambiente) | Altera o prefixo de instalação. O padrão é `~/.local`. |

## Atualização

Para atualizar o BIA, execute o comando de instalação novamente. Configurações, temas e arquivos de gramática não são alterados.

## Remoção

No Linux e no macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/thdtr/bia/main/install.sh | sh -s -- --uninstall
```

No Windows:

```powershell
$env:BIA_UNINSTALL = 1; irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
```

A remoção não apaga a pasta de configuração do BIA (veja [Configurações](@/docs/settings.pt-br.md#local)).

## Download manual

Cada versão publicada no [GitHub](https://github.com/thdtr/bia/releases) traz um arquivo compactado por plataforma e um arquivo `.sha256` com o checksum. Todos são gerados pelo CI do repositório, e a origem pode ser confirmada com a CLI do GitHub:

```sh
gh attestation verify bia-x86_64-unknown-linux-gnu.tar.gz --repo thdtr/bia
```

### Aviso na primeira execução

Os binários do BIA não são assinados com certificado pago. Quando o BIA é baixado pela página de versões, o sistema operacional pode bloqueá-lo na primeira vez que ele é aberto:

- **macOS:** abra *Ajustes do Sistema → Privacidade e Segurança* e clique em *Abrir Mesmo Assim*.
- **Windows:** clique em *Mais informações* e depois em *Executar assim mesmo*.

O script de instalação não provoca esse aviso.

## Compilação a partir do código-fonte

Com o Rust (1.88 ou mais recente) instalado:

```sh
cargo install --git https://github.com/thdtr/bia bia
```
