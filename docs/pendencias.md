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

**A primeira suspeita era certa, e foi consertada.** Ela dizia: "o laço de
reprodução empurra um quadro de 20 ms por tica e recupera atraso somando 20 ms
ao alvo — se uma volta passar do prazo, ele não repõe o que ficou para trás".
Era isso mesmo. O conserto está contado na pendência 15, que é onde o mesmo
defeito ficou grande o bastante para ser ouvido; aqui ele só vazava devagar.

**Duas das outras suspeitas caíram, medidas.** Nesta máquina os dois
dispositivos rodam a 48 kHz e os dois conversores são passagem direta
(`cargo run -p seele-audio --example device_smoke`), então nem conversão de
taxa nem contagem de canais explicam o que se mediu aqui. Continuam de pé para
uma máquina cujo dispositivo **não** rode a 48 kHz — o `:audio` diz a taxa.

**O que sobrou, e é a explicação mais provável.** Deriva de relógio. O laço
produz 48 000 amostras por segundo de `Instant`; o dispositivo consome no ritmo
do cristal dele, e os dois não são o mesmo. As centenas de amostras por dezena
de segundos medidas acima dão algo da ordem de algumas centenas de partes por
milhão, que é a faixa em que dois relógios independentes vivem. O `drift.rs` já
documenta que a correção certa é **reamostrar** — `RateConverter::adjust_ratio`
existe e tem teste — e não descartar. Ligar isso ao anel de reprodução é a
tarefa M1.8, e é tarefa própria.

**Por que não foi resolvido agora.** O que faltava para medir já não falta: o
`:sync` mostra `LAÇO volta … · reposição … · anel …`, e o `anel` conta a outra
ponta — o que a mistura produziu e o dispositivo recusou, que também era um
`let _ =`. Falta a correção de deriva em si, que é reamostragem contínua guiada
pela profundidade do anel, e não um remendo.

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

## 4 · Estreitada em 2026-08-17 · Em CGNAT sem IPv6, ainda só na mesma rede

**O que era.** Fora da rede local, só funcionava com o anfitrião alcançável de
fora: VPS, porta encaminhada à mão, ou VPN. Atrás de um roteador doméstico não
conectava — e a mensagem não explicava isso, que era metade do problema.

**O que ficou no lugar.** Os degraus 2 e 3 do ADR 0022, em
`crates/seele-server/src/alcance.rs`. Ao hospedar, o SEELE sobe uma escada e
para no degrau mais alto que funcionar, sem que ninguém configure nada:

- **Degrau 3 — UPnP.** Pede a porta ao próprio roteador do anfitrião. Nenhum
  terceiro em lugar nenhum. Resolve boa parte das casas.
- **Degrau 2 — IPv6.** A escuta era `0.0.0.0`, que atende **só IPv4**: o Dogma
  não estava em IPv6 nem quando as duas pontas tinham. Agora é `[::]` com pilha
  dupla escrita à mão, e o cliente também deixou de ligar só em IPv4.
- **Degrau 1.** Continua sendo a resposta quando os dois de cima não dão.

O endereço que entra no `seele://` passa a ser o do degrau alcançado, e não
mais sempre o da rede local.

**O que ainda não tem saída.** **CGNAT sem IPv6 e sem UPnP.** Nesse caso o
roteador da casa abriria a porta de boa vontade, e o endereço dele também é
privado: não há para onde apontar. É o que o ADR 0022 já dizia que ficaria de
fora antes do degrau 4, e continua verdade. O degrau 4 — ponto de encontro —
não foi feito de propósito: ele custa uma decisão sobre metadado que o ADR quer
tomar em voz alta. O degrau 5, retransmissão, está fora de escopo por decisão.

**O que mudou é que agora isso é dito.** A escada não falha em silêncio: cada
recusa é uma variante nomeada com frase própria, ela aparece junto do link — e
não numa tela de diagnóstico —, e `docs/alcance-pela-internet.md` explica caso
a caso o que fazer. Um link que só funciona na rede de casa e um link que
funciona pela internet são o mesmo texto, e era isso que fazia o anfitrião
mandar o primeiro achando que mandou o segundo.

**Uma coisa que o ADR não previa.** Ele trata CGNAT como um caso em que UPnP
não funciona. Não é: o roteador atende o pedido e **abriria a porta com
sucesso**, na WAN dele, que não sai para a internet. Não é um erro que se possa
mostrar — é um sucesso mentiroso. Por isso o endereço externo é conferido antes
de mapear. Na primeira rede real em que rodou, era exatamente esse o caso.

## 5 · Fechada em 2026-08-15 · Não havia limitação de taxa

**O que era.** `DisconnectReason::RateLimited` existia no protocolo e **nunca
era enviado**. Um convidado legítimo podia inundar o Dogma de mensagens, e
qualquer um podia bater à porta em laço — cada tentativa comprando um Argon2id
inteiro de CPU do anfitrião, que o ADR 0021 escolheu caro de propósito.

**O que ficou no lugar.** Um balde de fichas, em `crates/seele-server/src/taxa.rs`,
consultado em três lugares. ADR 0025 conta as escolhas; em resumo:

- **Antes de autenticar**, por endereço de origem, no primeiro instante de cada
  conexão: trinta apertos de mão de rajada, trinta por minuto de reposição.
  Estourar responde `RateLimited` **com motivo**, antes de o `Hello` ser lido.
- **Depois de autenticar**, por conexão: sessenta quadros de controle de
  rajada, vinte por segundo. O primeiro excedente rende `AlertReason::RateLimited`
  — variante nova, porque derrubar calado é o que faz alguém achar que o
  produto quebrou —, os seguintes são descartados, e ao ducentésimo a conexão
  cai com `RateLimited`.
- **A mídia** já tinha limite, em janela fixa de um segundo; passou a usar o
  mesmo balde, e com isso perdeu a borda que deixava passar o dobro da taxa na
  virada da janela.

**Como se sabe que não é enfeite.**
`crates/seele-conformance/tests/limite_de_taxa.rs` cobra as duas pontas contra
um Dogma de verdade, e o mecanismo tem teste próprio com o tempo entrando por
parâmetro — nenhum `sleep`, nada dependendo de a máquina estar desocupada. Cada
teste foi visto reprovar com o código sabotado antes de ser dado por bom: o
balde que nunca esvazia, a portaria que sempre deixa passar, o vigia que só
passa, o que nunca avisa, o que nunca derruba, e o balde reescrito de volta
como janela fixa.

**O que continua de fora.** O balde é consultado depois de o QUIC ter feito o
aperto de mão TLS: quem só abre conexões e não fala ainda gasta uma assinatura
por tentativa. Fechar isso é `Incoming::refuse()` do quinn, antes de qualquer
cripto, e tem o custo de recusar sem conseguir dizer por quê. Fica anotado no
ADR 0025 como o degrau seguinte, para o dia em que um Dogma for de fato
inundado.

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
(`apps/seele-app/ui/tela-sessao.js`) desenha os dois intervalos sem descontar a
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

## 15 · Uma máquina ouve picotado e a outra não

**Sintoma.** Duas máquinas na mesma rede, o Mac hospedando o Dogma e o Windows
conectando. O Windows fala e o Mac ouve perfeitamente; o Mac fala e o Windows
ouve picotado. O texto atravessa inteiro nos dois sentidos, o tempo todo.

**O primeiro erro foi de leitura, e custou uma rodada.** A assimetria foi lida
como sendo do **sentido** — servidor → cliente falha, cliente → servidor não —
e daí se concluiu que o trecho suspeito era o servidor reenviando. Só que em
cada metade do teste **uma máquina só está reproduzindo**. "O Mac fala e o
Windows pica" e "a reprodução do Windows pica" produzem exatamente a mesma
observação, e a segunda leitura reabre o caminho de recepção inteiro, que a
primeira tinha descartado. Nada no relato distingue as duas.

**Medido.** Com `cargo run --release -p seele-core --example cadencia`, que dá
voltas com a mesma forma do laço de voz — sem microfone, sem rede, sem outra
máquina:

| | p50 | p99 | pior |
|---|---|---|---|
| volta do laço, neste Mac | 5,65 ms | 5,70 ms | 22,44 ms* |
| a mesma volta sem a soneca | 2,26 ms | 2,30 ms | 7,90 ms |

\* numa corrida com a máquina ocupada; numa ociosa o pior caso foi 5,80 ms.

**O defeito que isso encontrou.** O laço de voz conferia `if agora >= próximo`
e produzia **um** quadro de 20 ms. Isso se sustenta enquanto a volta durar bem
menos que 20 ms: cada volta entrega 20 ms de áudio gastando cinco de relógio, e
qualquer atraso é reposto. Assim que a volta passar de 20 ms, a mesma linha
vira vazamento permanente — 20 ms de áudio por volta, custando mais que 20 ms
de relógio —, e o anel de reprodução esvazia na diferença, para sempre.
Medido em teste: com uma volta de 31 ms saem **322 quadros onde o relógio pedia
499**, 64,5% do áudio, e os outros 35,5% saem como silêncio inventado pelo
retorno de chamada. É picotado, e nada no áudio recebido explica o buraco.

E a volta não dura o mesmo em toda parte. Ela é feita de duas esperas de
temporizador — `timeout(1 ms)` pela mídia e `sleep(2 ms)` no fim —, e cada uma
é arredondada para cima pela granularidade do temporizador do sistema antes de
somar. Onde essa granularidade é fina, 5,65 ms. Onde é grossa, dezenas.

**O que foi eliminado.**

- **Fragmentação de datagrama.** Era a hipótese principal, e ela é bonita: o
  texto vai em fluxo e se adapta ao caminho sozinho, a voz vai em datagrama e
  um datagrama que não cabe é recusado inteiro. A aritmética não fecha. Medido
  em `codec.rs`: o maior datagrama de voz que este build produz, com áudio de
  verdade no teto de bitrate, tem **272 bytes**. A RFC 9000 §14.1 exige 1200
  bytes de carga UDP no pacote Initial, então um caminho que não os entregue
  **não completa o aperto de mão** — não haveria texto atravessando para
  comparar. As duas metades do sintoma não podem ter a mesma causa. Está travado
  em teste, com folga de 3x, para o dia em que alguém aumentar o quadro.
- **Conversão de taxa neste Mac.** Os dois dispositivos rodam a 48 kHz e os dois
  conversores são passagem direta (`device_smoke`). Não foi eliminada do lado
  do Windows: o `:audio` diz a taxa daquela máquina, e ninguém olhou.

**O que ficou no lugar.** O laço passa a perguntar **quantos** quadros venceram
(`seele_audio::playout::PlayoutClock`) e produz todos, com teto de quatro para
não despejar uma hibernação inteira no anel. Isso torna a reprodução correta
independentemente de quanto a volta durar, que é a propriedade que faltava.

**E o que torna a próxima medição conclusiva.** Três coisas, e nenhuma delas
depende de hardware:

- `:sync` ganhou `LAÇO volta … · reposição … · reacerto … · recusa … · anel …`.
  `volta` acima de 20 ms diz, com número, que a máquina não acompanha o
  relógio pela via normal.
- um `tracing::warn!` uma vez por sessão quando a volta passa de um quadro.
- `examples/cadencia` roda em dez segundos, em qualquer máquina, e dá veredito.

**O que falta.** Rodar o `cadencia` no Windows. Se a volta de lá couber dentro
de um quadro, este diagnóstico está errado e a suspeita seguinte é a fila de
saída do servidor — o `cage.rs` conta `drops.subscriber_lagging` e **`drops()`
só é lido em teste**, então nada em produção mostra aquilo. Se a volta não
couber, a alavanca já está medida: tirar a soneca do fim do laço tira uma das
duas esperas de temporizador, e custou 3,39 ms de p50 aqui.

**Quando dói.** Sempre, em qualquer máquina cujo laço de voz não feche uma
volta em 20 ms — e a folga neste Mac era de 3,5x, não das dez que a forma
antiga supunha.

## 16 · A assinatura está pronta e não há credencial para ela

**Sintoma.** O SmartScreen mostra "O Windows protegeu o computador" e o
Gatekeeper diz que não consegue verificar se o app contém malware. Foi essa a
queixa que originou este trabalho: «Windows com erro com o controle inteligente,
precisamos urgentemente assegurar a confiabilidade do sistema».

**O que se sabe.** Não falta código. O `release.yml` já escreve o `signCommand`
do Azure Artifact Signing quando os três segredos existem, já instala a
ferramenta que assina, e já achava a identidade da Apple do mesmo jeito. O que
falta é comprar: uma conta paga da Apple, uma assinatura do Azure, e uma
validação de identidade que a Microsoft faz por gente e demora dias.

**O que ficou tentado.** ADR 0026 e `docs/assinatura-e-atualizacao.md`, que é o
passo a passo inteiro — de criar a conta a nomear cada segredo. Escrito porque
quem vai fazer isso é uma pessoa, uma vez, e não vai lembrar.

**Por que não foi resolvido.** Depende de cartão de crédito e de validação
humana; nenhuma das duas é trabalho de código.

**Quando dói.** Toda instalação. É o primeiro contato de quem baixa, e a frase
do macOS — a que oferece "Mover para o Lixo" — é a mais assustadora das três.

## 17 · Fechada em 2026-08-17 · O botão de atualizar existe em Rust e não tem tela

**Como fechou.** A tela existe: quinta seção do Terminal Dogma, `ATUALIZAÇÃO`.
Procurar não baixa, instalar instala o que a última procura mostrou, e nenhuma
das duas roda sozinha — o ADR 0026 pede as três coisas. O aviso que esta
pendência exigia está escrito antes do ato, com a parte que mais importa: se
houver um Dogma hospedado naquela janela, quem estiver dentro cai junto.

O andamento vem pelo canal `seele://atualizacao`. Quando o pacote traz `total`,
é barra com porcentagem; quando não traz, é travessão com o motivo no `title` —
a mesma resposta que a barra da bateria já dava para a mesma falta, em vez de uma
barra fingindo medir o que ninguém mediu. As seis variantes de `FalhaAoAtualizar`
têm frase própria, e duas delas dizem para **não** tentar de novo.

Os dois nomes saíram de `AGUARDANDO_TELA`, que foi o que o teste daquela lista
existia para cobrar.

**O que segue aberto e não é isto:** a pendência **16** — a assinatura ainda
espera credencial. Esta tela sabe atualizar; o que ela vai buscar só é confiável
depois que houver chave.

---

**Sintoma.** Não há como atualizar sem baixar o instalador de novo. Foi a segunda
queixa: «botão de atualizar para não precisar ficar baixando o exe no github toda
vez», e já custou um teste real — as duas máquinas ficaram em versões diferentes.

**O que se sabe.** A metade em Rust está pronta e testada de compilação:
`procurar_atualizacao` e `instalar_atualizacao`, em `apps/seele-app/src/main.rs`,
com o andamento do download saindo pelo canal `seele://atualizacao`. Os dois
nomes estão em `AGUARDANDO_TELA`, em `apps/seele-app/tests/frontend.rs`, e o
teste que lê essa lista falha no dia em que a tela chamar um deles — que é o
lembrete de tirá-lo de lá.

**O que ficou tentado.** Nada de interface: `apps/seele-app/ui/` estava sendo
reescrito por outro trabalho ao mesmo tempo, e desenhar por cima seria conflito
garantido.

**Por que não foi resolvido.** Falta a tela, e ela não é um botão qualquer:
`instalar_atualizacao` **fecha e reabre o SEELE** nos três sistemas, então quem a
desenhar tem um aviso a escrever antes — e, se houver um Dogma hospedado naquela
janela, dizer que quem estiver dentro dele cai junto.

**Quando dói.** Em toda versão nova, em toda máquina. E dói em silêncio: quem não
souber que saiu versão nova simplesmente continua na antiga.
