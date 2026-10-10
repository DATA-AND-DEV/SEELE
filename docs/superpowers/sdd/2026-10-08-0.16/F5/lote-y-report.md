# Lote Y da F5: os intermitentes — relatório

Ramo `0.16/lotes-y`, a partir de `d9124cea`. Worktree:
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-aa13fd41758429571`.

| commit | o quê |
|---|---|
| `6061e38c` | parte 1: o socket de quem conecta não divide a porta com um socket IPv4 (`seele-core`) |
| `de04ab26` | parte 2: o `par_lento` espera por sinais do servidor e afirma o que ele promete (salvo pelo controlador como «em curso»; o código é o que foi medido abaixo) |
| `ffbcc3fc` | parte 2: o acréscimo em `docs/pendencias.md` |
| `2bb78972` | o relatório, primeira versão |
| `d7b969af` | parte 1: a reprodução roda e mede nos três sistemas, com teto (a preocupação bloqueante do controlador) |
| `99142c72` | parte 1: o conserto também cobre o IPv4 no curinga `0.0.0.0:X`, que `6061e38c` deixava passar |
| `06d0a208` | o relatório com a seção «v0.15.1», Linux e Windows |
| `5b167586` … `866718cc` | rodada 2: I1, I2, M4, M3, M5, M1, M2 (ver «Rodada 2») |
| este | o relatório com a rodada 2 |

Nada fora de `crates/seele-core`, `crates/seele-server/tests/par_lento.rs` e
`docs/`. Nenhum gancho em `crates/seele-server/src`. O `seele-sessao` não foi
tocado: a medida não apontou o assento. Nenhuma conformidade rodou.

---

## Parte 1 — `a_volta_avisa_a_voice_room_em_que_o_motor_sentou_a_pessoa_de_novo`

### A medida

Instrumentação temporária (revertida, `cmp` contra o original): linhas com
tempo em `connect_por` (antes do QUIC, depois do aperto do QUIC, do `open_bi`
e do aperto do protocolo), em `tentar` (o resultado do `connect_por`, o do
`restabelecer`, o `Reconectado`), em cada passo do `servidor_que_conta_os_hellos`
(o `Incoming` com o endereço de quem chega, o aperto, o `accept_bi`, o
`Hello`, o `Session`); um fio de batimento e uma tarefa de batimento no
teste, que anotam qualquer sono maior que 500 ms; e um vigia que roda `lsof`
na porta do cliente quando o aperto do QUIC passa de 3 s.

**Onde os 20 s foram.** No aperto do QUIC: `connect_with().await` sai com
`TimedOut` em **20,004 s** (a `IDLE_TIMEOUT`). O aperto do protocolo e o
`restabelecer` — o resgate do assento — nem chegam a rodar.

**O que o servidor de teste fazia.** Recebeu o `Incoming` do cliente e ficou no
`entrada.await` daquela conexão: a resposta dele ao aperto não chegava ao
cliente.

**Por quê, medido numa rodada natural** (sem nada meu rodando, a 9ª de 20 do
`--lib` serial com o codec, num vizinho que usa o mesmo auxiliar,
`a_volta_da_bateria_onde_o_pino_nao_prova_o_servidor_e_recusada_antes_do_hello`):

```
connect_por: inicio server=127.0.0.1:50016 local=Ok([::]:63456)
servidor 127.0.0.1:50016: Incoming de 127.0.0.1:63456
connect_por: aperto QUIC parado há 3 s; quem segura a porta 63456:
core-medi 53407  20u  IPv4  UDP 127.0.0.1:63456
core-medi 53407  27u  IPv6  UDP *:63456
connect_por: aperto QUIC ERRO em 20.004126459s: TimedOut
```

O socket do cliente é `*:63456`, de pilha dupla. O **mesmo processo** segura
`127.0.0.1:63456` em IPv4: é o socket do servidor de
`a_reentrada_na_sala_falha_na_reconexao_e_o_cliente_nao_se_da_por_sentado`, que
`servidor_que_aperta_a_mao_e_cai` esquece de pé de propósito
(`std::mem::forget(escuta)`), 31 ms antes. A resposta endereçada a
`127.0.0.1:63456` vai ao mais específico dos dois, o IPv4, que ninguém lê.

**A causa, medida no macOS fora do SEELE** (Python, sockets UDP):

| experimento | resultado |
|---|---|
| 4 000 `[::]:0` de pilha dupla, depois 4 000 `127.0.0.1:0` | 0 portas repetidas |
| 4 000 `127.0.0.1:0`, depois 4 000 `[::]:0` de pilha dupla | **992** portas repetidas (o esperado de sorteio independente é ~977) |
| 3 000 no endereço da rede de casa, depois `[::]:0` | **525** repetidas |
| para onde vai a resposta a `127.0.0.1:X` (ou `192.168.x.y:X`) com os dois de pé | ao socket IPv4, nos dois casos |
| ligação **explícita** a `[::]:X` com um IPv4 segurando `X` num endereço | `EADDRINUSE` (127.0.0.1 e rede de casa) |
| 3 000 IPv4 no **curinga** `0.0.0.0:0`, depois `[::]:0` | **535** repetidas; a resposta a `127.0.0.1:X` vai ao IPv4 curinga, 20 de 20 |
| ligação **explícita** a `[::]:X` com um IPv4 no curinga segurando `X` | **aceita**, 20 de 20, e a resposta vai ao IPv4 curinga |
| 3 000 IPv4 **conectados** (como o QUIC de um navegador) no endereço da rede de casa, depois `[::]:0` | 503 repetidas, mas a resposta de outro remetente vai à pilha dupla, 20 de 20: conectado **não rouba** |
| ligação explícita de uma sonda `0.0.0.0:X` com `X` de um IPv4 em `127.0.0.1`, na rede de casa, no curinga, ou de um `[::]:X` | `EADDRINUSE` nos quatro |
| curinga IPv4 `0.0.0.0:0` depois de IPv4 específicos, ou de 3 000 `[::]:0` | 0 repetidas |
| com `[::]:X` nosso de pé, ligar `127.0.0.1:X`, o da rede de casa ou `0.0.0.0:X` | `EADDRINUSE` |

A escolha de porta de `[::]:0` confere só a família IPv6. A ligação explícita
a `[::]:X` recusa o IPv4 ligado a um endereço, mas não o do curinga; a sonda
IPv4 explícita em `0.0.0.0:X` recusa os dois.

**Descartadas pela medida:**
- conexão estranha no servidor de teste: 310 `Incoming` em 30 rodadas, todos
  do próprio cliente;
- tarefas do `quinn` ou o processo parados: os dois batimentos nunca dormiram
  mais de 500 ms em 30 rodadas;
- defeito no resgate do assento: o `tentar` falha antes dele.

**Também induzido** (para confirmar o mecanismo, com um processo segurando 3 000
portas em `127.0.0.1`): o `lsof` mostra o Python com `127.0.0.1:X` e o teste
com `*:X` em cada aperto parado, e `a_volta_avisa_…` reprova com a assinatura
de F1 (`TimedOut`, 20,00 s, `None`).

**Frequência natural medida:** 1 rodada em 20 (a 9ª, acima) com o binário
instrumentado e sem nada de fora; antes, 1 em 30 (a 24ª, `a_recusa_pela_impressao_…`,
20 s, durante as minhas ondas de `par_lento`, sem `lsof` ainda). O processo do
core segura de 5 a 17 sockets IPv4 em `127.0.0.1` ao longo da rodada (amostra
de `lsof` a cada 0,5 s), sockets esquecidos de propósito por
`servidor_que_aperta_a_mao_e_cai`, `frame.rs` e `tela.rs`; outros processos
com sockets em `127.0.0.1` sobem a chance. A F1 viu 1 em 5.

### A reprodução, em toda rodada

`client::PROPOSTA_DE_TESTE` (só com `cfg(test)`, por thread) põe na frente da
proposta do sistema uma que colide de verdade.
`client::testes_do_socket::encenar_a_colisao_com(endereço)` segura 64 sockets
IPv4 (em `127.0.0.1`, ou no curinga) e pede `[::]:0` ao sistema, **no máximo
8 192 vezes**, e acaba sempre com uma de duas medidas (`d7b969af`):
- `Colisao`: o sistema deu a porta de um ladrão (o macOS; espera de 256
  pedidos). O teste encena e prova o conserto;
- `Recusa`: nenhuma das 8 192 caiu numa porta presa — onde o sistema deixa, a
  chance disso é e^-32. O teste abre o socket pelo caminho do produto com os
  ladrões de pé, e a resposta a `127.0.0.1` na porta dele tem de chegar a ele;
  a ligação explícita numa porta presa fica dita, não afirmada (onde a escolha
  de porta já recusa, o produto está a salvo com ela ou sem ela, e reprovar por
  ela seria reprovar o sistema por estar certo).

Não há `cfg!(target_os = …)` nos testes: os três rodam e medem nos três
sistemas. A primeira volta de `a_volta_avisa_…` encena a colisão quando há
colisão — o `tentar` roda no futuro do `block_on`, na thread do teste — e só
então exige a proposta consumida. O braço `Recusa` foi forçado aqui, para
rodar o caminho que o Linux e o Windows devem tomar: verde, 0,20 s para os
dois testes de socket (8 192 propostas cada), e a ligação explícita numa porta
presa em `127.0.0.1` recusada.

Antes do conserto (a proposta usada como vem, que é o que
`Endpoint::client([::]:0)` fazia), **3 de 3**, 20,04 s cada, a mensagem da
bateria palavra por palavra:

```
assertion `left == right` failed: a volta não avisou a sala que o motor refez nela: a casca desenha fora de sala quem está sentado, e o áudio que segue o assento fecha a voz de quem só caiu e voltou
  left: None
 right: Some(Some(VoiceRoomId(5)))
```

### O conserto (no produto)

`client::socket_de_quem_conecta` abre o mesmo socket de pilha dupla que
`Endpoint::client` abria, mas trata a porta do sistema como proposta: larga o
socket proposto, liga uma **sonda IPv4 em `0.0.0.0:X`** e a larga, e só então
liga `[::]:X` explícito; se a sonda ou a ligação voltam `AddrInUse`, propõe
outra (até 16 propostas; depois cai para o socket só IPv4, que não tem o
defeito). `local_endpoint` adota o socket com `Endpoint::new`. O gêmeo, o
socket da `Batida` (`encontro::abrir_socket_local`, que o QUIC adota depois do
furo), passa pelo mesmo caminho e continua exigindo pilha dupla.

A sonda veio em `99142c72`. `6061e38c` conferia só pela ligação explícita a
`[::]:X`, que recusa o IPv4 ligado a um endereço mas **aceita** o do curinga
(tabela acima); achei isso ao medir o custo para a orquestradora. A sonda é
largada antes de ligar `[::]:X` de propósito: segurá-la junto funciona no
macOS, mas um sistema que trate a pilha dupla e o curinga IPv4 como o mesmo
lugar recusaria o `[::]:X` sempre, e todo cliente cairia para só IPv4. A
janela entre largar a sonda e ligar é de microssegundos; depois de ligado,
nenhum IPv4 entra na porta. *(Corrigido na rodada 2: a janela **não** está
coberta para um IPv4 no curinga — ver M1, abaixo.)*

No produto o defeito era a entrada ou a volta que, de vez em quando, espera
20 s por nada (ver «v0.15.1», abaixo).

Testes novos: `client::testes_do_socket::o_socket_de_quem_conecta_nao_divide_a_porta_com_um_socket_ipv4`
(com os ladrões em `127.0.0.1` e no curinga) e
`encontro::testes::o_socket_da_batida_nao_divide_a_porta_com_um_socket_ipv4`.

### As reversões (cada arquivo de volta conferido com `cmp`, e tocado antes de recompilar)

| reversão | vermelho |
|---|---|
| sem o conserto: `socket_conferido` devolve a proposta como veio | `a_volta_avisa_…` 3 de 3 em 20,0 s, `left: None`, `right: Some(Some(VoiceRoomId(5)))`; `o_socket_de_quem_conecta_…` e `o_socket_da_batida_…` 2 de 2, «a resposta mandada a 127.0.0.1:62416 não chegou ao socket de quem conecta (a porta do socket IPv4 alheio é 62416)» |
| o guarda do teste: `Aviso::Reconectado` com `voice_room: None` fixo | só `a_volta_avisa_…` (76 de 77 do `enlace`), `left: Some(None)`, `right: Some(Some(VoiceRoomId(5)))` |
| `local_endpoint` de volta ao `Endpoint::client([::]:0)` | `a_volta_avisa_…`: «a volta não abriu o socket pela proposta guardada, e o teste não encenou a colisão» |
| o socket da `Batida` de volta ao `[::]:0` de antes | `o_socket_da_batida_…`: «o socket da batida não passou pela proposta guardada» |
| sem a sonda (o conserto de `6061e38c`, com o teste de `99142c72`) — é também o vermelho antes da sonda | `o_socket_de_quem_conecta_…`: o caso de `127.0.0.1` passa, o do curinga reprova, «a resposta mandada a 127.0.0.1:60117 não chegou ao socket de quem conecta (a porta de um socket IPv4 preso que é a mesma: Some(60117))» |
| sem o conserto, com os testes de `d7b969af` | os dois de socket reprovam (portas 55837 e 58537 iguais às de um ladrão), `a_volta_avisa_…` em 20,0 s com `left: None`, `right: Some(Some(VoiceRoomId(5)))` |

### As rodadas — `cargo test -p seele-core --lib -- --test-threads=1`, com o codec

| rodada | resultado | tempo dos testes | carga (1 min) |
|---|---|---|---|
| 1 | 458 passados | 24,56 s | 7,4 |
| 2 | 458 passados | 24,48 s | 6,9 |
| 3 | 458 passados | 24,50 s | 14,9 |
| 4 | 458 passados | 24,58 s | 14,7 |
| 5 | 458 passados | 24,59 s | 11,1 |
| 6 (sobre `6061e38c`) | 458 passados | 24,65 s | 5,7 |
| 7 (sobre `6061e38c`) | 458 passados | 25,56 s | 13,4 |
| 8 (sobre `6061e38c`) | 458 passados | 25,69 s | 36,6 |
| 9 (sobre `d7b969af`) | 458 passados | 24,60 s | 4,6 |
| 10 (sobre `d7b969af`) | 458 passados | 24,44 s | 4,6 |
| 11 (sobre `99142c72`) | 458 passados | 24,60 s | 5,7 |
| 12 (sobre `99142c72`) | 458 passados | 24,53 s | 5,8 |
| 13 (sobre `99142c72`) | 458 passados | 24,47 s | 5,7 |

`a_volta_avisa_…` sozinho, 3 de 3 sobre `99142c72`, 0,01 s. Clippy (`-p
seele-core -p seele-server --all-targets -D warnings`) e fmt limpos.

---

## v0.15.1

**1. O defeito existe na v0.15.1 publicada?** Sim. Conferido pelo que está
publicado e não suposto: a API de releases dá `v0.15.1 · 54f542f94 ·
2026-10-08T04:14:07Z`, a mais nova. Em `54f542f94`:
- `client.rs`, `local_endpoint` (linha 1658): `quinn::Endpoint::client(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)))`;
- `encontro.rs`, `abrir_socket_local` (linhas 294–296): `set_only_v6(false)` e
  `bind([::]:0)`;
- `enlace.rs` chama `local_endpoint` na entrada (linhas 1096 e 1319) e na volta
  da bateria, em `tentar` (linha 2836, `local_endpoint(furo)`);
- `transport.rs`: `IDLE_TIMEOUT` de 20 s.

É o código em que a colisão foi medida, e nos dois casos que ele não cobre: o
IPv4 ligado a um endereço e o IPv4 no curinga.

**2. O custo para quem usa.** Por tentativa, a chance é ~N/16 384 (a faixa
efêmera do macOS). Quando cai, é uma tentativa que espera 20 s e falha com
`SemResposta`; a bateria tenta de novo com um socket novo. Com uma ressalva à
conta do controlador, medida: **N não é todo socket UDP IPv4 no endereço.**
- Contam os IPv4 **não conectados** na faixa efêmera ligados ao endereço
  exato por onde a resposta volta (o da rede de casa depois do NAT; `127.0.0.1`
  para quem fala com um servidor na própria máquina) **e os ligados ao curinga
  `0.0.0.0`**, que roubam também (535 colisões em 3 000, e a resposta a
  `127.0.0.1:X` foi ao IPv4 curinga 20 de 20; com o endereço da rede de casa
  no lugar de `127.0.0.1`, não medi).
- **Não contam os conectados.** Com 3 000 IPv4 conectados no endereço da rede
  de casa (um `connect` a um destino, como o QUIC de um navegador faz), houve
  503 colisões de porta, e a resposta de outro remetente foi à pilha dupla 20
  de 20.
- Nesta máquina, agora: os 5 a 6 sockets no IP da rede de casa que o `lsof`
  mostra são todos conectados (`192.168.50.142:… -> …:443`, do navegador e do
  Claude); os `IPv4 *:*` não têm porta. **N = 0** aqui, agora. O processo do
  SEELE abre IPv4 no curinga também — `encontro::perguntar_a` liga `0.0.0.0:0`
  para perguntar ao quarto, por pouco tempo —, e esse pode cair na conta da
  própria entrada.

O número de campo (quantas máquinas de quem usa têm IPv4 não conectados ou no
curinga na faixa efêmera) não foi medido.

**3. Windows e Linux.**
- **Medido só no macOS** (o Darwin desta máquina): a colisão, para onde vai a
  resposta, a recusa da ligação explícita, a sonda, o conserto, e o braço
  `Recusa` do teste forçado.
- **O que sei do Linux, sem ter medido:** pelo que conheço do kernel, a
  escolha de porta de um `[::]` sem `IPV6_V6ONLY` confere os IPv4 na mesma
  porta, e o defeito não existiria lá. Também espero que o Linux recuse ligar
  `[::]:X` com um `0.0.0.0:X` de pé — é por isso que a sonda é largada antes.
  Nada disso foi medido.
- **Windows:** não sei. As regras de compartilhamento de porta dos sockets de
  modo duplo são outras, e eu não as medi; não sei nem em qual braço o teste
  cai.
- **O que só o CI vai provar** (`ubuntu-24.04`, `windows-2022`, `macos-15`):
  em qual braço cada sistema cai, e que o socket do produto recebe a resposta
  com os ladrões de pé. Se o Linux ou o Windows caírem em `Colisao` e
  aceitarem a ligação explícita na porta presa, o teste reprova pela resposta
  — e isso seria o defeito existindo lá sem o conserto cobrir. Se a sonda
  largada antes não bastar num deles, `socket_de_quem_conecta` volta erro, o
  `expect` do teste reprova, e é igualmente visível. A pendência 60
  (`a0bdcd9a`) registra a mesma lacuna; o caso do curinga é posterior a ela e
  vale acrescentar lá.

---

## Parte 2 — `par_lento`, `o_server_nao_perde_mensagem_calado_quando_um_par_para_de_ler`

### A medida — e ela desmente o «lote que ainda não gravou»

Instrumentação temporária no teste: a trajetória do contador a cada 50 ms
desde o fim da rajada, a fila do escritor, o contador relido por mais 15 s,
quantas sessões o servidor desligou por atraso, a leitura do fluxo de cada
falante depois, e o histórico inteiro do canal, falante por falante.

Sem carga extra, as 1 160 gravam num lote só, 50–70 ms depois da rajada. Com a
máquina carregada (ondas de 6 a 8 cópias do teste em paralelo, carga 9 a 88),
8 reprovações em 90 cópias. Nenhuma era lote atrasado:

| rodada | contagem final (e 15 s depois) | sessões desligadas | falantes com falta | faltas somadas |
|---|---|---|---|---|
| m3-1-1 | 1 074 | 16 | 18, 19 | 86 |
| m3-1-3 | 1 120 | 20 | 13, 17, 18, 19 | 40 |
| m3-2-3 | 1 137 | 21 | 17 | 23 |
| m3-2-4 | 1 113 | 21 | 16, 17, 18, 19 | 47 |
| m3-3-6 | 1 120 | 4 | 16, 19 | 40 |

(e três antes do histórico por falante: 1 134, 1 152 e 1 159, paradas por
15 s, com 8 falantes desligados na de 1 152.) Em todas:
- a contagem **não sobe mais** em 15 s, e a fila do escritor está vazia;
- **todo falante com falta recebeu `Disconnecting(FellBehind)`**;
- o que falta de cada um é uma **cauda contígua** (o falante 16 com 42
  gravadas, faltando 42..57), e a soma das caudas fecha as 1 160.

O que acontece: o barramento é um `broadcast` de 1 024. Sob carga, dois lotes
seguidos anunciam mais de 1 024 mensagens (279 + 839, 0 → 1 116 em 184 ms)
antes de a sessão de um falante voltar a ler o barramento; ela recebe
`Lagged`, se despede com `FellBehind`, e o que o falante mandou depois fica sem
ler. Que a sessão estava ocupada com as mensagens do próprio falante (cada uma
passa pelo cadeado de `persistence`, que o escritor segura enquanto grava o
lote) é inferência; o atraso e a despedida foram vistos.

Reperguntar o contador — o conserto de 14/09, de `4770dca1`, desfeito por
`becb0257` — não consertaria: ele nunca chega a 1 160.

### A reprodução de propósito

No teste (temporário): o teste segura o cadeado de `persistence` durante a
rajada, joga 2 048 eventos inofensivos no barramento
(`Event::MessageRejected` de uma pessoa que não existe) e solta o cadeado. As
sessões dos falantes ficam presas no cadeado enquanto o barramento passa à
frente delas.

- teste antigo + reprodução: **3 de 3** vermelhos, «o servidor perdeu mensagem
  antes de gravar, e aí reconectar não repõe nada», `left: 30`/`26`/`28`,
  `right: 1160`;
- teste novo + reprodução: **3 de 3** verdes — 27, 28 e 30 gravadas, os 20
  falantes desligados com `FellBehind`, cada um com um começo sem buraco.

### O conserto (só o teste; nenhum gancho no servidor)

Sem relógio em lugar nenhum — cada espera acaba num sinal do servidor, e um
prazo de segurança de 120 s só existe para o teste não travar:
- o ouvinte manda um `Ping` depois do `JoinChannel` e espera o `Pong`: a partir
  dali, tudo o que se gravar no canal é dele também;
- cada falante manda um `Ping` atrás da rajada e lê o fluxo de volta até o
  `Pong` dele (a sessão leu, e pôs na fila do escritor, cada mensagem de antes:
  o fluxo de controle é lido em ordem) ou até a despedida;
- **a testemunha:** uma mensagem num canal criado só para ela, postada pelo
  `Server::post` público atrás da fila do escritor; o anúncio dela no
  barramento sai depois de gravado tudo o que entrou antes;
- então, falante por falante, pelo histórico: sem repetir; sem buraco (do
  começo até a última gravada, nada falta que não tenha sido recusado com
  `MessageRejected`); quem teve o `Pong` tem as 58; quem tem falta foi
  desligado com `FellBehind`; e a contagem do canal é a soma;
- o ouvinte lê sem prazo por leitura até ter tudo o que foi gravado ou a
  despedida (a asserção da pendência nº 1 continua a mesma, contra o gravado).

O teste passou de ~3,3 s para 0,2–0,7 s.

### A pergunta do controlador: a asserção ficou mais fraca?

Num caso só, e estreito: **a sessão lê uma mensagem, a perde, e se despede com
`FellBehind` antes de ler a seguinte do mesmo falante.** A mensagem perdida fica
na cauda, e o teste não a separa das que nunca foram lidas. Um servidor que
perdesse a mensagem *e* seguisse lendo deixaria buraco, e é pego; um que
perdesse e não se despedisse é pego; um que perdesse de um falante que teve o
`Pong` é pego.

*(Corrigido na rodada 2: isto estava errado. A revisão achou mais dois casos
em que o teste de `de04ab26` ficava mais fraco — o servidor que recusa tudo
passava com zero gravadas (I1), e a despedida calada só reprovava pelo prazo
de segurança (I2). Os dois estão consertados abaixo, em «Rodada 2». O caso
estreito continua o único que sobra.)*

Fora desse caso o teste novo é mais forte que o antigo, e medido:
- nas rodadas sem carga extra (6 de 6 pelo `cargo`), nenhum falante foi
  desligado: todos tiveram o `Pong`, e o teste exige as 1 160, como o antigo;
- e o antigo, quando um falante era desligado, reprovava sempre — não
  separava a perda da cauda não lida; só não passava. O caso estreito acima
  também não era pego por ele: era uma reprovação que reprovaria do mesmo jeito
  sem a perda.

As reversões que o provam (`session.rs` e o teste de volta conferidos com `cmp`
/ `git diff --quiet`):

| reversão | vermelho, 3 de 3 |
|---|---|
| o servidor descarta uma mensagem que leu (a chave 30, uma vez) | «o servidor pulou mensagens do falante 00 e gravou as de depois: faltam [29]» |
| o servidor descarta a última de um falante (a chave 58) | «o servidor perdeu mensagem antes de gravar, e aí reconectar não repõe nada: a sessão do falante 00 leu todas e faltam [57]» |
| a despedida calada (o braço de `Lagged` sem o `Disconnecting`) + a reprodução | «o falante 00 ficou sem [6, …, 57] gravadas e o servidor não disse que ficou para trás: Caiu("stream finished early (0 bytes read)")» |
| sem o conserto + a reprodução | o teste antigo, «o servidor perdeu mensagem antes de gravar», `left: 30`, `right: 1160` |

### As rodadas

`cargo test -p seele-server --test par_lento`, 6 rodadas, carga 24–29 (a
bateria do controlador rodando): 6 de 6, 1 160 gravadas, nenhum falante
desligado, o ouvinte desligado com `FellBehind`, 0,24–0,69 s de teste. A
primeira levou 316,7 s de relógio, quase todos esperando a trava do cargo; a
compilação em si, 9,85 s.

`cargo test -p seele-server` inteiro, uma vez: saída 0, 18 alvos, **687
passados, 0 reprovados**, 3 295 s de relógio (02:20–03:15, carga 18–47).

### A pendência

`docs/pendencias.md`, `ffbcc3fc`: acréscimo nas duas passagens que davam a
espera de cinco segundos por feita — que o conserto de 14/09 (`4770dca1`) foi
desfeito por `becb0257`, que a causa medida é outra, e que o conserto novo é
`de04ab26`. O registro de antes não foi reescrito.

---

## Rodada 2 — o que a revisão pediu

A revisão aprovou a parte 1 e pediu conserto na parte 2 (2 Importantes, 5
Menores). Cada passo foi commitado verde; o vermelho veio antes de cada
conserto, as alavancas de reversão foram temporárias em `session.rs` (por
variável de ambiente, no mesmo binário), e `session.rs` voltou idêntico depois
de cada uma (`cmp` contra a cópia de `06d0a208`, e tocado antes de recompilar).

| | commit | o quê |
|---|---|---|
| I1 | `5b167586` | nenhuma recusa: o teste exige zero `MessageRejected` |
| I2 | `fa8a5974` | o ouvinte assina o canal da testemunha e lê até ela ou até a despedida |
| M4 | `0c53a0c2` | a escrita que falha no meio da rajada vira desfecho, e não erro |
| M3 | `deca9d10` | o comentário do braço `Lagged` diz o segundo jeito de ficar para trás |
| M5 | `de6a4a98` | «pode repetir com a mesma chave», a pedido (`reenviar_mensagem`) |
| M1 | `885bda67` | a janela entre a sonda e a ligação fica dita, com a medida |
| M2 | `866718cc` | a batida diz no registro quando o socket conferido não abre |

### As reversões e os vermelhos

| reversão (alavanca temporária) | antes do conserto | depois |
|---|---|---|
| **I1**: o braço `SendMessage` recusa toda mensagem com `PermissionDenied`, antes da fila (a da revisão) | 3 de 3 **verdes**, «gravadas 0, chegaram 0, perdidos no barramento 0» | 3 de 3 vermelhos: «o servidor recusou 58 mensagens do falante 00 (chaves [1, …, 58]) num canal que existe e em que ele pode escrever» |
| **I2**: o braço `Lagged` volta a `continue`, calado, sem a reprodução (a da revisão) | vermelho só pelo prazo, em **120,9 s**: «o ouvinte não teve nem tudo nem a despedida: o servidor travou: Elapsed(())» | 3 de 3 vermelhos em 0,4–1,0 s pela asserção: «pendência nº 1: 1023 de 1160 chegaram e o servidor não disse nada — 0 eventos morreram no barramento (a leitura acabou em Some("a testemunha"))» |
| **M4**: a sessão despede cada falante com `FellBehind` ao ler a chave 20 | 3 de 3 com «Error: connection lost — closed by peer: fell behind (code 0)», antes de qualquer asserção | sem contar em `atrasos`: 3 de 3 vermelhos pela asserção certa, «a sessão foi encerrada por ter ficado para trás e o contador marcou zero»; contando: 3 de 3 verdes, 380 gravadas (19 por falante), os 20 `Desligado(FellBehind)`, o ouvinte com as 380 e a testemunha |
| **M2**: o socket conferido da batida encenado como erro | «o socket conferido da batida não abriu e o registro não disse nada, nem por quê. Linhas: []» | 3 de 3 verdes, com a linha `WARN` e o motivo |

M1, M3 e M5 são só comentário e texto.

### M1, a janela que fica aberta

Entre largar a sonda `0.0.0.0:X` e ligar `[::]:X`, um IPv4 que se ligue a
`X` num endereço faz a ligação voltar `AddrInUse` (vem outra proposta), mas
um que se ligue ao curinga é aceito junto, e a resposta daquela conexão vai a
ele. Medido aqui (Python, 5 000 vezes, carga ~10): **4,1 µs na mediana, 9,0
µs no p99, 82,7 µs no máximo**. A chance por conexão é ~`r` × 9 µs / 16 384,
com `r` as ligações de IPv4 no curinga por segundo na máquina; com `r` = 100,
uma em 18 milhões. `r` não foi medido. Dito na doc de `socket_conferido`.

De passagem, a mesma medida deu um número de campo para a conta de N da
seção «v0.15.1»: das 5 006 propostas de `[::]:0`, **6** caíram numa porta que
um IPv4 desta máquina segurava naquele instante (0,12 %, N ≈ 20 sockets IPv4
na faixa efêmera, com a bateria da orquestradora rodando).

### M3, o achado de produto

O comentário do braço `Lagged` em `crates/seele-server/src/session.rs` dizia
que só acontece com quem parou de ler. Agora diz o segundo jeito medido: o
par que lê e escreve em rajada, cuja sessão espera no cadeado de PERSISTENCE e
na fila do escritor enquanto os lotes dos outros anunciam mais de um anel. Só
comentário; a pendência de produto (61) fica com o controlador.

### As rodadas da rodada 2

- `cargo test -p seele-server --test par_lento`, 5 rodadas, carga 8,2–8,7:
  5 de 5, 1 160 gravadas, nenhum falante desligado, o ouvinte desligado com
  `FellBehind` e a leitura acabando na despedida; 0,24–0,25 s de teste.
- `cargo test -p seele-core --lib -- --test-threads=1` com o codec, uma vez
  (o core mudou em M1 e M2): 459 de 459, 24,88 s, carga 7,7.
- `cargo test -p seele-server` inteiro, uma vez (13:28–13:29, carga 8,2–9,8): saída 0, 18 alvos, **687 passados, 0 reprovados**, 66,1 s de relógio. As quatro linhas `panicked` da saída são de testes que esperam o pânico.
- fmt limpo; clippy `-p seele-core -p seele-server --all-targets -D warnings`
  limpo.

## Preocupações

1. **Um achado de produto da parte 2, que não consertei.** Quem só escreve
   também é desligado com `FellBehind` quando lotes grandes passam do anel de
   1 024 do barramento; a despedida é dita e o cliente de verdade marca o resto
   `SemResposta`. Num servidor de 50 pessoas um lote não chega perto de 1 024
   em 200 ms; sob carga de teste, chegou. Na rodada 2 o comentário de
   `session.rs` passou a dizer isso (M3); a pendência (61) é do controlador.
2. **O teste da parte 2 mudou o que guarda** (ver a pergunta acima). Se a
   decisão for outra, `de04ab26` sai sozinho.
3. **As minhas medidas pesaram na máquina compartilhada.** Entre 01:33:15 e
   01:36:40 (-0300) um processo meu segurou 3 000 portas UDP em `127.0.0.1` —
   exatamente o que causa a parte 1 em qualquer cliente de pilha dupla que
   abrisse socket nesse intervalo — e, entre 01:24 e 01:37 e entre 02:00 e
   02:03, ondas de 6 a 8 cópias do `par_lento` levaram a carga a 38–88.
   Reprovação da bateria do controlador nessas janelas pode ser minha.
4. Os sockets esquecidos de propósito (`servidor_que_aperta_a_mao_e_cai`,
   `frame.rs`, `tela.rs`) continuam esquecidos: com o conserto, não roubam mais
   resposta, mas seguem sendo vazamento.
5. `tipo_de_fluxo.rs` também perdeu a tolerância em `becb0257`; está fora deste
   lote.
6. **Linux e Windows não foram medidos.** Os testes da parte 1 rodam e medem
   nos três (`d7b969af`), mas em qual braço cada um cai, e se a sonda largada
   antes basta lá, só o CI diz. O braço `Recusa` foi forçado aqui e passa.
7. **O conserto de `6061e38c` estava incompleto, e eu só vi ao medir o custo:**
   o IPv4 no curinga passava pela ligação explícita. `99142c72` cobre; a
   pendência 60 (`a0bdcd9a`) foi escrita antes e não tem esse caso.
8. A sonda IPv4 roda a cada socket de quem conecta: um `bind` a mais por
   tentativa de conexão, sem custo medido que pese.
9. **A janela entre a sonda e a ligação continua aberta** para um IPv4 no
   curinga (M1): 4,1 µs na mediana, a chance por conexão depende de `r`, que
   não medi. Fechá-la exigiria segurar a sonda junto, o que funciona no macOS
   e, pelo que espero do Linux, recusaria todo `[::]:X` lá.
10. **O caso estreito do `par_lento` sobra:** uma mensagem lida, perdida, e a
    despedida antes da seguinte do mesmo falante passam. Nem o teste antigo o
    pegava.
