# Pendências conhecidas

O que está quebrado ou frouxo e ainda não foi resolvido. Ordenado por quanto
atrapalha na prática, não por dificuldade.

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

## 4 · Não há limitação de taxa

`DisconnectReason::RateLimited` existe no protocolo e **nunca é enviado**. Um
convidado legítimo pode inundar o Dogma de mensagens ou de handshakes.

Não atrapalha rede local. **Bloqueia expor à internet**, e é a dívida mais
séria de segurança depois do ADR 0021.

## 5 · Apelido é validado só por tamanho

Trinta e dois bytes, e nada sobre o conteúdo. O terminal está protegido — o
ratatui filtra todo caractere de controle, verificado — e o app usa
`textContent`. Sobra a possibilidade de sósia: caracteres de direção invertida
ou parecidos com os de outra pessoa no roster.

Baixo impacto num Dogma de amigos, real num aberto.

## 6 · A matriz de três SOs nunca foi verde por inteiro

Linux e Windows compilam no CI, mas ninguém rodou o `plug` neles fora disso.
`docs/teste-duas-maquinas.md` é o roteiro.

## 7 · Sem troca de chaves pós-quântica

Ao tirar o `aws-lc-rs` da árvore (para não exigir CMake e NASM no Windows)
perdeu-se o `prefer-post-quantum` do rustls. Nada protege contra gravar hoje e
decifrar depois. Aceitável para v1 — o modelo é TOFU sobre TLS 1.3 e E2EE de
mídia já é pós-v1 — mas é perda real.

## 8 · `:conectar` não reconecta em execução

O comando existe e avisa que não faz. Reconectar exige derrubar uma conexão
QUIC viva e uma thread de áudio; reiniciar o processo faz isso certo.

## 9 · O esquema `seele://` não é clicável

Não está registrado no sistema operacional. Quando for, o cliente **precisa
perguntar antes de conectar**: um link que inicia conexão sozinho é superfície
nova. Ver ADR 0006.
