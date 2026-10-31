# API 8, parte 7 (a reserva) — decisões, provas e medidas

Sobre `desenho/api-8-partes` a partir de `c9e908a4`. Spec:
`docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`, «A
reserva», «Sem quem hospeda» e «Uma dona por vez» em «Como provar que
funciona». Decisões de antes que esta parte usa: M2 (servir só o que a oferta
aceita traz), M10/Q7 (a pasta pelo `alvo`), M18 (a metade para quando o enlace
troca de geração), Q1 (as duas portas do download), Q6 e P6-9 (`desempacotar`).
Cada decisão diz o custo, se estiver errada. O MOD de multiplayer tem as dele em
`DECISOES.md`.

## Os nomes que saíram (JS)

**Na metade de servidor** (`mundo.maquinas`, só para o MOD com metade de máquina):

- `mundo.maquinas.pedir({ quem, servidor, papel, oferta })` — `papel`:
  `'principal'` (o padrão, sem a chave) ou `'reserva'`.
- `mundo.maquinas.avisar({ servidor, papel, carga })` — à máquina daquele papel
  (principal sem a chave).
- `mundo.maquinas.dona({ servidor, papel })` — quem é a dona agora; `papel`
  obrigatório.
- `contexto.maquina` = `{ servidor, papel, dona }`.
- Os momentos levam `papel`: `MaquinaAceitou`, `MaquinaRecusou`,
  `MaquinaParou` `{ servidor, papel, pessoa, ... }`; e dois novos:
  `MaquinaSumiu { servidor, papel, pessoa, motivo, segundos }` e
  `MaquinaVoltou { servidor, papel, pessoa }`.
- Recusas novas em `recusas.maquinas`: `papel-invalido`, `dona-invalida`,
  `donas-demais` (lançadas) e `mesma-pessoa-nas-duas` (momento).

**Na metade de máquina** (`maquina.programa`):

- `empacotar({ origem, arquivo })` →
  `{ arquivo, tamanho, sha256, arquivos, bytes, linksPulados }`.
- `oferecerCopia({ arquivo, porta })` → `{ codigo, tamanho, sha256, porta }`.
- `receberCopia({ tunel, codigo, sha256, tamanho, destino, aoProgresso })` →
  `{ destino, tamanho, sha256, retomadas, jaEstavam, caminho }`.
- Recusas novas em `recusas.programa`: `nao-e-pasta`,
  `arquivo-dentro-da-origem`, `porta-fora-do-aceite`, `porta-ocupada`,
  `copia-recusada`, `copia-caiu`, `copia-nao-bate`; e `tunel-recusado` em
  `recusas.programa-da-cola`.

**No servidor SEELE**: `ServerConfig::prazo_da_maquina_sumida` (padrão
`PRAZO_DA_MAQUINA_SUMIDA`, 60 s).

## Decisões

- **R0. O protocolo continua 12, e nada mudou no fio.** Conferido em 31/10
  pelo `curl` do CLAUDE.md: a mais nova é a v0.18.3 (as partes 4 e 6 leram
  nela `17554c28`, protocolo 11 e sem `api/v8.json`). Nenhuma mensagem, campo
  ou ouro mudou: `fio_2_0.txt` e as sementes do fuzz ficam. *Custo:* nenhum.
- **R1. O papel não vai no fio; vai a pessoa.** O livro chaveia por
  `(mod, servidor, papel)`, e o aceite, a recusa, a parada e o pedido da
  máquina — que chegam pela pessoa e pelo servidor de jogo — acham o papel pela
  pessoa, porque **a mesma pessoa não pode ser as duas**
  (`Excesso::MesmaPessoaNasDuas`, que volta como `MaquinaRecusou` com
  `mesma-pessoa-nas-duas`). No app, a pasta e o aceite guardado continuam
  chaveados por `(alvo, mod, servidor)`, e a principal e a reserva são duas
  máquinas, então não se cruzam. *Custo:* uma pessoa não serve de reserva de si
  mesma (dois computadores com a mesma identidade); a tela do aceite não diz
  «reserva» por conta do produto — o MOD põe no título se quiser. Uma mensagem
  com o papel seria um acréscimo no fim, se um dia precisar.
- **R2. Duas máquinas aceitas, e só a dona serve** (`Maquinas::conferir_tunel`):
  um pedido a outra pessoa só dispensa (`OutroPedido`) quem tinha **aquele**
  papel. A máquina aceita que não é a dona e pede para servir ouve o mesmo
  `ForaDoAceite` no fio — **nenhuma variante nova** em `RecusaDeTunel` — e o
  servidor diz `nao-e-a-dona` no log e conta
  (`Maquinas::tuneis_de_quem_nao_e_dona`). *Custo:* a metade de máquina lê
  `fora-do-aceite` e não sabe sozinha que é a posse; o MOD de multiplayer
  pergunta o plano, que diz `dona: false`.
- **R3. A posse é da metade de servidor, e mora na memória** (`Maquinas::donas`;
  `mundo.maquinas.dona`). Ausente é a principal: um MOD de uma máquina só segue
  como era. Passar a posse **não** pede aceite novo e para na hora os túneis da
  dona de antes (`passar_a_posse` → `parar_tuneis`), mesmo com o processo dela
  vivo — é o item 4 do que a parte 6 achou que faltava. A posse sai no despejo
  **antes** dos pedidos e dos avisos da mesma chamada. O servidor nunca passa a
  posse sozinho. *Custo:* um servidor SEELE que reinicia volta à principal até a
  metade de servidor dizer de novo; o MOD de multiplayer diz a cada pedido.
- **R4. O relógio: `MaquinaSumiu`, e não um temporizador genérico.** Toda
  parada de máquina aceita que não é dispensa do servidor (`saiu`, e as da
  pessoa: `pelo-dono`, `revogada`, `app-fechou`, `metade-falhou`) abre um prazo
  (`abrir_o_prazo`, uma tarefa com o servidor fraco); se o lugar segue vazio e
  na mesma **vez** (`Maquinas::vezes`, que sobe a cada aceite e a cada saída),
  sai `MaquinaSumiu` com os segundos. Um temporizador genérico daria à metade
  de servidor um relógio para tudo, com teto e taxa a decidir; este responde à
  pergunta da spec e não acorda ninguém à toa. *Custo:* um MOD que precise de
  outro relógio na metade de servidor pede uma API nova.
- **R5. O prazo é do servidor (60 s), não do MOD.** `ServerConfig` o leva para
  as provas (2 s). *Custo:* um MOD que queira 30 s ou 5 min não escolhe; se
  precisar, um campo no `pedir` é acréscimo.
- **R6. `MaquinaVoltou`**: a principal que volta precisa perguntar quem é a
  dona, e a metade de servidor não sabia quando alguém volta (não há momento de
  «pessoa conectou»). O livro lembra os lugares de quem caiu (`ausentes`, até
  16 por pessoa) e, quando uma conexão dela negocia o 12, diz cada um que segue
  vazio, uma vez. *Custo:* nenhum conhecido; uma pessoa que volta e cujo lugar
  já foi ocupado não gera momento (a metade de servidor já sabe de quem é).
- **R7. A cópia anda pelo túnel de jogo, como TCP de verdade** (`copia.rs`): a
  dona serve o pacote em `127.0.0.1:<porta de um túnel TCP da oferta aceita>`
  (`Politica::portas_de_copia`), e a outra entra no túnel como quem joga e liga
  na porta local. Tudo o que o túnel já sabe — direto pela malha, estrela como
  reserva, quedas contadas, só a dona serve — vale para a cópia sem uma linha
  nova no túnel, no servidor ou no fio. O volume (64 MiB, inteiro na memória,
  pela estrela) não serve para um mundo de GiB. *Custo:* a cópia aparece na
  faixa de túneis de quem recebe enquanto dura; uma porta da oferta a mais
  (o MOD usa a do jogo + 10 000).
- **R8. Um código por oferta, e só pela metade de servidor.** O túnel é de
  todo mundo que tem `entrar-no-jogo`, e o mundo não: a dona sorteia 16 bytes a
  cada `oferecerCopia`, compara sem atalho, e só a metade de servidor leva o
  código à outra máquina (num aviso). *Custo:* quem tem o código e a ação lê o
  mundo enquanto a oferta vale — a metade de servidor manda só à outra máquina.
- **R9. Retomar, e conferir só o inteiro.** A fala é uma linha
  (`SEELE-COPIA/1 <codigo> <sha256> <desde>`); o que chega vai a um parcial com
  o começo do sha256 no nome, que **conta no disco**; uma ligação que cai volta
  de onde parou (até `MAX_TENTATIVAS_DA_COPIA` = 5, 30 s sem byte é queda), e o
  parcial fica para a próxima vez da mesma cópia. Uma cópia que mudou no disco
  da dona é recusada (`copia-mudou`), e uma retomada contra outra cópia também
  (`outra-copia`). Só com o sha256 anunciado o pacote vira o destino; senão,
  nada fica. *Custo:* conferir o inteiro relê o pacote todo no fim (medido
  abaixo).
- **R10. Cópias inteiras, e não só o que mudou.** A spec deixa a escolha, desde
  que o salvamento seguro venha antes. Os arquivos de região do Minecraft já são
  comprimidos e mudam aos pedaços pelo mundo inteiro; uma diferença pediria um
  índice de hashes em cada lado e uma conta de o que mudou, e um erro nela é um
  mundo misturado sem ninguém ver. A inteira se confere com um hash e retoma.
  `empacotar` usa o gzip rápido (`Compression::fast`). *Custo:* um mundo grande
  ocupa a ligação a cada cópia (medido abaixo; o intervalo é do MOD).
- **R11. `empacotar` não salva o mundo.** O salvamento seguro é da receita
  (`save-off`, `save-all flush` até «Saved the game», o pacote, `save-on`), e o
  MOD o segue. *Custo:* um MOD que empacote com o jogo escrevendo leva um mundo
  torto, e o produto não vê.
- **R12. A cópia espera a malha até 5 s** (`ESPERA_PELA_MALHA`, no app): um
  túnel começa pela estrela e só as ligações novas vão pelo direto; uma cópia
  que começa na estrela fica nela. *Custo:* até 5 s a mais antes de cada cópia
  quando não há malha.
- **R13. O que fica aberto do «sem quem hospeda»** — ver «Aberto». O núcleo não
  amarra o processo ao enlace (provado), e o servidor não decide nada parado;
  mas no app a metade de máquina ainda para quando a sessão cai (M18), e o
  túnel derruba tudo com `ServidorSaiu`, inclusive o direto.

## As provas dos guardas (revertidos um a um)

Cada guarda foi desfeito sozinho por um script (o trecho trocado, o teste
rodado, o arquivo restaurado e conferido pelo SHA-256), e o teste caiu:

| # | Guarda revertido | Teste que caiu |
|---|---|---|
| G7-C1 | o código da oferta (`iguais`) | `sem_o_codigo_ou_com_outra_copia_nada_sai` |
| G7-C2 | o sha256 do pacote inteiro antes de virar o destino | `a_copia_que_nao_bate_e_apagada_e_o_destino_fica` |
| G7-C3 | a retomada (pedir sempre do zero) | `a_ligacao_que_cai_no_meio_continua_de_onde_parou` |
| G7-C4 | só uma porta de túnel TCP aceita oferece | `a_copia_so_sai_por_uma_porta_de_tunel_aceita` |
| G7-C5 | links fora do pacote, e contados | `empacotar_nao_segue_link_nem_mora_na_origem` |
| G7-C6 | o teto de disco ao empacotar | `empacotar_para_no_teto_de_disco_e_nada_fica` |
| G7-S1 | só a dona serve (`conferir_tunel`) | `a_principal_e_a_reserva_ficam_aceitas_e_so_a_dona_serve` |
| G7-S2 | o livro chaveado pelo papel (todo papel virou principal) | o mesmo (pedir a reserva dispensou a principal) |
| G7-S3 | `mesma-pessoa-nas-duas` | `a_mesma_pessoa_nao_e_as_duas` |
| G7-S4 | o prazo só com o lugar vazio e na mesma vez | `o_prazo_so_vale_com_o_lugar_vazio_e_na_mesma_vez` |
| G7-S5 | a posse que muda para os túneis da de antes | conformância `a_passagem_avisada_nao_perde_nada` (quem jogava não viu o fim) |
| G7-S6 | o prazo aberto na saída | conformância `a_queda_de_surpresa_passa_a_posse_depois_do_prazo` |
| G7-S7 | `MaquinaVoltou` | conformância `a_principal_que_volta_com_o_processo_vivo_o_mata` |
| G7-S8 | `contexto.maquina.dona` (sempre `true`) | o mesmo (a principal ouviu que era a dona) |
| G7-A1 | `receberCopia` só pelo túnel TCP da própria oferta | app `a_copia_so_entra_pelo_tunel_tcp_da_oferta` |

No MOD (`node --test test/reserva.test.cjs`): sem cópia não assume (G7-M1),
a principal sem a posse desliga o jogo (G7-M2), a cópia agendada passa pelo
salvamento seguro (G7-M4) e a queda com reserva não desliga o servidor de jogo
(G7-M5) caíram. **G7-M3 (a passagem avisada para o jogo antes da cópia
final) passou sozinho na primeira volta**: o teste via o `stop`, mas o `stop`
vinha depois, do «desligar» da posse. O teste passou a ver se o jogo estava de
pé no momento do pacote (`vivoAoEmpacotar`), e então caiu.

O guia de tipos do prelúdio (`o_preludio_e_o_roteador_falam_os_mesmos_tipos_pela_mesma_capacidade`)
caiu sozinho com os três tipos novos, como deve, e foi atualizado para 16.

**Provado só por raciocínio:** a posse sair no despejo antes dos avisos da
mesma chamada (a ordem é a das linhas de `despejar`; a prova da passagem não
distingue as duas ordens); a espera pela malha no app (`ESPERA_PELA_MALHA`; o
comando Tauri não sobe num teste — a conformância espera do mesmo jeito e
mediu `Direto`).

## As medidas

**Conformância (`reserva.rs`, três `Enlace`s, `127.0.0.1`, prazo de 2 s):**
a cópia de um mundo de 2 MB (60 633 bytes no `.tar.gz`) pela malha
(`Direto { par }`) em 7,7 ms; sem a malha, pela `Estrela`. Na queda, a
reserva ouviu «assumir» 2,00 s depois da queda; o tempo de jogo perdido
contado foi 1,6 s (jogou-se 1,5 s depois da cópia; a diferença é o servidor ver
a queda no tempo dele). Sem o anfitrião, por três prazos: nenhum «assumir», a
dona seguiu de pé.

**O Minecraft de verdade** (`minecraft_pelo_tunel`,
`a_reserva_liga_com_o_mundo_da_principal`, 31/10, um Mac ARM; plano do MOD em
`5cfabf8`, `node ferramentas/plano-de-verdade.cjs`): Minecraft 1.21.4 na
principal, `Done` em 4,9 s; `scoreboard players set prova reserva 42`
(«Set [reserva] for prova to 42»); `save-off`, `save-all flush` e «Saved the
game» em 0,20 s; o pacote, 20 arquivos e 476 137 bytes em 336 290 bytes, em
40 ms; a cópia **pela malha** (`Direto`) em 13,2 ms, 0 retomadas; a reserva
(outra pasta, o mesmo jogo baixado) abriu o mundo e ligou o Minecraft em
4,7 s, e `scoreboard players get prova reserva` respondeu **«prova has 42
[reserva]»** — o mundo da reserva é o da principal; o status pela porta de
quem joga, pelo túnel da reserva já dona, em 18 ms, com
`"§bCasa §6pelo túnel"`.

**Não medido:** um mundo de GiB (o mundo novo do 1.21.4 tem 0,5 MB); duas
casas de verdade; o app inteiro com a metade de máquina do MOD fazendo a
reserva.

## A suíte

Só os crates tocados: `seele-core` (lib 549; `programa_copia` 8,
`programa_arquivos` 11, `programa_https` 2, `programa_processo` 12 — 1 falha
sob a carga da suíte em paralelo, `matar_a_pedido…`, que passou 3 de 3
sozinho), `seele-ffi` (249), `seele-motor`, `seele-server` (lib 718 e os
alvos de teste; `fora_do_ar` falha, como a parte 6 já mediu na base),
`seele-app` (`maquina` 39, e `frontend` o guarda do prelúdio), e as
conformâncias `reserva` (4, 3 de 3), da máquina, do túnel, da FFI, da API, do
MOD de referência, do aceite dos MODs e da ponte de pedidos.
`dois_produtores_de_recusa::outra_linha_diz_a_mesma_versao_e_o_mesmo_mais_nova`
falha: ele lê a linha do protocolo (`seele-web`, `seele-proto`), que esta
parte não toca (`git diff` vazio nos dois). `cargo xtask check-api` limpo;
`clippy --all-targets --all-features -D warnings` limpo nos crates tocados; o
`rustfmt` só nos arquivos novos e nos que já estavam limpos. No MOD: `npm test`
175, `npm run build`, `npm run check` e o QuickJS check limpos.

## Aberto

- **No app, a metade de máquina ainda para quando a sessão cai** (M18:
  `tirar_de_geracao_velha` e `sessao_saiu` → `servidor-saiu` → `aoParar` →
  `stop`). Então, no produto de hoje, «sem quem hospeda, a dona segue rodando»
  **não vale no app**: o jogo da dona é parado (salvo) quando a sessão com o
  anfitrião cai, e religado quando ele volta e a metade de servidor pede de
  novo (`MaquinaVoltou`; o app aceita sozinho a mesma oferta). A regra de uma
  dona por vez segue de pé (o lado seguro: nada muda de dona e ninguém mais
  liga), e a prova do núcleo (`sem_quem_hospeda_nada_muda_de_dona`) mostra que
  o `Programa` não depende do enlace. Falta: a metade sobreviver à troca de
  geração (marcada órfã, religada pelo pedido igual sem subir de novo), um
  `maquina.aoReconectar` para o MOD servir de novo o túnel e perguntar a
  posse, e um teto para a órfã que nunca é pedida de novo. *Custo de deixar:*
  uma queda do anfitrião derruba o jogo de todos, mesmo de quem estava na malha.
- **O túnel derruba tudo quando o enlace com o anfitrião cai**, inclusive as
  ligações diretas (`self.tuneis.derrubar_tudo(Queda::ServidorSaiu)` no
  `enlace.rs`). A spec diz «quem joga por ligação direta continua jogando»; não
  é o caso hoje, e não foi tocado nesta parte.
- **A principal que volta com o processo vivo só acontece no núcleo hoje**: no
  app o processo já foi parado com a metade (acima). O caminho do MOD
  (`perdeuAPosse`, contado em `voltouRodando`) e o do servidor
  (`MaquinaVoltou`, `nao-e-a-dona` contado) estão provados; no app ele vale
  quando a metade sobreviver à queda.
- **A cópia por diferença** (R10) e uma cópia que não releia o pacote inteiro
  para o sha256 no fim.
- **A cópia não tem teto de banda**: num mundo grande, a cópia de 5 em 5
  minutos disputa a ligação com o jogo. O MOD escolhe o intervalo (1 a 60 min).
- **O indexador** (`SEELE-MODS-INDEXER`) precisa aceitar `papel` no pedido e o
  segundo túnel na oferta descrita antes de o MOD sair.
- **A bancada da casca não mudou**: a tela do aceite mostra o segundo túnel
  como mostra o primeiro (nome, protocolo e porta), sem dizer que é a cópia;
  o título da oferta pode dizer.
- **Não medido:** duas casas de verdade (as provas estão em `127.0.0.1`, e o
  «direto» é um par WebRTC na mesma máquina); Windows e Linux (o `rename`
  sobre um pacote aberto no Windows); um mundo de GiB; o app inteiro (a
  metade de máquina do MOD de verdade com a reserva, no executor do app).
