# API 8 — sem quem hospeda: decisões, provas e medidas

Sobre `desenho/api-8-sobre-0.19`, depois da fusão da parte 7 (b3ef3a24). Spec:
`docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`, «A
reserva», último item: sem quem hospeda o SEELE, nada muda de dona, **a dona
atual continua rodando**, quem joga pela malha continua jogando, quem passava
pela estrela cai e a tela diz por quê, e quando ele volta a dona é a mesma. A
parte 7 deixou isso em aberto (R13 e «Aberto» de
`2026-10-31-api-8-reserva-decisoes.md`). Cada decisão diz o custo, se estiver
errada. O MOD de multiplayer tem as dele em `DECISOES.md`.

Conferido em 31/10/2026 no release (`curl` do CLAUDE.md): a mais nova é a
v0.18.3. Nem o protocolo 12 nem a API 8 saíram; a v8 segue emendável.

## Os nomes que saíram

- `maquina.aoReconectar(fn({ fora }))` (prelúdio; `api/v8.json`,
  «metade-de-maquina»): a metade seguiu rodando sem quem hospeda e foi religada
  pelo pedido igual ao aceito; `fora` são os segundos sem ele.
- `aoParar('sem-volta')` (app, `Fim::SemVolta`): a órfã que ninguém chamou de
  volta em `ESPERA_PELA_VOLTA` (10 min).
- `maquina-estado` ligada com `motivo: 'sem-anfitriao'`: a faixa diz, em
  laranja, que a máquina segue sem quem hospeda.
- `Queda::AnfitriaoSaiu` (núcleo; `anfitriao-saiu` na FFI e na faixa de túneis):
  «quem hospeda o SEELE saiu; quem passava por ele caiu».
- `MaquinaVoltou` também para o lugar pedido a quem não estava
  (`Maquinas::lembrar_quem_nao_esta`).
- **Nenhuma variante no fio.** `seele-proto`, `seele-web` e os bytes-ouro não
  mudaram.

## Decisões

- **S1. M18 revista: a metade de máquina sobrevive à queda do enlace.** Antes,
  a geração nova do enlace (`tirar_de_geracao_velha`) e o fim da sessão
  (`sessao_saiu`) paravam as metades com `servidor-saiu`, e o `aoParar` delas
  desligava o jogo. Agora a geração nova as marca **órfãs** (`Ligada::orfa`): o
  executor, os processos e os túneis seguem, e a faixa diz `sem-anfitriao`. O
  fim da sessão por **quem hospeda indo embora** — `LinkLost` (a bateria
  descarregou), `ServerShuttingDown`, `ScheduledMaintenance` — também as deixa
  órfãs; os outros fins (expulso, banido, recusado) e a saída pedida pela pessoa
  as param como antes. *Custo se errada:* uma máquina cuja sessão caiu segue
  gastando CPU e disco até 10 min sem ninguém poder jogar pela estrela; e uma
  principal que caiu sozinha (quem hospeda seguiu) roda junto com a reserva que
  assumiu até voltar e ver que perdeu a posse — o caso que a spec manda contar,
  e que o MOD conta.
- **S2. Religar é o pedido igual ao aceito** (`Maquinas::religar_se_orfa`):
  mesma chave, mesmo resumo, mesmo pacote, de qualquer sessão do mesmo alvo. A
  metade é a mesma instância — o fio dela passa a ser a conexão nova
  (`Viva::trocar_fio`) —, o `AceiteDeMaquina` sai de novo, e só depois o
  `aoReconectar`. Um pedido com outro resumo segue o caminho de sempre (a tela,
  ou o aceite sozinho), e ao ligar a nova a órfã para. *Custo:* nenhum
  conhecido; uma oferta que mudou enquanto quem hospeda estava fora para o jogo
  e sobe outro, como mudaria com ele presente.
- **S3. O nome é `maquina.aoReconectar`**, no idioma de `aoAviso` e `aoParar`
  (um ouvinte, o último vence), com `{ fora }`. O app não serve o túnel nem
  pergunta a posse por conta própria: a metade sabe o que servia e o que o
  plano dela diz. *Custo:* um MOD que não registre o ouvinte fica religado sem
  túnel até o próximo aviso dele; o guia mostra o padrão.
- **S4. A órfã que ninguém chama em 10 min para** (`ESPERA_PELA_VOLTA`), com
  `aoParar('sem-volta')`, sem `MaquinaParada` (quem hospeda já não a tem no
  livro), e a faixa em vermelho diz «quem hospeda o SEELE não chamou esta
  máquina de volta em 10 minutos; o jogo foi salvo e parado». O prazo é da vez
  da orfandade (`Orfa::vez`): o de uma orfandade velha não para a de agora.
  Dez minutos cobrem os cinco da bateria e um computador que reinicia. *Custo:*
  um anfitrião que leva mais de 10 min encontra o jogo salvo e parado, e a
  metade de servidor o liga de novo pelo caminho de sempre.
- **S5. Sair da sessão não para as órfãs** (`sessao_saiu` e
  `sessao_saiu_sem_tela` pulam `orfa`): dispensar a placa de uma sessão que
  quem hospeda encerrou não é pedir para desligar o jogo. DESLIGAR na faixa
  para, e fechar o app para (`app-fechou`). *Custo:* quem quer parar tudo ao
  sair precisa apertar DESLIGAR.
- **S6. No núcleo, a queda da conexão não derruba os túneis**
  (`Tuneis::sem_anfitriao` no lugar de `derrubar_tudo(ServidorSaiu)` em
  `soltar_a_conexao`). Cada túnel de quem joga decide no próprio laço
  (`Evento::EstrelaCaiu`): pela estrela acaba com `AnfitriaoSaiu`; pela ligação
  direta segue, as conexões vivas e as novas pelo túnel já concedido. Se o
  direto cair enquanto quem hospeda está fora, acaba com `AnfitriaoSaiu` (não há
  estrela para onde voltar). A máquina não aborta as ligações diretas
  (`Maquina::sem_anfitriao`); a estrela dela acaba sozinha. *Custo:* o relatório
  do túnel direto não diz «sem quem hospeda» (a faixa da sessão diz que a
  conexão caiu); um campo no relatório seria acréscimo na FFI.
- **S7. Nada novo abre sem quem hospeda.** Um túnel novo precisa de
  `TunelConcedido` (quem joga) e de `SirvaTunelPara` (a máquina), que são dele e
  conferem a ação `entrar-no-jogo`; a ligação direta só aceita canais dos túneis
  já apresentados àquela pessoa (`Permitidos`). O que segue é o que ele já tinha
  concedido, e **conexões novas nesse túnel** (o Minecraft abre uma por
  entrada). *Custo:* quem perdeu a permissão enquanto quem hospeda estava fora
  segue jogando pela malha até ele voltar (S8 fecha isso na volta).
- **S8. Na volta, tudo é conferido de novo.** Quem joga pede entrada de novo
  sozinho para cada túnel que seguiu (`Tuneis::reentrar`, depois de
  `on_reconnected`): a concessão troca a estrela do túnel e mantém a porta
  (`Evento::NovaEstrela`); uma recusa porque a máquina ainda não serve
  (`NaoExiste`, `MaquinaSaiu`) é pedida de novo a cada 1 s dentro de 30 s; outra
  recusa acaba o túnel com o motivo. A máquina, a partir da volta, dá 30 s
  (`PRAZO_DA_REAPRESENTACAO`) para quem hospeda apresentar de novo cada túnel a
  cada pessoa ligada pela malha; o que ele não apresentar sai da ligação direta
  como uma retirada (`SemPermissao` nos canais). *Custo:* uma máquina que leva
  mais de 30 s para ser chamada de volta derruba quem jogava pela malha (que
  entra de novo pelo ENTRAR); quem hospeda que voltou e não concede mais
  (permissão perdida) corta quem estava na malha.
- **S9. Quem hospeda fechando o SEELE se despede de todos.** O
  `ServerShuttingDown` acaba as sessões (como `quem_hospeda_fecha.rs` exige), e
  com elas os túneis, os diretos inclusive: o enlace termina e leva o motor dos
  túneis junto. A metade de máquina segue órfã (S1). Só a queda (o anfitrião
  que cai, dorme, perde a rede) mantém quem está na malha jogando, e só
  enquanto a bateria do enlace corre (5 min). *Custo:* um anfitrião que fecha o
  app de propósito corta o jogo de quem estava na malha; manter o túnel além do
  enlace pediria um motor de túneis fora da sessão.
- **S10. Chamar quem não está é lembrado** (`lembrar_quem_nao_esta`, no ramo
  `nao-esta-aqui` do despejo): o lugar entra em `ausentes`, e a chegada da
  pessoa dá `MaquinaVoltou` dele, uma vez. É o que faz a volta de quem hospeda
  fechar sem corrida: o livro nasce vazio, a metade de servidor chama quem
  guardou, e quem ainda não reconectou é chamado quando reconectar. *Custo:*
  um `MaquinaVoltou` de um lugar que a metade de servidor pediu e esqueceu; ela
  decide se pede de novo, como em toda volta.
- **S11. A metade de servidor é quem chama de novo depois de um servidor que
  reinicia.** O livro e a posse moram na memória (R3); nada no SEELE lembra
  quem operava. A prova chama pelo pedido `{ religar }` de quem opera; o MOD de
  multiplayer ainda não chama sozinho ao subir (ver «Aberto»). *Custo:* sem a
  chamada, a órfã para em 10 min (S4) e a metade de servidor liga de novo pelo
  caminho de sempre, com o jogo subindo outra vez.

- **S12. O servidor que sobe diz à metade de servidor que subiu** (`MetadeSubiu`;
  e2e da API 8, 31/10, passo 16, que fecha a S11). O despachante, ao recarregar
  o conjunto (`despacho::dizer_que_subiu`), manda pelo barramento
  `MudancaDeMaquina::MetadeSubiu { motivo, maquinas }` a cada MOD que declara
  `maquina` e cujo código é novo desde a última recarga daquele laço: todos na
  primeira (`servidor-subiu`), e depois só quem entrou no conjunto ou mudou de
  revisão (`codigo-novo`). `maquinas` é `Maquinas::lugares_do_mod` naquele
  instante (aceitas e pendentes) — vazio num servidor que reiniciou. A metade de
  servidor chama de novo quem o quintal diz que rodava, com o pedido igual ao
  aceito (S2 religa a órfã), e quem ainda não reconectou volta pelo S10.
  Acréscimo na v8 (momento novo, nada renomeado; a v8 segue sem release,
  conferido no `curl` do CLAUDE.md em 31/10). Vai pelo barramento, e não por uma
  chamada direta, para o quintal, os avisos e o que ela declarar em
  `mundo.maquinas` seguirem o caminho de todo momento; e só ao MOD de máquina,
  para um MOD de API anterior não ouvir um nome que o contrato dele não tem.
  *Custo se errada:* um conjunto salvo que só troca outro MOD não repete o
  momento (o hash do de máquina não mudou); um MOD de máquina que volte a ser
  ligado depois de desligado ouve `codigo-novo`, e não `servidor-subiu`.

## As provas dos guardas (revertidos um a um)

Cada guarda foi desfeito sozinho por um script
(`scratchpad/m/reverter/reverter.py`: o trecho trocado, o teste rodado, o
arquivo restaurado e conferido pelo SHA-256), e o teste caiu:

| # | Guarda revertido | Teste que caiu |
|---|---|---|
| GH-1 | a estrela acaba com `AnfitriaoSaiu` (virou `ServidorSaiu`) | conformância `sem_quem_hospeda_quem_passa_pela_estrela_cai_e_sabe_por_que` |
| GH-2 | o túnel direto segue sem a estrela (acabava sempre) | `sem_quem_hospeda_a_dona_segue_e_quem_joga_pela_malha_continua` |
| GH-3 | a máquina segue servindo pela malha (`abort_all` na queda) | o mesmo («o túnel do jogo acabou: AnfitriaoSaiu») |
| GH-4 | a queda não derruba tudo (de volta a `derrubar_tudo(ServidorSaiu)`) | o mesmo |
| GH-5 | na volta, quem joga pede entrada de novo | `quem_hospeda_volta_e_a_dona_e_a_mesma_religada` |
| GH-6 | pede de novo enquanto a máquina ainda não serve | o mesmo |
| GH-7 | na volta, só segue o que foi apresentado de novo | núcleo `na_volta_de_quem_hospeda_so_segue_o_que_ele_apresentou_de_novo` |
| GS-1 | o despejo lembra o pedido a quem não está | conformância `o_pedido_a_quem_nao_esta_volta_quando_ela_chega` |
| GS-2 | o livro guarda o lugar | servidor `o_pedido_a_quem_nao_esta_volta_quando_ela_chega` |
| GU-1 | a frase de `anfitriao-saiu` | `tunel_cada_recusa_e_cada_queda_do_rust_tem_frase` |
| GA-1 | a queda deixa órfã, e não para (M18 de volta) | app `a_volta_religa_a_orfa_sem_subir_de_novo` |
| GA-2 | o pedido igual religa a órfã | o mesmo |
| GA-3 | a religada fala pela conexão nova | o mesmo |
| GA-4 | o prazo é o da orfandade de agora | `a_orfa_que_ninguem_chama_de_volta_para_com_o_motivo` |
| GA-5 | sair da sessão não para as órfãs | `sair_da_sessao_nao_para_as_orfas` |
| GA-6 | o prelúdio entrega o `aoReconectar` | `a_principal_que_volta_sem_a_posse_mata_o_jogo_e_diz` |
| GS-3 | o despachante diz `MetadeSubiu` ao subir (o envio desligado; a asserção da primeira subida também tirada, para cair na parte do reinício) | conformância `o_servidor_que_reinicia_chama_as_maquinas_sozinho` («o servidor subiu e não chamou a dona de volta: nenhum PedidoDeMaquina de mundo-1 chegou») |

No MOD (`node --test test/reserva.test.cjs`): a religada serve de novo o túnel
(GM-1), a religada que esperava o aceite termina no aviso (GM-2), o
`aoReconectar` é ouvido (GM-3) e a metade de servidor guarda a religada (GM-4)
caíram, todos em «sem quem hospeda a dona segue de pé…». O teste «a principal
religada que perdeu a posse… desliga o jogo» **passa também sem o
`aoReconectar`**: o aviso `preparar` que o `MaquinaAceitou` manda à que não é a
dona já pergunta o plano e desliga (o caminho da parte 7). Ele prova o
comportamento, não o guarda novo.

**Provado só por raciocínio:** o braço do `Event::Ended` que escolhe órfã ou
parada (`sem_quem_hospeda`) e o relógio da órfã em thread (`orfanar_e_vigiar`)
precisam de um `AppHandle` — o que eles chamam (`orfanar`, `tirar_orfa_vencida`,
`encerrar(SemVolta)`) está provado; a faixa laranja e a frase de `sem-volta`
não foram retratadas.

## As medidas

Conformância (`reserva.rs`, três `Enlace`s em `127.0.0.1`, prazo de 2 s), 31/10,
um Mac ARM:

- **Sem quem hospeda, pela malha:** depois de `Daemon::shutdown`, a conexão TCP
  do jogo aberta pela ligação direta ecoou de novo, uma conexão nova pela mesma
  porta local também, por 2 × o prazo; a dona seguiu de pé e a reserva não
  ligou.
- **Pela estrela:** o túnel de quem joga acabou com `AnfitriaoSaiu`.
- **A volta:** o servidor subiu de novo no mesmo endereço e banco; os enlaces
  voltaram pela bateria; a reserva (dona antes da queda) seguiu dona, serviu de
  novo o túnel e ouviu `dona: true`, com `partidas = 1` — o jogo subiu uma vez
  só; a principal ouviu `dona: false`; quem joga pediu entrada de novo sozinho
  (concedido sem clique) e manteve a mesma porta, com a conexão pela malha
  ecoando o tempo todo. O teste leva ~7 s.

**Não medido:** duas casas de verdade (o «direto» é um par na mesma máquina);
o app inteiro com a faixa laranja; a órfã de 10 min de verdade; Windows.

## Aberto

- ~~O MOD de multiplayer não chama as máquinas sozinho quando o servidor SEELE
  reinicia (S11).~~ Fechado pela S12 (`MetadeSubiu`) e pelo MOD (`DECISOES.md`,
  «O servidor SEELE que reinicia»).
- O relatório do túnel direto não diz «sem quem hospeda» (S6).
- Quem hospeda fechando o app corta quem estava na malha (S9).
- A faixa laranja da órfã e a frase de `sem-volta` não foram retratadas nem
  medidas no app inteiro.
