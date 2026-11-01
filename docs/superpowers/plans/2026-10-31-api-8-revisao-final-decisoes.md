# API 8, revisão final — decisões dos consertos do lado do SEELE

Sobre `desenho/api-8-revisao` a partir de `5d053dbd`. A revisão é
`revisao-final-api8.md` (I1–I10, M1–M13); aqui ficam só os achados do lado do
SEELE (o I2, o I10, o M1, o M10, o M11 e o M12 são do MOD de multiplayer).
Spec: `docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`.
Conferido em 31/10/2026 pelo `curl` do CLAUDE.md: a mais nova é a v0.18.3, e
a v8 (protocolo 12) segue sem release.

Cada decisão no formato `Ruling: <o quê> — <por quê> — <custo se errada>`.

## Decisões

- **Ruling R1 (I1): o que roda é o que foi aceito, por caminhos presos pela
  política, sem estado.** A política prende o comando (a pasta de cima
  inteira quando ele mora numa — `java/` para `java/bin/java` —, ou o arquivo
  com e sem `.exe` quando mora na raiz) e cada argumento que é caminho
  (`server.jar`; `@arquivo` sem a arroba; o que começa com `-` não). Uma
  escrita que não traz os bytes de um artefato aceito — `baixar` pela segunda
  porta com um hash que não é o de um artefato, `escrever`, `trazer`,
  `empacotar`, `receberCopia`, `desempacotar` de um pacote cujos bytes não são
  os de um artefato — não cai sobre um preso, dentro de uma pasta presa, nem
  sobre uma pasta que contém um (por cima, cada entrada é conferida antes de
  qualquer uma entrar); a recusa é `reservado-ao-aceite`, com `caminho` e
  `reservado`. A comparação é pelo caminho real (link dentro da raiz), sem
  caixa e sem os pontos e espaços do fim. O pacote é «aceito» pelos bytes
  (tamanho + hash do mesmo algoritmo de um artefato desta máquina), e não por
  quem o baixou. **E o `rodar` só roda na raiz** (`pasta-fora-do-aceite`): os
  argumentos se leem a partir da pasta, e uma pasta livre rodaria o
  `server.jar` que o MOD escreveu nela. — Por que não «o `rodar` confere a
  proveniência»: a proveniência é estado, e um estado da memória não
  atravessa uma reabertura do app (um `server.jar` plantado na sessão 1 rodaria
  na 2); re-hashear o comando antes de `rodar` não serve ao Java aberto de um
  pacote (a tela mostra o hash do pacote, não o de `bin/java`), nem ao
  `fabric-server-launch.jar` que o instalador cria. A regra pela política vale
  na primeira escrita, em qualquer ordem e depois de reabrir. — Custo: (a) uma
  receita cujo comando mora numa pasta que também guarda dados (o mundo, os
  mods) não deixa a metade escrever ali: o runtime tem de ter a pasta dele; (b)
  um argumento aceito que é uma pasta (`--world world`) prende a pasta inteira
  contra troca; (c) o que o aceite não nomeia segue livre pela decisão Q1 — os
  `mods/*.jar` de um pack e as bibliotecas que um `@unix_args.txt` lista são
  código Java que o Java aceito carrega (é o I2, a tela do MOD); (d) o
  processo do jogo, que já é o código aceito, escreve onde quiser; (e) nomes
  curtos do Windows (`JAVA~1`) e formas Unicode diferentes de um nome não-ASCII
  só se resolvem quando o caminho já existe (o `canonicalize`); (f) a
  `pasta` de `rodar` deixa de ser uma escolha: um MOD que rodava numa
  subpasta passa a ser recusado (nenhum dos de referência roda).
- **Ruling R2 (I3): a máquina serve a porta que ela pediu, e recusa a
  concessão que traz outra.** `servindo` guarda o protocolo e a porta do
  `ServirTunel` ao lado da resposta; a concessão é registrada com eles, e uma
  com outra porta ou outro protocolo não registra nada, manda `PararTunel` ao
  servidor e responde `fora-do-aceite` a quem pediu, com as duas portas no
  log. — Por que recusar, e não só registrar a pedida: uma concessão que não
  confere é um servidor quebrado ou hostil, e servir calado esconderia isso;
  e `fora-do-aceite` já é o nome que a metade conhece para «servir o que não
  foi aceito». — Custo: um servidor que um dia reescreva a porta de propósito
  (não há caso hoje) faria a máquina parar de servir até os dois concordarem.
- **Ruling R3 (I4): o túnel que o servidor para é dito à máquina.**
  `maquinas::parar_tuneis` (a reserva que assume a posse, o aceite que muda os
  túneis, outro pedido) passa por `Tuneis::parar_pelo_servidor`, que manda
  `TunelEncerrado { MaquinaParou }` também à máquina; o `parar` que a própria
  máquina pede (`PararTunel`) segue sem eco. No cliente, esse aviso já chama
  `Maquina::parar`, que fecha as ligações diretas. — Por que não um
  `TunelRetirado` por pessoa: a retirada tira uma pessoa e deixa o túnel; aqui
  o túnel inteiro acaba, e o `MaquinaParou` é o motivo que a metade conhece.
  — Custo: a máquina que ainda é jogadora do próprio túnel também o vê acabar
  (é o que aconteceu); um `PararTunel` seguido de `ServirTunel` da máquina não
  passa por aqui, então não há eco atrasado que pare o serviço novo.
- **Ruling R4 (I5): servir de novo o mesmo protocolo e a mesma porta reusa o
  `Servido` vivo; outro para o de antes.** As ligações diretas que seguiram
  sem quem hospeda seguram o `Arc<Servido>` de antes; reusá-lo faz o `parar`
  seguinte chegar a elas e mantém as medidas num lugar só. Uma porta ou um
  protocolo novos param o de antes (`MaquinaParou`) antes de o trocar. — Custo:
  um servido de pé reusado leva junto as medidas de antes da volta (é o que se
  quer contar); um `Servido` já parado nunca é reusado.
- **Ruling R5 (I8): o teto de disco deixa de fora só os parciais vivos, e
  `:` no meio de um nome é recusado.** `Interior::parciais` guarda o parcial de
  cada download em andamento (entra ao ser criado, sai ao ser solto); a conta
  de `baixar`, `escrever`, `trazer`, `desempacotar`, `empacotar` e
  `receberCopia` pula só esses. `dentro_da_raiz` e as entradas de um pacote
  recusam `:` dentro de um pedaço (`caminho-invalido` / `pacote-invalido`): no
  NTFS, `x:s` é um fluxo alternativo que nenhuma listagem vê. — Por que não
  recusar o nome `*.seele-parcial`: recusar o nome fecha o MOD, mas não o
  parcial que um app fechado no meio deixou para trás, que também saía da
  conta; contar pelo que está vivo fecha os dois. — Custo: um parcial deixado
  por um app que caiu passa a contar no teto até alguém o apagar (o lado
  seguro); um nome de arquivo com `:` no Mac ou no Linux é recusado também.
- **Ruling R6 (I7): a conexão velha que cai de quem segue aqui por outra
  publica `Voltou` logo depois do `Parou`.** `Maquinas::voltas_de_quem_segue`
  tira de `ausentes` os lugares de quem ainda tem uma conexão no 12, e
  `maquinas::saiu` os publica; o prazo da máquina que sumiu abre como antes
  (é o mesmo par `Parou` → prazo → `Voltou` da volta comum). — Por que aqui, e
  não no `chegou`: o `chegou` da conexão nova rodou com o lugar ocupado, e
  nada mais roda depois. — Custo: enquanto a velha ainda vive (o zumbi), um
  pedido de novo volta `JaAceito` e a metade na conexão nova não é religada até
  a velha cair (o item 4 do achado) — fica, porque só o fim da velha diz que
  ela era zumbi.
- **Ruling R7 (I6): o portão da ponta conta quem o armou, e a ligação de
  outra impressão vai a quem a espera.** `PontaDaSessao` ganha um `Balcao`:
  `passar_a_atender` soma e `parar_de_atender` subtrai, e só o último fecha o
  portão; cada `atender` deixa ali uma caixa pela impressão que espera, e a
  ligação que chega com a impressão de outro que atende na mesma ponta vai à
  caixa dele, em vez de ser fechada como impostora. Vale para os túneis e para
  a tela emprestada, que passam pelo mesmo `atender`. — Por que não um laço de
  aceite só por ponta: o mesmo efeito com uma tarefa a mais para viver e
  morrer com a ponta; com a caixa, quem já espera na fila é quem despacha, e
  nada roda quando ninguém atende. — Custo: uma ligação que chega com uma
  impressão esperada por um `atender` que acabou de sair é fechada (como
  antes), e quem disca tenta de novo até o prazo dele; um `passar_a_atender`
  sem o seu `parar_de_atender` (só os testes fazem) deixa a ponta armada.
- **Ruling R8 (I9): no Unix, o filho de `rodar` nasce num grupo de processos
  próprio, e parar mata o grupo; no Windows, só o filho, por ora.**
  `process_group(0)` no `spawn`, e um `Grupo` que, ao ser solto, manda
  `SIGKILL` ao grupo (`killpg`, pelo `nix` que já estava na árvore — o
  workspace proíbe `unsafe`): na ordem de matar (DESLIGAR, REVOGAR, app
  fechando, `encerrar_tudo`, `Programa` solto) antes de colher o filho, e
  também quando o filho sai sozinho (o ajudante que ele deixou sai junto). Medido
  no Mac com um filho que sobe um neto: `matar`, `encerrar_tudo`, soltar o
  `Programa` e o filho que sai sozinho param o neto
  (`parar_mata_os_netos_por_todos_os_caminhos`). — Por que não o Job Object
  no Windows agora: pede o `windows-sys` com `Win32_System_JobObjects` e
  `unsafe` numa exceção de lint, e nada aqui o roda — «existir não é
  funcionar»; fica para quem puder medir no Windows. — Custo: (a) no Windows,
  um neto segue vivo depois de DESLIGAR (o achado inteiro, lá); (b) o app que
  cai sem soltar o `Programa` deixa o grupo vivo nos dois sistemas (só um
  vigia de fora, ou o `PR_SET_PDEATHSIG` do Linux, fecharia isso); (c) a
  varredura depois de o filho sair sozinho usa o número do grupo já colhido:
  outro processo só o herdaria se o sistema desse a volta nos números de
  processo nesse intervalo de microssegundos; (d) um processo que o servidor
  de jogo queira deixar de pé de propósito morre com ele.
- **Ruling R8b (a régua do batimento dos testes): ler de novo um arquivo
  vazio.** O programa de teste reescreve o batimento com `fs::write` (trunca e
  escreve); uma leitura no meio via `0` e dava o processo por morto. Visto
  uma vez em `matar_a_pedido_para_o_processo_e_diz_que_foi_a_pedido` (passou
  nas três seguintes), e sempre no laço novo do I9. — Custo: nenhum no produto.
- **Ruling R9 (M3): as ligações da cópia moram num `JoinSet` da escuta, e a
  oferta confere `encerrado` de novo depois de guardar a escuta.** Parar a
  escuta solta o conjunto e corta cada transferência; uma oferta que corria
  durante o `encerrar_tudo` se desfaz e volta `encerrado`. — Custo: uma
  oferta nova (a mesma porta) também corta quem estava baixando a de antes —
  que já não seria a cópia anunciada.
- **Ruling R10 (M5): o diretório central de um `.zip` tem de caber no
  arquivo**, como em `seele_proto::pack`, antes de alocar. — Custo: nenhum
  pacote válido tem o diretório além do fim.
- **Ruling R11 (M6): `guardar` e `revogar` dizem no log quantas linhas
  ilegíveis a regravação tira** (`WARN`, com o arquivo e o caminho). Não
  mudei a assinatura (a ffi e a casca seguem iguais). — Custo: a contagem
  fica no `seele.log`, e não na tela.
- **Ruling R12 (M2): cada porta da tela do aceite vem com o nome do túnel**
  («o túnel «mc-copia» escuta na porta 35565…»), e não «o jogo escuta». O
  produto não sabe sozinho qual túnel é de cópia (`portas_de_copia` são todas
  as TCP da oferta), mas o MOD o nomeia, e o nome é o que a pessoa compara.
  — Custo: um MOD que nomeie mal o túnel engana a tela do mesmo jeito; marcar
  a cópia na oferta pediria um campo novo no fio.
- **Ruling R13 (M9): a falha de `maquinas_aceitas` aparece na lista**
  (`#maquinas-aceitas-erro`, em frase), além do registro. Bancada
  `aceite-da-maquina.cjs`, caso 17, nos dois motores. — Custo: a lista de
  antes fica desenhada embaixo da frase até a próxima leitura dar certo.
- **Ruling R14 (M7): o MOD desligado leva do livro as vezes e os ausentes
  dele, e o livro lembra no máximo `MAX_AUSENTES_LEMBRADOS` (4096) pessoas
  ausentes** — passado o teto, o pedido a quem não está não é lembrado, e o
  `WARN` diz. `donas` já saía com o MOD; segue crescendo por nome de
  `servidor` enquanto o MOD está ligado (limitado pelo que a metade de
  servidor declara). — Custo: a 4097ª pessoa chamada sem estar aqui não ouve
  `MaquinaVoltou` ao chegar (a metade de servidor pode chamá-la de novo);
  recusar um `quem` que não é membro ficou de fora (pede o livro de membros
  dentro deste, e um membro novo ainda não gravado seria recusado).
- **Ruling R15 (M13b): o teste das dispensas lê numa tarefa só, e o prazo
  fica na fila.** Um `timeout` em volta de `frame::read` cancelava a leitura
  no meio de um quadro partido, e o resto vinha desalinhado. — Prova por
  reversão: não há (o defeito é uma corrida rara do teste, não do produto).
  O `esperar` do mesmo arquivo tem o mesmo desenho (300 ms) e ficou: os
  quadros dele são pequenos e chegam inteiros numa mensagem. Custo: se um dia
  não chegarem, a mesma falha falsa volta lá.

## O que ficou, com o porquê

- **Ruling R16 (M4): os canais `j` fora do crédito da conexão ficam** —
  PLAUSÍVEL, e o conserto (recusar `t`/`j` sem opt-in, ou contá-los no
  crédito) mexe no `seele-enlace`, que serve a tela e a voz; sem medir o
  teto real que uma ponta segura, um conserto daqui seria palpite. — Custo:
  um par hostil pode prender ~250 MiB num jogador ou num par de tela.
- **Ruling R17 (M8): o pedido cujo destinatário saiu fica calado no livro.**
  Dizer `MaquinaRecusou` com o pedido ainda de pé contradiz o nome; trocar o
  pendente por um ausente (e o `MaquinaVoltou` na chegada) muda o contrato
  que a metade de servidor de referência segue, e pede o MOD junto. — Custo:
  o pendente conta no `MAX_PEDIDOS_PENDENTES_POR_MOD` até a pessoa voltar ou
  o MOD pedir de novo.
- **Ruling R18 (M13a): os guardas de texto de `tests/frontend.rs` ficam.**
  O M9 e o M2 ganharam caso na bancada `aceite-da-maquina.cjs`, que roda o
  produto nos dois motores; mover os outros guardas para as bancadas é
  trabalho de cada tela, fora desta revisão. — Custo: um guarda de texto pode
  passar com o defeito presente, como o M9 passava.
- **Fora do SEELE:** I2, I10, M1, M10, M11 e M12 são do MOD de multiplayer;
  e o contrato que o MOD copia (`test/contrato/api-v8.json`) precisa ser
  copiado de novo: a v8 ganhou `reservado-ao-aceite`, `pasta-fora-do-aceite`
  e os campos `caminho`, `reservado` e `pasta`, e o `rodar` só roda na raiz.

## Revisão do release 0.19 + API 8 (31/10)

Relatório: I1 e M1–M7. As provas por reversão abaixo foram rodadas: cada
teste reprova com o conserto desfeito e passa com ele.

- **Ruling R19 (I1): nenhuma metade sobe enquanto um jogo que sobrou ainda
  salva — e a espera vale para toda metade, não só para a da mesma chave.**
  `PortaDasSobras` (`maquina.rs`) é fechada no `setup`, antes de a thread da
  procura existir; a thread acha as sobras (`achar_sobras`, núcleo), para
  cada uma (`parar_sobra`) e abre a porta quando a última sai (e também se
  cair no meio). `processar_pedido` e `aceitar` esperam a porta **antes** da
  `vez`, para um pedido não prender os outros no cadeado. Esperar, e não
  recusar: os motivos de recusa são uma lista fechada do protocolo, e o
  servidor não tem prazo para o pendente. A espera vale para todas as chaves
  porque a porta do jogo é da máquina inteira, e o mesmo servidor alcançado
  por outro endereço é outra chave. A faixa diz `parando` («o SEELE pediu ao
  jogo para salvar e sair e espera ele sair; até lá, nenhum jogo sobe neste
  computador»), e o desfecho toma o lugar dessa linha. **O MOD não ouve nada
  além da espera**: o aceite dele chega quando o órfão sai. Dizer mais pede
  um momento novo na v8. — Prova: `nenhuma_metade_sobe_enquanto_o_jogo_que_sobrou_salva`
  (o pedido já está na porta antes de a procura ler a pasta, e o órfão leva
  1 s para sair depois do `SIGTERM`). Sem a espera ele reprova sempre, porque
  o órfão ainda nem recebeu o sinal. A bancada `aceite-da-maquina.cjs`, caso
  20, prova a linha `parando` e a troca dela pelo desfecho. — Custo: na
  abertura, uma metade de outro jogo também espera até ~22 s por sobra. A
  ligação entre a porta e as duas chamadas tem uma linha em cada lugar, e
  nenhum teste a liga ao `processar_pedido`.
- **Ruling R20 (M1): o sinal que chega durante a despedida da janela não sai
  por cima dela.** `despedir_no_sinal` volta sem chamar o `exit`: o `exit`
  vem da despedida que já corre. — Prova:
  `o_sinal_durante_a_despedida_da_janela_nao_sai_por_cima`.
- **Ruling R21 (M2): sem o começo do dono, um pid do dono que existe vale
  como dono vivo.** O registro fica, e o log diz (`warn`). O registro sem o
  começo do próprio jogo também passa a ser dito em `warn`, ao gravar e ao
  apagar. Errar para este lado custa um órfão, que a próxima abertura para;
  errar para o outro derruba o jogo de quem está jogando. — Prova:
  `sem_o_comeco_do_dono_um_dono_vivo_nao_perde_o_jogo`.
- **Ruling R22 (M3): o registro é gravado antes de `rodar` voltar**, e é
  apagado nos dois retornos de erro que vêm depois. Ainda há uma janela: o
  tempo de um `ps` entre o `spawn` e o arquivo, porque o começo do processo
  só existe depois que ele existe. — Prova: `o_registro_existe_quando_rodar_volta`.
- **Ruling R23 (M4): «que não parou» diz «pelo gerenciador de processos do
  sistema (no Mac, o Monitor de Atividade)».** — Prova: bancada
  `aceite-da-maquina.cjs`, caso 20.
- **Ruling R24 (M5): a volta sozinha só acontece se a sessão que acabou era
  a do servidor desta janela.** O alvo dessa sessão tem de ser igual a
  `meu_servidor().alvo`. — Prova: bancada `volta-depois-de-salvar-o-conjunto.cjs`,
  caso 4.
- **Ruling R25 (M6): a janela que fecha com a despedida de um sinal correndo
  não despede de novo.** O `prevent_exit` continua na linha de sempre, e
  `ao_fechar_a_ultima_janela` faz o `swap`. — Prova:
  `a_janela_que_fecha_durante_a_despedida_nao_despede_de_novo`.
- **Ruling R26 (M7): o logoff do Windows fica como custo aceito da W3,
  documentado.** Ele não é medido.
  - **O que o relatório supunha**: que o logoff mata o app sem caminho
    nenhum. Lendo o `tao` 0.35.3 (`platform_impl/windows/event_loop.rs`,
    `WM_ENDSESSION`), esse caminho existe: o `loop_destroyed` gera o
    `RunEvent::Exit`, e daí `despedir_no_fim_do_app` chama
    `parar_todas_na_saida`, que dá o `aoParar("app-fechou")` com o `stop`.
  - **Por que não basta**: tudo isso corre dentro do teto inegociável de 1 s
    (`TETO_DA_DESPEDIDA_NO_FIM`). Quando a mensagem volta, o Windows encerra
    o processo, e o Job mata o `java`. Um mundo que leva mais de 1 s para
    salvar perde o que mudou desde o último autosave.
  - **Por que não foi além**: estender esse tempo pede
    `ShutdownBlockReasonCreate` e responder ao `WM_QUERYENDSESSION`, que o
    `tao` não trata. Seria código Windows `unsafe` novo numa janela que não é
    nossa, e não é simples.
  - **Por que não foi medido**: medir pede fazer logoff da sessão do dono no
    `winpc`, e isso é mexer no sistema.
  - **Os registros no Windows**: só são gravados e apagados, sem parar nada
    (`como_esta` devolve `None`). Ficam como estão, porque a W3 cobre a queda.
- **Ruling R27 (teste instável): `a_volta_religa_a_orfa_sem_subir_de_novo`
  tinha um defeito de tempo no teste, e o produto estava certo.** O `fora`
  do `aoReconectar` é o tempo de órfã em segundos inteiros. Sob carga, as
  esperas pelo batimento passam de 1 s, e «voltei 1» é a resposta certa.
  Nenhum pedido se perdeu: o quadro chegou uma vez, e pela conexão nova.
  - **Prova do diagnóstico**: com 1,1 s a mais entre orfanar e religar, o
    teste antigo reprova com a mesma mensagem da suíte (`«pedir "voltei 0"»
    não chegou ao fio; chegou ["pedir \"voltei 1\""]`), e o novo passa.
  - **O conserto**: o teste espera qualquer `voltei N`, exige que seja um
    quadro só, e confere `N` contra o relógio do teste.
  - **Medida**: 30 de 30 rodadas passaram com 30 `yes` ocupando 15 núcleos.
