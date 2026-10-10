# Lote Y da F5: os intermitentes — relatório

Ramo `0.16/lotes-y`, a partir de `d9124cea`. Worktree:
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-aa13fd41758429571`.

| commit | o quê |
|---|---|
| `6061e38c` | parte 1: o socket de quem conecta não divide a porta com um socket IPv4 (`seele-core`) |
| `de04ab26` | parte 2: o `par_lento` espera por sinais do servidor e afirma o que ele promete (salvo pelo controlador como «em curso»; o código é o que foi medido abaixo) |
| `ffbcc3fc` | parte 2: o acréscimo em `docs/pendencias.md` |
| este | o relatório |

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
| ligação **explícita** a `[::]:X` com um IPv4 segurando `X` | `EADDRINUSE` (127.0.0.1 e rede de casa) |
| curinga IPv4 `0.0.0.0:0` depois de IPv4 específicos | 0 repetidas |
| com `[::]:X` nosso de pé, ligar `127.0.0.1:X` ou `0.0.0.0:X` | `EADDRINUSE` |

A escolha de porta de `[::]:0` confere só a família IPv6; a ligação explícita
confere as duas.

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
proposta do sistema uma que colide de verdade:
`client::testes_do_socket::proposta_que_colide` segura 64 sockets IPv4 em
`127.0.0.1` e pede `[::]:0` ao sistema até ele cair na porta de um deles
(espera de 256 pedidos; 8 192 no máximo, chance de não achar e^-32). A
primeira volta de `a_volta_avisa_…` passa a encenar a colisão sempre — o
`tentar` roda no futuro do `block_on`, na thread do teste — e confere que
encenou. Fora do macOS a escolha de porta confere as duas famílias e não há
colisão a encenar; ali o teste segue sem ela, e no macOS reprova se não a
encenar.

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
socket proposto, liga de novo, explícito, em `[::]:X`, e propõe outra se a
ligação volta `AddrInUse` (até 16 propostas; depois cai para o socket só IPv4,
que não tem o defeito). `local_endpoint` adota o socket com `Endpoint::new`. O
gêmeo, o socket da `Batida` (`encontro::abrir_socket_local`, que o QUIC adota
depois do furo), passa pelo mesmo caminho e continua exigindo pilha dupla.

No produto o defeito era a entrada ou a volta que, de vez em quando, espera
20 s por nada, quando outro programa segura um socket UDP ligado ao endereço
por onde a resposta volta (medido com `127.0.0.1` e com o da rede de casa).

Testes novos: `client::testes_do_socket::o_socket_de_quem_conecta_nao_divide_a_porta_com_um_socket_ipv4`
e `encontro::testes::o_socket_da_batida_nao_divide_a_porta_com_um_socket_ipv4`
(a proposta que colide entra, e um datagrama a `127.0.0.1:porta` tem de chegar
ao socket devolvido).

### As reversões (cada arquivo de volta conferido com `cmp`, e tocado antes de recompilar)

| reversão | vermelho |
|---|---|
| sem o conserto: `socket_conferido` devolve a proposta como veio | `a_volta_avisa_…` 3 de 3 em 20,0 s, `left: None`, `right: Some(Some(VoiceRoomId(5)))`; `o_socket_de_quem_conecta_…` e `o_socket_da_batida_…` 2 de 2, «a resposta mandada a 127.0.0.1:62416 não chegou ao socket de quem conecta (a porta do socket IPv4 alheio é 62416)» |
| o guarda do teste: `Aviso::Reconectado` com `voice_room: None` fixo | só `a_volta_avisa_…` (76 de 77 do `enlace`), `left: Some(None)`, `right: Some(Some(VoiceRoomId(5)))` |
| `local_endpoint` de volta ao `Endpoint::client([::]:0)` | `a_volta_avisa_…`: «a volta não abriu o socket pela proposta guardada, e o teste não encenou a colisão» |
| o socket da `Batida` de volta ao `[::]:0` de antes | `o_socket_da_batida_…`: «o socket da batida não passou pela proposta guardada» |

### As rodadas — `cargo test -p seele-core --lib -- --test-threads=1`, com o codec

| rodada | resultado | tempo dos testes | carga (1 min) |
|---|---|---|---|
| 1 | 458 passados | 24,56 s | 7,4 |
| 2 | 458 passados | 24,48 s | 6,9 |
| 3 | 458 passados | 24,50 s | 14,9 |
| 4 | 458 passados | 24,58 s | 14,7 |
| 5 | 458 passados | 24,59 s | 11,1 |
| 6 (sobre o commit) | 458 passados | 24,65 s | 5,7 |
| 7 (sobre o commit) | 458 passados | 25,56 s | 13,4 |
| 8 (sobre o commit) | 458 passados | 25,69 s | 36,6 |

`a_volta_avisa_…` sozinho, 5 de 5, 0,01–0,02 s. Clippy (`-p seele-core
-p seele-server --all-targets -D warnings`) e fmt limpos.

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

## Preocupações

1. **Um achado de produto da parte 2, que não consertei.** Quem só escreve
   também é desligado com `FellBehind` quando lotes grandes passam do anel de
   1 024 do barramento; a despedida é dita e o cliente de verdade marca o resto
   `SemResposta`, mas o comentário de `session.rs` só prevê o par que parou de
   ler. Num servidor de 50 pessoas um lote não chega perto de 1 024 em 200 ms;
   sob carga de teste, chegou. Decidir se isso vira pendência é do controlador.
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
6. A reprodução da parte 1 só encena no macOS; em Linux e Windows a escolha de
   porta já confere as duas famílias (não medi lá), e o teste segue sem a
   colisão.
