# API 8, parte 4 (o poder `programa` na metade de máquina) — decisões, provas e medidas

A cola do núcleo `seele-core::programa` (ramo `desenho/api-8-partes`, a partir de
`219ca17e`) com a metade de máquina da parte 3: a capacidade, o contrato, a
política tirada do aceite guardado, os objetos JS, o ciclo de vida, a tela do
aceite por artefato e a marca de sistema. Spec:
`docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`, §4, §5 e
«Como provar que funciona». Cada decisão diz o custo, se estiver errada.

## Os nomes que saíram (JS)

Só na metade de máquina, só com o poder `programa` (capacidade
`maquina.programa`; `compatibilidade.capacidades-da-maquina.programa` e o bloco
`metade-de-maquina` de `api/v8.json`):

- `maquina.plataforma` — `windows-x64`, `mac-aarch64`, `mac-x64`, `linux-x64`,
  `linux-aarch64` (ou `desconhecida`).
- `maquina.programa.baixar({ url, sha1|sha256|sha512, tamanho, destino, aoProgresso })`
  → `Promise<{ destino, tamanho, algoritmo, hash }>`; `aoProgresso({ recebidos, total })`.
- `maquina.programa.escrever({ destino, texto })` → `Promise<{ destino, bytes }>`.
- `maquina.programa.desempacotar({ arquivo, destino, raiz })` →
  `Promise<{ arquivos, bytes, linksPulados, foraDaRaiz }>`.
- `maquina.programa.rodar({ comando, argumentos, pasta })` → `Promise<processo>`;
  `processo.numero`, `.pid`, `.aoLinha(fn(linha, fluxo))`, `.aoSair(fn(fim))`,
  `.fim` (promessa), `.escrever(texto)`, `.matar()`; `fim = { codigo,
  mortoAPedido, linhasPerdidas, linhasCortadas, linhasNaoOuvidas }`.
- `maquina.programa.recusas()` → `Promise<{ nome: vezes }>`.
- Toda recusa rejeita com `Error`: o nome em `erro`, a frase inteira em `frase`,
  e os dados em campos (`host`, `aceitos`, `url`, `esperado`, `obtido`,
  `usado`, `pedido`, `teto`, `campo`, `aceito`, `origens`...). A lista fechada
  é `recusas.programa` (= `seele_core::programa::NOMES_DAS_RECUSAS`, amarrada
  pelo guarda de conformidade), e as da cola, `recusas.programa-da-cola`.

O idioma é o da parte 3 (`aoAviso`, `aoParar` → `aoLinha`, `aoSair`) e o do
núcleo (`matar`, e não `encerrar`).

## Decisões

- **Q0. O protocolo continua 12, e a v8 segue sem release.** Conferido em 31/10
  pelo `curl` do CLAUDE.md: a v0.18.3 sai de `17554c28` com
  `PROTOCOL_VERSION = 11` e sem `api/v8.json` (`git show`), e a v0.18.2
  (`13918258`) também. *Custo:* uma release com o 12 fora obrigaria a subir para 13.
- **Q1. Duas portas para um download** (a regra que o dono pediu para decidir;
  `conferir_contra_o_aceite`). Uma URL que é de um artefato aceito **para esta
  máquina** só baixa com o hash e o tamanho que a tela mostrou
  (`diferente-do-aceito`); a de um artefato de outro sistema não baixa
  (`artefato-de-outro-sistema`); qualquer outra só de um host de `origens`
  (`fora-dos-artefatos-aceitos`), com o hash que o MOD declarar. Os
  redirecionamentos seguem conferidos contra `dominios` (os hosts dos
  artefatos mais as origens). É o que deixa a parte 6 baixar os arquivos de um
  modpack sem pôr cada um no aceite. *Custo:* um host em `origens` deixa o MOD
  baixar dali qualquer arquivo com o hash que ele escolher — dito na tela
  («e pode baixar outros arquivos, cada um com o hash conferido, de …»). O MOD
  de multiplayer põe `github.com` nas origens (o CDN da Adoptium), e por isso
  o Java não depende só da primeira porta.
- **Q2. O hash do artefato aceito é comparado no mesmo algoritmo.** O mais
  forte declarado no pedido tem de ser o do aceite; um `sha1` certo ao lado do
  `sha256` aceito não vale como ele. *Custo:* um MOD que declare só um
  algoritmo diferente do da oferta é recusado, com os dois nomes na frase.
- **Q3. A marca de sistema: `ArtefatoOferecido.plataformas: Vec<String>`**,
  lista fechada (`PLATAFORMAS_DE_MAQUINA`), sem repetir, vazio = todos.
  Acrescentada no fim da struct do 12 — nasceu neste ramo e nunca saiu; o R6
  da parte 2 é o precedente —, com **só** a linha `servidor:PedidoDeMaquina`
  de `fio_2_0.txt` trocada (o arquivo segue «congelado») e a semente
  `servidor-pedido-de-maquina` do fuzz regerada. Uma lista, e não uma marca só,
  porque o Java de Mac Intel serve também no Mac ARM (`senaoHouver`). O sistema
  da máquina sai do alvo do build (`plataforma_desta_maquina`, por `cfg!`), e
  não do sistema: um app Intel no Rosetta é um processo Intel, e roda o Java
  Intel. **Uma regra só** (`artefatos_para` no proto) conta para a política e
  para a tela (`seele_ffi::programa::artefatos_desta_maquina` vai no evento
  `maquina-pedido` como `artefatos_daqui` e `de_outros_sistemas`). *Custo:*
  a oferta continua levando os cinco Javas pelo fio (alguns KB), e um runtime
  sem marca ainda é somado em todas as máquinas.
- **Q4. A política sai do aceite guardado, e só dele** (`politica_para`,
  `programa_para`): ao ligar, o app lê o aceite do disco; ele tem de ter o
  mesmo resumo **e** o mesmo pacote do pedido que sobe a metade, ou a metade
  sobe sem `Programa` (`sem-aceite`, `aceite-diferente`, `aceite-invalido`,
  `programa-nao-montou`), cada pedido do `programa` é recusado, contado e dito
  no log. `dominios` = hosts das URLs de todos os artefatos + `origens`;
  `artefatos` = os desta máquina; `comando` = `rodar`; `teto_de_disco` =
  `disco_maximo`; `raiz` = `pasta_do_servidor_de_jogo`. Os `dominios_do_jogo`
  ficam fora (são do jogo, e não do `programa`). Um MOD sem o poder não monta
  `Programa` nenhum (`sem-o-poder`), nem cliente TLS. *Custo:* uma oferta nova
  que espera aceite não para a metade de pé com a velha: ela segue rodando o
  que já foi aceito até o dono responder.
- **Q5. `escrever` é acréscimo mínimo do SEELE.** O jogo lê `eula.txt` e
  `server.properties`, e a metade não tem disco fora do `programa`. Até
  64 KiB, trocado de uma vez (parcial e `rename`), só com a regra da pasta e o
  teto de disco — sem conferir o aceite, porque é exatamente o que a §4 diz
  que não pede aceite novo (a configuração). *Custo:* o MOD escreve qualquer
  texto pequeno na pasta, inclusive um que o comando aceito leia (um
  `@argfile` só se ele estiver nos argumentos, que o aceite prende).
- **Q6. `desempacotar` é acréscimo mínimo do SEELE.** O Java da Adoptium vem
  em `.tar.gz` (Mac, Linux) e `.zip` (Windows), e um QuickJS sem disco não os
  abre. `.tar.gz` pelo `tar` + `flate2`; `.zip` lido à mão (diretório central,
  `stored` e `deflate`, CRC-32 conferido; zip64 e criptografia recusados pelo
  nome) — **zero crate novo no `Cargo.lock`**, só as duas arestas do
  `seele-core`. Só o que está sob `raiz` sai, sem o prefixo; **links não são
  criados**, e são contados; uma entrada absoluta ou com `..` recusa o pacote
  inteiro; tudo vai para uma pasta parcial que só vira o destino no fim, e o
  destino velho só sai então. Os índices do `.zip` são todos conferidos (o
  clippy achou 13 que davam pânico com um pacote quebrado). *Custo:* um pacote
  que precise de link perde-o (o JRE do Mac tem 0 links — medido; Linux e
  Windows não medidos); um JRE em zip64 seria recusado.
- **Q7. A chave da pasta (M10) fica pelo `alvo`.** A mesma chave do aceite: um
  aceite, uma pasta, um processo. Chavear pela impressão faria dois aceites (o
  mesmo servidor por dois endereços) dividirem uma pasta e um mundo, com dois
  `Programa` escrevendo nela — os «dois mundos que se separam calados» ao
  contrário. *Custo:* o mesmo servidor alcançado por outro endereço baixa de
  novo e tem outro mundo.
- **Q8. O disco do `desempacotar` é medido uma vez, no começo**, sem reserva.
  *Custo:* um download simultâneo pode passar do teto pelo tamanho dele (o MOD
  baixa e abre em série).
- **Q9. Parando, nada começa, mas o que roda ainda ouve.** `processo-escrever`,
  `processo-matar` e `programa-recusas` passam com a metade parando; o resto
  volta `parando`. Com processo vivo, o `aoParar` tem até **20 s**
  (`ESPERA_DO_AO_PARAR_COM_PROCESSO`; sem processo, os 2 s da parte 3), e
  depois `encerrar_tudo` mata e espera até 10 s. **Medido** no Minecraft 1.21.4:
  antes do conserto, o `stop` do `aoParar` era recusado, a parada esperava os
  15 s do MOD e o jogo morria sem salvar (15,04 s); depois, 0,80 s e 0,65 s, o
  jogo sai sozinho e a porta fecha. *Custo:* fechar o app com um jogo cujo
  `stop` trava espera até 30 s.
- **Q10. Tentar de novo uma falha antes de qualquer resposta** (até
  `MAX_TENTATIVAS` = 3, 1 s entre elas; uma resposta, mesmo de erro, e uma
  falha no meio do corpo não são repetidas). **Medido** em 31/10 contra
  `piston-data.mojang.com` (Azure Front Door): o primeiro pedido de cada
  processo leva 3 a 5 s, e alguns morrem em 9 a 10 s com «peer closed
  connection without sending TLS close_notify»; o segundo leva ~80 ms (8
  processos novos: 1 primeiro pedido morto; outras 3 mortes nas rodadas do
  dia). **A primeira hipótese estava errada e fica escrita:** um pedido sem
  `User-Agent` falhou e um com passou, e concluí que era o cabeçalho; a medida
  de 4 rodadas alternadas mostrou que era a ordem (o primeiro pedido do
  processo), e o `User-Agent` saiu. *Custo:* uma origem morta custa três
  vezes o prazo de conexão (15 s) antes da recusa.
- **Q11. A saída do processo espera a fila do executor** (até 30 s por linha;
  64 mensagens, 256 KiB), em vez de perder linha — inclusive parando, porque o
  fim é o que o `aoParar` espera. O progresso do download não espera: um que
  não coube é trocado pelo próximo. O prelúdio guarda até 256 linhas de um
  processo antes do `aoLinha`, e conta o resto (`linhasNaoOuvidas`). *Custo:*
  uma metade que não lê segura a tarefa do processo (o núcleo segue contando
  as perdidas dele, `MAX_LINHAS_NA_FILA`).
- **Q12. `encerrar_tudo` em `Viva::encerrar`**, o caminho de todo motivo
  (DESLIGAR, REVOGAR, app fechando, MOD desligado, outro pedido, sem aceite,
  sessão que saiu, metade que falhou), e em `descartar`. Soltar a metade solta
  o `Programa`, que manda matar (P14).
- **Q13. As recusas aparecem na tela do MOD por ele**: `programa.recusas()`
  soma as do núcleo e as da cola, e o MOD de multiplayer as manda junto de
  cada falha (`dados.recusas`), que a aba MÁQUINA já mostra com os dados.
- **Q14. Os tipos do `programa` estão escritos em `tratar`**, e não só em
  `programa::TIPOS`: o guarda do prelúdio (`tests/frontend.rs`) os procura no
  corpo de `tratar`, e passou de 4 para 11.

## As provas dos guardas (revertidos um a um)

Cada guarda foi desfeito sozinho por um script (o trecho trocado, o teste
rodado, o arquivo restaurado e conferido), e o teste caiu:

| # | Guarda revertido | Teste que caiu |
|---|---|---|
| G-P1 | `maquina.programa` em `capacidades_da_maquina` | `o_poder_programa_da_o_programa_a_metade_de_maquina` |
| G-P2 | nome de sistema fora da lista | `a_oferta_fora_dos_tetos_e_recusada` |
| G-C1 | artefato aceito só com o hash mostrado | `o_artefato_aceito_so_baixa_como_foi_mostrado` |
| G-C2 | fora dos artefatos, só das origens (trocado por `dominios`) | `fora_dos_artefatos_so_das_origens` |
| G-C3 | artefato de outro sistema | o mesmo |
| G-C4 | `..` numa entrada do pacote | **passou sozinho**: o `inner_path` logo abaixo também recusa. Desfeitos os dois, `uma_entrada_com_pai_recusa_o_pacote_e_nada_fica` caiu (`Ok`, 1 arquivo escrito fora) |
| G-C5 | CRC-32 do `.zip` | `o_zip_do_windows_vira_a_pasta_do_java` |
| G-C6 | teto de disco ao desempacotar | `desempacotar_para_no_teto_de_disco` |
| G-C7 | teto de disco ao escrever | `escrever_respeita_o_teto_de_disco` |
| G-C8 | link não criado | `o_tar_gz_do_mac_vira_a_pasta_do_java` |
| G-C9 | as tentativas (trocado por 1) | `a_conexao_que_cai_antes_da_resposta_e_tentada_de_novo` |
| G-A1 | o aceite tem de ser deste resumo e pacote | `a_versao_ou_o_pack_mudados_nao_rodam_sem_aceite_novo` |
| G-A2 | `encerrar_tudo` em `encerrar` | `os_processos_morrem_quando_a_metade_para` |
| G-A3 | sem aceite, sem política (trocado por uma inventada) | `sem_o_aceite_nada_baixa_e_nada_roda` |
| G-A4 | parando, o processo ainda ouve | `o_ao_parar_manda_stop_e_o_jogo_sai_sozinho` |
| G-V1 | `recusas.programa` do contrato | `as_recusas_do_programa_da_v8_sao_as_do_nucleo` |
| G-V2 | `capacidades-da-maquina.programa` do contrato | `as_capacidades_por_poder_da_v8_sao_as_do_codigo` |
| G-T1 | a tela lista só `artefatos_daqui` (trocado pela oferta inteira) | bancada `aceite-da-maquina.cjs`, caso 15, nos dois motores |

## O Minecraft de verdade (31/10/2026, um Mac ARM)

Duas provas com rede, fora da suíte (`#[ignore]` e
`SEELE_MINECRAFT_DE_VERDADE`), sobre a oferta e o plano que a metade de
servidor do MOD monta (`node ferramentas/plano-de-verdade.cjs plano.json`, no
MOD): `server.jar` do 1.21.4 (56 880 250 bytes, sha1) e o JRE Temurin
21.0.13+9 de `mac-aarch64` (47 935 740 bytes, sha256); os outros quatro Javas
ficaram de fora, ditos.

- **A — a metade de máquina de verdade** (`seele-app`,
  `maquina::testes::minecraft_de_verdade_pela_metade_de_maquina`): o executor
  QuickJS do app, o `maquina/main.js` do MOD, o `Programa` do aceite, o fio do
  servidor de mentira (responde o plano e anota os estados). Medido:
  - 1ª rodada: a falha do primeiro pedido à Mojang (Q10) chegou à «tela» com
    a URL, o hash, o tamanho, a frase e `recusas: {origem-falhou: 1}`;
  - 2ª rodada (antes do Q9): baixar + abrir em 8,3 s, «Done» e o túnel pedido
    em 15,9 s, o status do jogo em `127.0.0.1:25565` com
    `"description":"§bCasa §6do SEELE","players":{"max":4}`; a parada levou
    15,04 s (o `stop` recusado);
  - 3ª rodada: a mesma falha do primeiro pedido (já com o `User-Agent`, o que
    derrubou aquela hipótese);
  - 4ª e 5ª, com Q9 e Q10: pronto em 14,6 s e 6,3 s, túnel em 22,0 s e 10,2 s,
    o mesmo motd, a parada pelo dono em 0,80 s e 0,65 s, o jogo diz «desligado»
    sozinho e a porta fecha.
- **B — o túnel de verdade** (`seele-conformance`, `minecraft_pelo_tunel`):
  o `Programa` do núcleo baixa (o `server.jar` em 12,2 s, coerente com um
  primeiro pedido morto e a tentativa seguinte; o JRE em 0,8 s), abre 259
  arquivos (157 171 014 bytes, 0 links), escreve, roda; «Done» em 6,6 s. O
  servidor SEELE de verdade, a máquina e quem joga como `Enlace`, o aceite pelo
  caminho do produto; o status vai **pela porta local de quem joga**:
  pela malha `Direto { par: 1 }`, 31,6 / 6,1 / 2,5 ms; pela estrela
  `Estrela`, 2,7 / 2,3 / 2,4 ms; nos dois, `"§bCasa §6pelo túnel"`.
  `encerrar_tudo` matou 1 processo em 18,6 ms, e a porta do jogo fechou.

**Não medido:** duas casas de verdade (as duas pontas estão em `127.0.0.1`; o
«direto» é um par WebRTC na mesma máquina); um cliente do Minecraft entrando
no mundo (o login pede conta da Microsoft — só o status, que é o que a lista
de servidores faz); o app inteiro (a tela do aceite, os comandos Tauri e a
conexão FFI no lugar do fio de mentira da prova A); Windows e Linux (o `.zip`
e o `java.exe` só nos testes de unidade); se o Java cria netos; um download que
cai no meio do corpo.

## A suíte

Só os crates tocados, por instrução: `seele-proto`, `seele-core`, `seele-ffi`,
`seele-motor`, `seele-server` (lib e `maquina_no_servidor`, `tunel_no_servidor`),
as conformâncias da máquina, do túnel, da API e do MOD de referência, o app
(bin: 509 passaram, 3 ignorados — os dois processos filhos dos testes e o
Minecraft de verdade) e `tests/frontend.rs` (360 passaram), que fica nas
**4 falhas da base**
(`epocaDaVista`, `.roster`, `palco-sair-desktop`, `image/png`; nenhuma toca
arquivo desta parte — a 5ª, do guarda do prelúdio, foi consertada no Q14).
`cargo xtask check-api` limpo; `clippy --all-targets --all-features -D warnings`
limpo nos crates tocados; o `fmt` só acusa arquivos que esta parte não toca
e que já vinham assim (`apps/seele-app/src/main.rs`, `aceite_dos_mods.rs`,
`convite_e_mods.rs`, `seele-server/src/maquinas.rs`, `maquina_no_servidor.rs`). A bancada
`aceite-da-maquina.cjs`: 16 casos, 0 falhas, nos dois motores.

## Aberto para a parte 6 (`.mrpack`)

- **O instalador do loader é um segundo comando** (M14): a `Politica` tem um
  `comando` só, e o Forge e o NeoForge pedem `java -jar installer.jar
  --installServer` antes do servidor. A parte 6 acrescenta uma mensagem no fim
  da oferta (ADR 0058) e uma lista de comandos aceitos na `Politica`.
- **`SeeleUI.enviar` para `application/x-modrinth-modpack+zip`**, conferido
  pelos bytes, até 64 MiB (§8), e o caminho do volume até a máquina.
- **`SeeleUI.salvar`**, para entregar o `.mrpack` a quem joga.
- O que o Q1 já deixa: os `files` do pack baixam pela segunda porta, dos hosts
  de `origens` (`cdn.modrinth.com`, `github.com`...), com o `sha512` do índice;
  os `overrides` saem pelo `desempacotar` do `.zip` (deflate). Falta o filtro
  de `env.server` e a ordem `overrides` → `server-overrides`, no MOD.
- **Não há «já tenho este arquivo»**: o MOD baixa de novo a cada partida do
  app (o `preparado` dele é da memória). Um `programa.conferir({ destino,
  sha… })` evitaria ~105 MB por partida.
- O teto de disco não vigia o que o jogo escreve depois (P9 do núcleo).
- O indexador (`SEELE-MODS-INDEXER`) precisa aceitar `maquina`, `poderes` e
  `artefatos[].plataformas` antes de o MOD ser publicado.
- Netos do processo (P14 do núcleo): o Minecraft Vanilla não foi medido.
