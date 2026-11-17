+++
title = "BIA, um editor minimalista feliz"
description = "Um editor de código pequeno para Linux, macOS e Windows, feito em Rust. Ele não tem código de rede, então nunca se conecta a nada."
template = "index.html"

[extra]
headline = "Um editor minimalista feliz."
lede = "Um editor de código pequeno para Linux, macOS e Windows, feito em Rust."
tab_unix = "macOS e Linux"
tab_windows = "Windows"
install_note = "Não precisa ser administrador. Se preferir, [baixe o arquivo](#download)."
shot_alt = "O BIA numa janela do macOS, com um projeto Next.js na barra lateral e uma rota de API aberta no editor."
shot_caption = "O BIA no macOS, aberto numa rota de API do Next.js."

features_title = "Tudo o que o BIA faz"
features = [
  { name = "Árvore de arquivos", text = "Crie, renomeie e mande para a lixeira por ela." },
  { name = "Destaque de sintaxe", text = "Cerca de 200 linguagens, mais os seus `.sublime-syntax`." },
  { name = "Prévia de Markdown", text = "Ao vivo, ao lado do texto." },
  { name = "Buscar e substituir", text = "Um por vez, ou todos de uma vez." },
  { name = "Abrir pelo nome", text = "<kbd data-mod>Ctrl+P</kbd> e parte do nome. Com `:42`, vai para a linha 42." },
  { name = "Indentação", text = "Tabs ou espaços, conforme cada arquivo." },
  { name = "Temas", text = "Noite e Dia, ou o seu em JSON." },
  { name = "Português e inglês", text = "Segue o sistema, ou o que você configurar." },
]

left_out_title = "O que ele ainda não faz"
left_out = ["Terminal", "Git", "Depurador", "LSP", "Plugins", "IA"]
left_out_note = "Alguns podem chegar mais tarde. Só o que depende de internet fica de fora."

saves_title = "Salvar nunca estraga um arquivo"
saves_lede = "O BIA grava uma cópia antes e só faz a troca quando ela já está no disco."
saves_steps = ["Grava uma cópia oculta", "Garante que ela foi para o disco", "Renomeia por cima do original"]
saves_notes = [
  "Quebras de linha e BOM ficam como estavam.",
  "Se o arquivo mudou no disco, o BIA pergunta antes.",
  "Arquivos apagados vão para a lixeira.",
]

settings_title = "Um arquivo de configuração"
settings_lede = "<kbd data-mod>Ctrl+,</kbd> abre o `settings.json`. Salvou, a mudança aparece na hora."
settings_link = "Todas as configurações"

sealed_title = "O BIA nunca se conecta à internet"
sealed_lede = "Sem telemetria e sem busca por atualização. O CI falha se entrar código de rede."
sealed_code_label = "Confira você mesmo"
sealed_code = """
cargo deny check bans
cargo test -p bia-core --test sealed
unshare -rn bia ~/codigo/projeto"""
sealed_site = "Este site segue a mesma regra. Sem analytics e sem requisições a outros servidores."
sealed_link = "Como a verificação funciona"

download_title = "Versões para Linux, macOS e Windows"
download_head = ["Sistema", "Processador", "Arquivo"]
download_note = "Esses arquivos não são assinados com certificado pago, então o sistema pode mostrar um aviso na primeira vez. O comando de instalação não gera esse aviso."
download_link = "O que fazer com o aviso"
download_anchor = "aviso-na-primeira-execucao"
+++
