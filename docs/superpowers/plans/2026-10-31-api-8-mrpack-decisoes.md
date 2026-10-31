# API 8, parte 6 (o Minecraft com mods: o `.mrpack`, o envio do pack e `SeeleUI.salvar`) — decisões, provas e medidas

Sobre `desenho/api-8-partes` a partir de `9f2a444a`. Spec:
`docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`, §6
(`SeeleUI.salvar`), §8 (o envio do `.mrpack`, os 64 MiB, a máquina que o busca
da metade de servidor pelo volume, o hash no aceite, o caminho do link do
Modrinth) e «Minecraft, a primeira receita» → «Com mods». Decisões de antes que
esta parte usa: Q1 (as duas portas do download), Q3 (a marca de sistema, campo
no fim da struct do 12), Q5 e Q6 (`escrever` e `desempacotar`) da parte 4, e
M14 (`rodar` é um comando só) da parte 3. Cada decisão diz o custo, se estiver
errada. O MOD de multiplayer tem as dele em `DECISOES.md`.

## Os nomes que saíram (JS)

**Na metade de cliente**, só para o MOD da 8 com o poder `porta` (a capacidade
`salvar`, em `compatibilidade.capacidades-por-poder.porta` ao lado de `tunel`):

- `SeeleUI.salvar({ pedido, nome, canal })` → `Promise<{ bytes } | null>`: o
  diálogo de salvar do sistema, de dentro de um clique (o bilhete de gesto do
  túnel); `pedido` vai ao `aoPedir`, que autoriza com `{ ok: true, volume }`;
  `nome` é só a sugestão. O caminho nunca volta. Recusas em `recusas.salvar`.
- `SeeleUI.enviar(arquivo, token)` aceita o `.mrpack`
  (`application/x-modrinth-modpack+zip`, conferido pelos bytes), até 64 MiB,
  quando o `arquivo` pede `tipos: ['pack']` e a espera aceita `mrpack`. O guia
  e `volume.cliente` escrevem os tipos por extenso, cada um com o seu teto.
  `arquivo-grande-demais` chega com `tamanho` e `teto`.

**Na metade de servidor**, só para o MOD com metade de máquina
(`Portas::maquinas`):

- `volume.pack(caminho)` → `{ ok: true, bytes, sha512, indice } | { ok: false, erro, ... }`.
- `mundo.buscarPack({ url, sha512, tamanho })` → o mesmo objeto.
- `volume.esperar(..., ['mrpack'], ...)` e a oferta com `instalar: { comando, argumentos }`.
  Recusas em `recusas.pack`.

**Na metade de máquina** (`maquina.programa`):

- `trazer({ pedido, destino })` → `Promise<{ destino, tamanho, hash, jaEstava }>`.
- `ler({ arquivo })` → `Promise<texto>`.
- `desempacotar({ arquivo, destino, raiz, mesclar })`.
- `baixar(...)` → `{ ..., jaEstava }`.
- `rodar` aceita o `instalar` da oferta.
- Recusas novas: `sem-pack-aceito`, `pack-diferente`, `nao-e-texto`
  (`recusas.programa`), e `pack-nao-veio` (`recusas.programa-da-cola`).

## Decisões

- **P6-0. O protocolo continua 12, e a v8 segue sem release.** Conferido em
  31/10 pelo `curl` do CLAUDE.md: a mais nova é a v0.18.3 (a parte 4 leu nela
  o `17554c28`, protocolo 11 e sem `api/v8.json`). *Custo:* uma release com o
  12 fora obrigaria o `instalar` a virar uma mensagem nova (M14), e não um
  campo.
- **P6-1. O passo de instalar é um campo novo no fim da oferta**
  (`OfertaDeMaquina.instalar: Option<ComandoOferecido>`, `#[serde(default)]`),
  e não uma lista de comandos nem uma mensagem nova. O M14 previa uma mensagem
  no fim da lista; com o 12 sem release, o precedente do Q3 (`plataformas`) é
  mais barato: o campo entra no resumo sozinho (mudar o instalador pede aceite
  novo; o teste do resumo cobre `instalar`, `instalar.comando` e
  `instalar.argumento`), a validação é a do `rodar`, e só a linha
  `servidor:PedidoDeMaquina` de `fio_2_0.txt` e a semente
  `servidor-pedido-de-maquina` mudaram. Na `Politica`, `instalar` ao lado de
  `comando`; **`rodar` aceita o par inteiro de um dos dois** (o comando e os
  argumentos dele), e nada que misture: os dois são o mesmo `java/bin/java`.
  *Custo:* um loader com dois passos de instalação (nenhum dos quatro tem)
  pede outro campo; `ArgumentosForaDoAceito` mostra os argumentos do primeiro
  comando com aquele programa (o `rodar`), e não os dois.
- **P6-2. O `.mrpack` é reconhecido pelos bytes** (`seele_proto::pack`: um zip
  com cabeçalho local no começo e `modrinth.index.json` exato na raiz do
  diretório central; zip64 recusado), no seletor (`tipo_do_escolhido_para`) e
  no servidor (o arquivo de obras relido só no diretório central e na entrada
  do índice, que precisa abrir). *Custo:* um `.mrpack` com o índice numa pasta
  ou em zip64 é `formato-desconhecido`.
- **P6-3. O teto é o do tipo**: o fio de volume passa a levar até 64 MiB
  (`PedidoDeImagem::Enviar.bytes` e `RespostaDeImagem.bytes`, o maior dos
  tipos), e o servidor confere o do tipo — o da espera (64 MiB se ela aceita
  `mrpack`, 10 MiB no resto) **antes do primeiro byte**, quando o pedido diz o
  tamanho, e no corte quando não diz; o da leitura pelos bytes do arquivo
  servido. O cliente lê até 64 MiB. *Custo:* uma espera que aceita `mrpack` e
  `png` deixa uma imagem de 64 MiB passar no fio (o MOD a recebe como imagem e
  a janela a recusa ao decodificar).
- **P6-4. O seletor só oferece o pack a quem pede o papel `pack`**
  (`tipos: ['pack']` no `arquivo`, extensão `mrpack`), com o teto de 64 MiB;
  uma imagem ou um som acima de 10 MiB num pedido que aceita o pack é recusado
  com os números. O pack escolhido fica na memória do app, como a imagem.
  *Custo:* quatro packs de pé ocupam até 256 MiB da memória do app até o MOD
  os soltar (o MOD de multiplayer solta depois do envio).
- **P6-5. A recusa do teto leva o tamanho e o teto no nome**
  (`arquivo-grande-demais:<tamanho>:<teto>`), e a casca os faz campos
  (`recusaParaOMod`), no seletor, no envio e na leitura. *Custo:* um formato a
  mais no sufixo, escrito à mão em `base.js`.
- **P6-6. O SEELE abre o índice para a metade de servidor** (`volume.pack`,
  `mundo.buscarPack`; `seele-server/src/mods/pack.rs`, `flate2` — 0 crate
  novo, só a aresta). A oferta precisa do que o pack pede (a versão do
  Minecraft, o loader e a versão dele) **antes** do aceite, a metade de
  servidor não lê bytes do volume, e o `mundo.buscar` só devolve texto. O
  produto só tira o texto do zip (stored ou deflate, até 4 MiB descomprimidos,
  o tamanho conferido contra o do diretório); as regras do índice são do MOD.
  O `buscarPack` usa os tetos de `mundo.buscar` (2 s, 4 MiB). *Custo:* um pack
  pelo link acima de 4 MiB não entra (vai pelo envio), e a primeira busca TLS
  pode passar de 2 s (a Mojang levou de 3 a 5 s na parte 4); o SHA-512 de 64
  MiB roda na thread dos MODs (~0,2 s).
- **P6-7. A máquina busca o pack pelo fluxo de volume de sempre**
  (`PedidoDeImagem::Ler`, pela estrela; `Fio::ler_volume` →
  `ler_imagem_mod`), o mesmo pedido que serve quem joga: o `aoPedir` da metade
  de servidor autoriza, como um pedido de janela de quem opera a máquina. O
  núcleo (`Programa::trazer`) só grava bytes com o hash de `oferta.pack`
  (`Politica.pack`), de uma vez, dentro da pasta e do teto de disco.
  *Custo:* os bytes ficam inteiros na memória (até 64 MiB); o prazo de 60 s do
  fluxo corta um pack de 64 MiB abaixo de ~1,1 MB/s.
- **P6-8. `programa.ler`** (texto UTF-8 até 4 MiB, a régua de caminho de
  `escrever`, sem aceite): a máquina lê o índice do pack que abriu. *Custo:* o
  MOD lê qualquer texto da pasta do servidor de jogo dele.
- **P6-9. `desempacotar` com `mesclar`**: o que sai entra por cima do destino,
  arquivo a arquivo (cada destino conferido de novo contra a raiz e os links),
  e o resto fica; `destino: ""` (a própria pasta) só com `mesclar`, e a parcial
  fica dentro da raiz. Sem `mesclar`, `""` é `caminho-vazio`: trocar a pasta
  inteira apagaria o mundo. *Custo:* o que se troca conta duas vezes no teto
  (medido uma vez, no começo, como o Q8).
- **P6-10. Já está: o `baixar` confere o destino antes da rede** (o tamanho e
  o hash do algoritmo mais forte declarado, em blocos, fora do runtime),
  depois da política — um arquivo que já está não passa por cima de uma URL
  fora do aceite —, e responde `jaEstava: true` sem reservar disco nem pedir.
  O `trazer` faz o mesmo com o hash do aceite. *Custo:* cada partida lê de novo
  os ~105 MB para o hash (medido: pronto em 2,0 s na segunda partida, contra
  19,8 s na primeira).
- **P6-11. `SeeleUI.salvar` é do poder `porta`** (a capacidade `salvar`, que o
  Rust confere em `salvar_do_mod` pela tabela dos MODs de pé, como o túnel) e
  **só de dentro de um clique**: o diálogo do sistema não abre sem a pessoa ter
  pedido, e o bilhete é o do túnel (2 s, uso único). Primeiro o diálogo, depois
  os bytes; o arquivo entra de uma vez (parcial e `rename`). O nome sugerido é
  limpo (letras, números, espaço, `.-_()`, até 80, sem começar por ponto).
  *Custo:* um MOD não oferece salvar fora de um clique; um pack grande é
  baixado depois de a pessoa escolher o lugar.
- **P6-12. A tela do aceite mostra o pack** (o algoritmo e os 16 primeiros hex:
  «o pack do modpack, conferido pelo sha512 …»), **os dois comandos** («antes,
  para instalar, vai rodar: …» e «vai rodar: …») e, como já mostrava, «o jogo
  baixa por conta própria de …» com os domínios do loader que o MOD somou a
  `dominios_do_jogo`. `instalar` entra em `campos_que_mudaram` e na frase do
  que mudou. *Custo:* nenhum conhecido; o hash inteiro não cabe na linha.
- **P6-13. Pelo link, o `.mrpack` é um artefato da oferta** (do
  `cdn.modrinth.com`, com o `sha512` que a API do Modrinth dá, o mesmo de
  `oferta.pack`): a máquina o baixa pela primeira porta, direto do CDN, e nada
  passa pelo envio (spec §8). *Custo:* a tela lista o pack duas vezes (como
  artefato e como pack).

## Os netos (P14 do núcleo) — medido

O `programa` mata o filho, e não a árvore. Na prova A (abaixo), uma amostra de
`ps -axo pid,ppid,command` a cada 250 ms, de todo processo que roda da pasta do
servidor de jogo e de todo filho de um deles:

- **o instalador do Fabric** (`java -jar fabric-installer.jar server ...`):
  nenhum filho visto;
- **o servidor do Fabric** (`java -jar fabric-server-launch.jar nogui`):
  `system_profiler SPDisplaysDataType` e `system_profiler SPMemoryDataType` (a
  sonda de hardware do macOS que o jogo chama ao subir), de vida curta; numa
  das duas rodadas, também um segundo `java` da mesma pasta, filho do servidor,
  cujo comando não foi lido inteiro. Depois do DESLIGAR, nenhum estava vivo.

**Não medido:** um neto que sobreviva a um `encerrar_tudo` no meio da sonda;
Forge, NeoForge e Quilt; Windows e Linux.

## As provas dos guardas (revertidos um a um)

Cada guarda foi desfeito sozinho por um script (o trecho trocado, o teste
rodado, o arquivo restaurado e conferido pelo SHA-256), e o teste caiu:

| # | Guarda revertido | Teste que caiu |
|---|---|---|
| G6-1 | o «já está» do `baixar` (desligado) | `o_arquivo_que_ja_esta_nao_e_baixado_de_novo` (a origem recebeu 2 pedidos) |
| G6-2 | o «já está» só pelo tamanho, sem o hash | `o_arquivo_que_esta_mas_nao_bate_e_baixado` |
| G6-3 | `rodar` só com o `rodar` (o `instalar` fora da política) | `o_passo_de_instalar_e_um_segundo_comando_aceito_e_so_ele` |
| G6-4 | o par comando+argumentos (qualquer argumento do mesmo programa) | o mesmo |
| G6-5 | `trazer` sem conferir o hash do aceite | `trazer_so_grava_o_pack_aceito` |
| G6-6 | `mesclar` ignorado (o destino trocado inteiro) | `desempacotar_por_cima_troca_o_de_mesmo_nome_e_deixa_o_resto` |
| G6-7 | o teto do `ler` (dobrado) | `ler_devolve_texto_da_pasta_e_so_dela` |
| G6-8 | o teto da espera antes do primeiro byte | `o_mrpack_vai_ao_volume_e_volta_com_o_teto_do_tipo` (o tamanho na recusa mudou) |
| G6-9 | o pack pelos bytes no servidor (qualquer zip numa espera de `mrpack`) | o mesmo (o zip sem índice passou) |
| G6-10 | a leitura do pack até 64 MiB (de volta a 10 MiB) | o mesmo (a leitura de 20 MiB recusada) |
| G6-11 | o pack só para quem pede o papel, no seletor | `o_pack_so_para_quem_o_pede_e_com_o_teto_dele` |
| G6-12 | o índice **na raiz** (aceito numa pasta) | `um_zip_sem_o_indice_ou_com_ele_numa_pasta_nao_e` |
| G6-13 | o pack que já está, sem pedir o volume (app) | `o_pack_trazido_so_entra_com_o_hash_do_aceite` |
| G6-14 | `salvar` em `capacidades_de` do poder `porta` | `as_capacidades_por_poder_da_v8_sao_as_do_codigo` |
| G6-15 | `nao-e-texto` em `NOMES_DAS_RECUSAS` | `os_nomes_das_recusas_sao_os_de_cada_variante` |
| G6-T | a linha do `instalar` na tela do aceite | bancada `aceite-da-maquina.cjs`, caso 16, nos dois motores |

**Provado só por raciocínio:** a parcial do `mesclar` dentro da raiz (ela é
apagada no fim, e nenhum teste vê o lugar dela no meio); a conferência do poder
em `salvar_do_mod` e o bilhete do clique no `case "salvar"` de `base.js` (o
comando Tauri não sobe num teste; o guarda das gerações conta o
`recusar_se_morta` dele); o prazo de 60 s do fluxo de volume para 64 MiB.

## O Minecraft com mods de verdade (31/10/2026, um Mac ARM)

O pack de prova (`ferramentas/pack-de-prova.cjs` do MOD; 1 434 bytes, sha512
`e3cf79bd5232ad76…`): Fabric 1.21.4 com o loader 0.16.14, a Fabric API
0.119.4 e o Lithium 0.15.3 (o servidor os baixa do `cdn.modrinth.com`) e o
Sodium 0.6.13 (`env.server: unsupported`, que não é baixado), um `overrides`
e um `server-overrides` que troca um arquivo dele. O plano:
`node ferramentas/plano-de-verdade.cjs plano.json --mods` (o pack enviado ao
volume do anfitrião simulado, como quem configura o envia).

- **A — a metade de máquina de verdade** (`seele-app`,
  `minecraft_de_verdade_pela_metade_de_maquina`, o mesmo comando da parte 4
  com o plano novo; o fio de mentira entrega os bytes do `.mrpack` como o
  volume os serviria). 1ª rodada (pasta nova): baixando aos 17 ms, o pack
  pedido ao volume aos 15,1 s (depois dos ~105 MB do Vanilla e do instalador),
  `instalando` aos 16,3 s, `pronto` aos 19,8 s, o túnel aos 29,5 s; o status
  em `127.0.0.1:25565` respondeu
  `"description":"§bCasa §6com mods","players":{"max":4,"online":0}`; o log do
  jogo diz «Loading 43 mods»; `mods/` tem só a Fabric API e o Lithium;
  `config/seele-prova.txt` é o do `server-overrides`, e
  `config/so-overrides.txt` ficou; a parada pelo dono levou 0,75 s, e a porta
  fechou. 2ª rodada (a mesma pasta): nada baixado, o pack não pedido ao
  volume, o instalador não rodado, `pronto` aos 2,0 s e o túnel aos 7,3 s; a
  parada, 0,56 s.
- **B — o túnel de verdade** (`seele-conformance`, `minecraft_pelo_tunel`, o
  núcleo seguindo o mesmo plano): o pack trazido pelo `trazer` (1 434 bytes,
  o hash do aceite), aberto, lido, os dois arquivos do servidor baixados
  (2 149 128 e 797 555 bytes, sha512), os overrides e os server-overrides por
  cima, o instalador do Fabric com o código 0 em 1,96 s, «Loading 43 mods»,
  «Done» em 8,8 s; o status pela porta local de quem joga, pela malha
  (`Direto { par: 1 }`, 45,2 / 2,7 / 2,2 ms) e pela estrela (`Estrela`,
  2,9 / 2,8 / 3,0 ms), nos dois `"§bCasa §6pelo túnel"`. `encerrar_tudo`
  matou 1 processo em 20,7 ms.
- **O `.mrpack` de 20 MiB pelo fluxo de volume** (`seele-server`,
  `o_mrpack_vai_ao_volume_e_volta_com_o_teto_do_tipo`, servidor e par de
  verdade em `127.0.0.1`): o envio e a leitura de volta em 0,70 s, iguais.

**Não medido:** o pack pelo link de ponta a ponta com rede (o caminho do link
roda nas fixtures do MOD e no QuickJS check, e o `mundo.buscarPack` só nos
testes de unidade); o pack trazido pelo volume **da estrela** na prova A (lá o
fio é de mentira; o fluxo de volume foi medido à parte, em loopback); o
`SeeleUI.salvar` no app inteiro (o diálogo do sistema e o comando Tauri — só
as funções puras e a casca nos testes); Quilt, Forge e NeoForge; Windows e
Linux; duas casas de verdade; um cliente com mods entrando no mundo.

## A suíte

Só os crates e os alvos tocados, por instrução: `seele-proto` (lib 471 e os
alvos de teste; o ouro do fio com só a linha da oferta trocada, e a semente do
fuzz regerada), `seele-core` (lib 549, `programa_arquivos` 11,
`programa_processo` 12, `programa_https` 2), `seele-sessao` (151), `seele-ffi`
(249), `seele-motor` (8), `seele-server` (lib 715, `imagens_de_mod` 2,
`maquina_no_servidor` 19, `tunel_no_servidor` 15), o app (bin: 512 passaram, 3
ignorados — os dois processos filhos e o Minecraft de verdade), `tests/frontend.rs`
(360 passaram, nas **4 falhas da base**: `atender_e_ligar…`,
`no_two_screens…`, `every_element…`, `the_page_never_composes…`; o guarda dos
tipos do prelúdio passou de 11 para 13), e as conformâncias da API, da ponte
de pedidos (a recusa do teto agora leva os números), do aceite dos MODs, do
aceite da máquina, das duas máquinas e do MOD de referência. Fora do pedido, e
para conferir que não são desta parte: `fora_do_ar` e `pedido_recusado` do
`seele-server` falham também em `9f2a444a` (rodados num worktree da base: o
apelido com `#XXXX`). `cargo xtask check-api` limpo; `clippy --all-targets
--all-features -D warnings` limpo nos crates tocados; o `rustfmt` aplicado aos
arquivos tocados que já estavam limpos na base. A bancada
`aceite-da-maquina.cjs`: 17 casos, 0 falhas, nos dois motores. No MOD: `npm
test` 165, `npm run build`, `npm run check` e o QuickJS check (com o caminho
com mods novo) limpos.

## Aberto

- **Forge e NeoForge**: a receita do MOD os declara (o instalador por Maven
  Reposilite, o hash e o tamanho lidos), sem medida; rodam pelo `unix_args.txt`,
  e por isso a receita os marca só para Mac e Linux. Um comando por sistema na
  oferta (o `win_args.txt`) seria um campo novo.
- **O indexador** (`SEELE-MODS-INDEXER`) precisa aceitar `instalar` na oferta
  descrita, e o `mrpack` nos tipos, antes de o MOD sair.
- **O navegador** não tem o papel `pack` no seletor (a ponte web não o lê); um
  MOD com o poder `porta` não sobe lá, e o `salvar_do_mod` da ponte recusa com
  `so-no-app`.
- **O teto de disco** continua sem vigiar o que o jogo e o instalador escrevem
  sozinhos (P9 do núcleo); o disco da oferta com mods é uma estimativa do MOD.

## O que a parte 7 (a reserva) precisa do SEELE

A reserva copia o mundo da principal para a reserva pelo caminho do túnel,
passa a posse em 60 s quando a principal some, e mantém uma dona por vez. Hoje
o SEELE não tem:

1. **Duas máquinas aceitas para o mesmo servidor de jogo.** O livro das
   máquinas guarda uma por servidor de jogo: pedir a outra pessoa dispensa a de
   antes (`MaquinaDispensada { OutroPedido }`). A reserva pede um papel no
   pedido (principal ou reserva), dois aceites de pé, e `servir` o túnel só
   para quem a metade de servidor disser que é a dona agora (hoje é «a máquina
   aceita»), trocada sem aceite novo.
2. **Mandar o mundo de uma máquina a outra.** O `programa` não copia, não
   empacota e não manda nada: falta um `empacotar` (o inverso do
   `desempacotar`, de uma pasta para um `.tar.gz` dentro da pasta) e um fluxo
   de arquivo máquina→máquina sem o teto de 64 MiB do volume (um mundo vai a
   8 GiB na receita), pelo caminho do túnel (direto pela malha, ou pela
   estrela), com hash e retomada; e, na reserva, receber e conferir antes de
   abrir.
3. **Um relógio na metade de servidor.** Os 60 s da posse pedem um momento que
   chegue sem ninguém pedir (um temporizador, ou um `MaquinaSumiu` depois do
   prazo): hoje a metade de servidor só acorda por pedido e por momento de
   pessoa. O `MaquinaParou { saiu }` já diz quando a principal saiu.
4. **A principal que volta sem posse**: o «é a dona?» já é da metade de
   servidor (`ordem: 'desligar'`); falta o SEELE parar o túnel dela quando a
   posse muda, e não só quando ela para.
