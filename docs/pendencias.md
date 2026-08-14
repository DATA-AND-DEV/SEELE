# Pendências conhecidas

O que está quebrado ou frouxo e ainda não foi resolvido. Ordenado por quanto
atrapalha na prática, não por dificuldade.

Uma entrada que fecha **não sai da lista e não é renumerada**: os números são
citados de fora — "pendência #9" aparece em `docs/` e em `specs/` — e renumerar
faria cada citação apontar para outra coisa. Ela fica no lugar, marcada como
fechada, com a data e com o que a substituiu.

## 1 · Rajada de mensagens grandes perde entrega

**Sintoma.** Dez mensagens de ~3,9 KB enviadas em rajada, sem o receptor ler no
meio: só duas chegam. As mesmas dez, com o receptor drenando entre lotes,
chegam todas. Corpos pequenos chegam todos em qualquer ordem.

**O que se sabe.** Não é o tamanho isolado — 10 de 3900 bytes com drenagem
entre lotes entregam 10/10. Suspeitas na ordem: janela de controle de fluxo do
QUIC no começo da conexão, a fila da tarefa que grava em lote, ou a tarefa
leitora do cliente morrendo em silêncio e o erro sendo engolido por um
`if let Ok(Ok(_))`.

**Uma afirmação daqui estava errada.** Esta seção dizia "não é o conserto de
cancelamento: o comportamento é idêntico antes e depois". Aquela comparação foi
feita quando só o **cliente** tinha sido consertado; a sessão do servidor
continuou lendo quadro dentro de um `select!` até o defeito derrubar o
`acceptance_m5` no Linux e ser diagnosticado de verdade. Descartar o
cancelamento com meia correção na mão não valia nada, e a frase saiu.

**O que ficou tentado.** `crates/seele-conformance/tests/rajada.rs` roda o
cenário da pendência. Ele passa — inclusive com a sessão sabotada de volta ao
código antigo —, então **não reproduz o sintoma no macOS**. Não confirma nem
descarta o cancelamento como causa; serve como rede, e roda nos três sistemas.

**Por que não foi resolvido.** Precisa de instrumentação dos dois lados, e o
`Casper::connection()` é `pub(crate)`, então um teste de conformidade não
consegue conferir o que foi gravado. Investigar isto direito é uma tarefa
própria, não um remendo no fim de outra.

**Quando dói.** Colar um texto longo, ou um cliente reconectando e recebendo
histórico em rajada. Não apareceu em uso normal.

## 2 · A reprodução perde amostras devagar, o tempo todo

**Sintoma.** "ÁUDIO LOCAL FALHANDO" acende sozinho e volta a acender depois de
apagar, com o áudio audivelmente bom.

**Medido**, com `:sync` num `plug --hospedar` sem ninguém do outro lado:

| | captura | saída |
|---|---|---|
| arranque | 832 | 320 |
| +10 s, modo TECLA | 832 | 320 |
| +10 s, modo ABERTO | 832 | 448 |

Duas coisas separadas, e uma delas eu já sabia errado por outro motivo:

1. **A captura estoura uma vez, no arranque, e nunca mais.** O fluxo começa a
   encher o anel antes de alguém drenar. Inofensivo — e era isto que acendia o
   aviso para sempre, antes de a regra virar derivada.
2. **A reprodução perde amostras continuamente**, algumas centenas por dezena
   de segundos. É pouco para ouvir e é suficiente para o aviso ser verdade.

**O que a medição desmentiu.** A suspeita era que o anel de captura só fosse
drenado ao transmitir, o que explicaria o aviso sumir no modo aberto. **Não é
isso**: os contadores crescem igual nos dois modos. O laço drena a captura
incondicionalmente, a cada 2 ms. A diferença que aparece na interface entre
TECLA e ABERTO ainda não tem explicação.

**Suspeitas, na ordem.** O laço de reprodução empurra um quadro de 20 ms por
tica e recupera atraso somando 20 ms ao alvo — se uma volta passar do prazo,
ele não repõe o que ficou para trás. Depois: contagem de canais do dispositivo
de saída, e a conversão de taxa quando o dispositivo não roda a 48 kHz.

**Por que não foi resolvido agora.** Precisa de instrumentação dentro do
retorno de chamada de áudio, e chutar aqui produziria um conserto que parece
funcionar. O `:sync` já mostra os números, que é o começo.

## 3 · O instalador do Windows não põe `plug` no `PATH`

O `.exe` do NSIS instala o app e os dois programas de terminal em
`%LOCALAPPDATA%\Programs\SEELE`, e **não acrescenta essa pasta ao `PATH`**.
Quem instalou pelo app tem o `plug`, mas precisa do caminho inteiro para
chamá-lo.

O NSIS do Tauri aceita um gancho de pós-instalação que resolveria isso. Não
entrou agora porque não tenho Windows aqui para testá-lo, e um gancho errado
quebra o instalador inteiro — que é pior do que não ter `PATH`.

No macOS o problema é o mesmo e a saída está nas notas de release: dois
`ln -s`. No Linux o `.deb` já instala em `/usr/bin`.

## 4 · Só dá para conectar na mesma rede

Fora da rede local, só funciona com o anfitrião alcançável de fora: VPS, porta
encaminhada à mão, ou VPN. Atrás de um roteador doméstico, não conecta — e a
mensagem não explica isso, que é metade do problema.

O ADR 0022 mapeia a escada de saídas, do que não custa terceiro nenhum (IPv6,
UPnP) ao que custa (ponto de encontro, retransmissão), e diz o que cada degrau
cobra em troca. Nada disso está implementado.

**É o que mais separa o SEELE de "dá para usar com os amigos" hoje.**

## 5 · Não há limitação de taxa

`DisconnectReason::RateLimited` existe no protocolo e **nunca é enviado**. Um
convidado legítimo pode inundar o Dogma de mensagens ou de handshakes.

Não atrapalha rede local. **Bloqueia expor à internet**, e é a dívida mais
séria de segurança depois do ADR 0021.

## 6 · Apelido é validado só por tamanho

Trinta e dois bytes, e nada sobre o conteúdo. O terminal está protegido — o
ratatui filtra todo caractere de controle, verificado — e o app usa
`textContent`. Sobra a possibilidade de sósia: caracteres de direção invertida
ou parecidos com os de outra pessoa no roster.

Baixo impacto num Dogma de amigos, real num aberto.

## 7 · A matriz de três SOs nunca foi verde por inteiro

Linux e Windows compilam no CI, mas ninguém rodou o `plug` neles fora disso.
`docs/teste-duas-maquinas.md` é o roteiro.

## 8 · Sem troca de chaves pós-quântica

Ao tirar o `aws-lc-rs` da árvore (para não exigir CMake e NASM no Windows)
perdeu-se o `prefer-post-quantum` do rustls. Nada protege contra gravar hoje e
decifrar depois. Aceitável para v1 — o modelo é TOFU sobre TLS 1.3 e E2EE de
mídia já é pós-v1 — mas é perda real.

## 9 · `:conectar` não reconecta em execução

O comando existe e avisa que não faz. **`:ejetar` agora resolve o caso comum**:
volta à tela de seleção, com a conexão e o áudio derrubados de verdade, e de lá
se escolhe outro Dogma. O que continua faltando é trocar de destino num comando
só, sem passar pela tela.

O que o laço externo mostrou é que o teardown fecha —
`crates/seele-conformance/tests/ejetar.rs` conecta, solta e conecta de novo no
mesmo processo. O que a pendência recusava era outra coisa: trocar a conexão por
baixo de uma sessão viva, com roster e áudio de pé.

## 10 · O esquema `seele://` não é clicável

Não está registrado no sistema operacional. Quando for, o cliente **precisa
perguntar antes de conectar**: um link que inicia conexão sozinho é superfície
nova. Ver ADR 0006.

## 11 · Reconectar rápido pode esvaziar o roster do Cage

**Sintoma esperado.** Alguém dá `:ejetar` e entra de novo em seguida. A sessão
nova sobe, fala e ouve normalmente, e o Cage aparece **vazio** — sem nem a
própria pessoa — até o movimento de alguém redesenhar a lista.

**O que se sabe.** É uma corrida entre a sessão que morre e a que nasce, e as
duas mexem na lotação pela mesma chave. `Occupancy::seat` começa apagando o
piloto de toda parte antes de sentá-lo (`dogma.rs:171-174`), e o desmonte da
sessão antiga chama `occupancy.vacate(cage, pilot)` (`session.rs:845`). Como
`vacate` filtra só por `PilotId` (`dogma.rs:177-181`), ele não distingue a
cadeira da sessão velha da cadeira da sessão nova: se o desmonte da primeira
chegar **depois** do `seat` da segunda, apaga a segunda. A ordem depende de
quando a conexão QUIC antiga é dada por morta, o que ninguém controla.

Só atinge a mesma identidade voltando — dois pilotos diferentes não colidem,
porque as chaves diferem. E o cliente não tem como serializar isso do lado dele:
`Drop for Enlace` é um `abort()`, que é assíncrono.

**Encontrado lendo, não observado.** Saiu da revisão do
`crates/seele-conformance/tests/ejetar.rs`, ao perguntar por que os dois lados
do teste usavam a mesma semente. **Não foi reproduzido em uso**, e fica
registrado como defeito de leitura, e não como relato de campo.

**A janela tem dois tamanhos, e o segundo não é estreito.** Com a rede
entregando, o `CONNECTION_CLOSE` da conexão antiga chega e o Dogma desmonta
aquela sessão em milissegundos — aí a corrida exige que o desmonte caia depois
de um handshake inteiro, e é de fato improvável. Mas o `CONNECTION_CLOSE` é um
pacote só e não é retransmitido: se ele se perder, o Dogma não fica sabendo de
nada e só derruba a sessão pelo tempo ocioso, que é o
`seele_proto::transport::IDLE_TIMEOUT` de **20 s**. Contra um handshake com
orçamento de 10 s, a janela deixa de ser uma corrida e passa a ser a regra —
qualquer volta dentro desses 20 s cai nela. Perder um datagrama numa rede real
não é exótico, e é justamente ao ejetar por causa de uma conexão ruim que se
volta depressa.

**O que ficou tentado.** Nada, de propósito — mas o `ejetar.rs` foi escrito para
não depender disto: o teste que mede lotação usa duas identidades distintas, e o
que faz a mesma pessoa voltar não olha a lotação. Está comentado nos dois
lugares, senão alguém junta os dois "simplificando" e ganha uma reprovação
intermitente no lugar do defeito.

**Por que não foi resolvido.** O conserto é no Dogma, não no cliente: `vacate`
precisa saber de qual sessão veio o pedido — carregar o `SessionId` no
`Occupant` e só desocupar se for o mesmo —, e isso mexe em `seat`, `vacate`,
`vacate_everywhere` e nos avisos de roster. É tarefa própria, com revisão
própria, e não um remendo no fim de uma tarefa de teste.

**Quando dói.** `:ejetar` seguido de reconexão imediata no mesmo Cage, que é
exatamente o que a tela de seleção convida a fazer. Some assim que qualquer
pessoa entra ou sai, porque aí o roster é reconstruído.

## 12 · Fechada em 2026-08-13 · A conferência da impressão digital do convite

**O que era.** O app lia a impressão digital de um `seele://` e não a conferia:
colar um link com impressão conectava como se não houvesse impressão nenhuma.
O `plug` conferia — ou parecia conferir: comparava a impressão esperada com ela
mesma, porque `PinDecision::Matches` não carregava a ofertada, e era um teste
que não tinha como reprovar. Duas cascas, dois comportamentos, nenhum dos dois
o que o ADR 0006 desenhou.

**O que ficou no lugar.** Uma decisão só, em `seele-core`, com cinco desfechos
nomeados (`tofu::Verdict`). A impressão do link atravessa a ponte
(`ConnectConfig::expected_fingerprint` → `Destino::impressao_esperada`) e a
comparação acontece antes de haver sessão. No primeiro contato, um convite que
não confere **recusa**: derruba a conexão e desfaz o pin que o TLS já tinha
escrito — sem essa segunda metade a recusa seria decorativa, porque a visita
seguinte, sem link para conferir, entraria calada no servidor recusado. Contra
um Dogma já fixado, um convite que discorda **avisa** e não derruba: o TOFU já
provou que é o servidor de ontem, e trancar alguém para fora por causa de um
link velho seria o erro oposto. As duas cascas leem o mesmo veredito; o `plug`
não compara mais nada por conta própria.

**A segunda ponta, do mesmo fio, também fechou.** O `Session::convite` morre
com a sessão que ele abriu e é descartado quando o endereço no campo não é o do
convite. Enquanto nada era conferido isso era inerte; deixou de ser no mesmo
dia em que a conferência passou a existir.

**Como se sabe que não é enfeite.** `crates/seele-conformance/tests/convite.rs`
prova os três desfechos contra um Dogma de verdade — a impressão certa
verificando, a errada recusando e desfixando, e o link velho avisando sobre uma
sessão que continua falando. Cada um foi visto ficar vermelho com a política
desligada antes de ser dado por bom.

**O que sobrou, e é de outra entrada.** A faixa de veredito da janela nunca foi
desenhada para um humano — é a mesma ausência da pendência 13, e é lá que ela
está contada.

## 13 · As três telas novas do app nunca foram vistas por ninguém

**Sintoma.** Não há sintoma relatado, e é justamente esse o problema. Três
regiões da janela — a lista de Dogmas visitados na tela de entrada, a faixa de
veredito de identidade que a sessão acende, e a barra de busca com o contador
`[n/m]` — foram escritas, testadas por fora e **nunca desenhadas para um
humano**. O ambiente onde este ramo foi feito não consegue capturar tela, e
`cargo tauri dev` abre uma janela que ninguém está lá para olhar.

**O que se sabe.** O que dá para afirmar sem ver está afirmado, e não é pouco:
`apps/seele-app/tests/frontend.rs` amarra cada `invoke` a um comando registrado
e cada `$("id")` a um id que existe na página, `apps/seele-app/tests/tokens.rs`
recusa cor que não esteja nos tokens, e a aritmética do realce é conferida
sobre os mesmos tipos que atravessam a ponte. Nada disso alcança o que só o
olho alcança: se o contador cabe ao lado do campo em janela estreita, se a
lista de visitados empurra o formulário para fora da tela quando tem vinte
entradas, se o realce corrente se distingue do resto de verdade e não só no
papel, se a faixa de veredito aparece onde alguém vai ler.

**O que ficou tentado.** Nada — e a ausência é o registro. `specs/06-clientes-gui.md`
já descreve as três como prontas, e é essa diferença entre "descrito" e "visto"
que esta entrada existe para não deixar passar calada.

**Por que não foi resolvido.** Precisa de uma máquina com tela e de alguém na
frente dela. Não é tarefa de código, e fingir que um teste de texto substitui
isso seria trocar a verificação pelo seu retrato.

**Quando dói.** Na primeira vez que alguém abrir o app depois de M5 — que é
tarde demais para descobrir que uma das três está torta. Ver
`docs/teste-duas-maquinas.md`, que é o roteiro onde este passo cabe.

## 14 · A janela reemite caractere em casamentos sobrepostos

**Sintoma.** No app (GUI), buscar um termo cujas ocorrências se sobrepõem —
"aa" em "aaa" — não só realça errado: o corpo da mensagem sai reescrito.
`occurrences` devolve `(0,2)` e `(1,3)`; `corpoComRealce`
(`apps/seele-app/ui/seele.js`) desenha os dois intervalos sem descontar a
sobreposição, e o caractere do índice 1 sai dentro de dois `<mark>` — "aaa"
que a pessoa escreveu aparece como "aaaa" na tela. É texto do usuário saindo
errado, não só a cor do realce.

**O que se sabe.** O terminal já resolve o mesmo caso. `ui.rs` (linhas
546-553) guarda `if start < cursor { continue; }` depois de já ter lido
`ordinal` de `*seen`, então pula só o desenho e mantém a contagem certa.
`corpoComRealce` não tem guarda equivalente nenhuma.

**O que ficou tentado.** Nenhum conserto nesta passada. Um registro anterior
(`.superpowers/sdd/2026-08-10-navegacao-gui-tui/final-fix-report.md`, achado
5) deu como motivo que consertar mexeria na numeração dos ordinais que o
realce corrente (`I1`) acabou de amarrar — **esse motivo está errado**. O
ordinal de `corpoComRealce` vem do índice do `.entries()` sobre `intervalos`,
não de um contador manual que a sobreposição pudesse atrapalhar; um
`continue` cedo, logo depois de ler `ordinal`, deixaria a numeração
exatamente como está — a mesma forma que `ui.rs` já usa do outro lado. O
conserto em si é barato.

**Por que não foi resolvido.** O que falta não é a dificuldade do conserto, é
prová-lo. `apps/seele-app/tests/frontend.rs` só confere o script como texto —
nomes de comando existentes, ids que aparecem na página — porque o projeto
não tem runtime de JavaScript no conjunto de testes: não há como executar
`corpoComRealce` e afirmar sobre o DOM que ela produz sem abrir uma janela.
Trocar o comportamento de um caminho que desenha toda mensagem, sem forma de
provar que o resultado bate, é um risco diferente de escrever o `continue`
em si.

**Quando dói.** Buscar um termo curto que se repete dentro de si mesmo — "aa",
"ll", "oo" — num corpo que o contém sobreposto. Raro, e visível assim que
acontece: a mensagem na tela deixa de ser a mensagem que a pessoa escreveu.
