# 0027 — A reserva do anel de reprodução, e o que ela custa de latência

Status: aceito

Contexto: entre o laço de voz e o retorno de chamada do dispositivo há um anel de 100 ms (`RING_MS` em `crates/seele-core/src/voice.rs`). Ele nunca teve alvo nenhum: o laço empurra 48 000 amostras por segundo **de `Instant`**, o dispositivo consome no ritmo do cristal dele, e a diferença — dezenas de partes por milhão entre dois cristais quaisquer, e nada garante que sejam o mesmo relógio — só cabe ali dentro. Sem ninguém segurando, o anel **encosta** numa das duas paredes e fica lá, e qual delas depende só do sinal do erro. Encostado no fundo, o retorno de chamada inventa silêncio algumas amostras por segundo, para sempre; encostado no topo, a latência de reprodução é o anel inteiro — 100 ms —, para sempre. É a pendência 2.

Medido nesta máquina com `cargo run --release -p seele-audio --example ritmo -- --sem-malha`, que dá voltas com a forma do laço de voz contra o dispositivo de verdade: o fundo do anel foi **zero em todos os intervalos de dez segundos**, do começo ao fim. O anel raspa o fundo o tempo todo, e a perda sai quando o retorno de chamada calha de cair lá. Não havia reserva nenhuma, e ninguém tinha decidido que não haveria.

A linha "Dispositivo (captura + reprodução + conversores) — 19,6 ms" do ADR 0009 foi medida com o `examples/latencia.rs`, que empurra o clique e o consome na hora: naquela plataforma de medida o anel contribui com ~0. **O produto nunca entregou aqueles 19,6 ms** — ele entregava 19,6 mais o que o anel estivesse segurando, que era 0 ou 100 conforme o sinal de um cristal.

Decisão: o anel passa a ter **alvo**, e o alvo é **dois blocos do dispositivo**, segurado por reamostragem contínua (`crates/seele-audio/src/pacing.rs`, tarefa M1.8).

Dois blocos, e não um número em milissegundos, porque a grandeza é uma propriedade do dispositivo e não um palpite: o retorno de chamada leva o bloco dele **inteiro**, e com menos que um bloco no anel ele inventa o resto por mais cheio que o anel esteja em média. Um bloco é o mínimo aritmético; o segundo é a folga para a volta do laço atrasar, e a volta do laço atrasa — a pendência 15 mediu 5,65 ms de p50 e 22,44 ms de pior caso nesta máquina. Um dispositivo de 128 quadros e um de 2048 precisam de reservas dezesseis vezes diferentes, e nenhuma das duas é adivinhável de fora. `crates/seele-audio/src/rt.rs` passou a contar o maior bloco já pedido, que é de onde o alvo sai.

**O anel vazio é reposto com silêncio, de uma vez**, em vez de subir até o alvo no passo lento da malha — que custaria um minuto de perda contínua no arranque. Com o anel vazio o dispositivo já está inventando silêncio sozinho: o que se insere ali não custa nada que não estivesse sendo pago.

Consequências: **a reserva entra no orçamento de latência**. Neste Mac o bloco é de 512 quadros, então são 21,3 ms, e o piso do ADR 0009 vai de ≈ 67 ms para ≈ 88 ms — acima do portão de 70 ms que aquele ADR fixou para o aceite de M1 no nível piso.

Isso não é uma piora de 21 ms: é a substituição de um número que ninguém sabia por um que está escrito. Antes, a contribuição do anel era **0 ou 100 ms conforme o sinal de um cristal**, e o 0 vinha acompanhado de perda contínua. O que muda é que passa a haver um número, e que ele passa a ser uma decisão em vez de um acidente.

Duas dívidas ficam registradas aqui, e nenhuma das duas é deste trabalho:

1. **A linha "Dispositivo" do ADR 0009 precisa ser remedida com o laço rodando**, e não com a plataforma de medida que esvazia o anel. Enquanto isso não for feito, o ≈ 67 ms daquela tabela é o piso de um rig, não do produto.
2. **O aceite de M1 no nível piso precisa ser reencarado** à luz do número medido. Mexer no portão é decisão de outro ADR, com a medida na mão.

Alternativas:

- **Reserva de um bloco só.** O bloco é servido inteiro ou o resto é inventado, então uma reserva de exatamente um bloco não deixa nada para a volta do laço atrasar — e ela atrasa até 22 ms aqui. Seria escolher a latência sabendo que a perda volta.
- **Nenhum alvo, corrigindo só a deriva.** Não funciona, e o motivo é o que a medida mostrou: o anel já está no fundo. Correção de deriva **segura** um nível, ela não cria um. Sem alvo não há o que segurar.
- **Encurtar a volta do laço** (a alavanca da pendência 15: tirar a soneca do fim, medida em 3,39 ms de p50). Ela encolhe a parte da reserva que existe por causa do laço, não a que existe por causa do bloco do dispositivo — que é a que manda aqui. Continua valendo, por outros motivos, e não substitui esta decisão.
- **Descartar ou inserir quadro em vez de reamostrar.** Custa um estalo de 20 ms de cada vez. `crates/seele-audio/src/drift.rs` já tinha escrito por que não, para a outra deriva.
