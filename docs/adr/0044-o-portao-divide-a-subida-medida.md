# ADR 0044 — O portão divide a subida medida, e a medida sobrevive ao reinício

**Estado:** aceito
**Data:** 2026-09-05

O [ADR 0038](0038-o-teto-da-sala-e-contado-nao-declarado.md) escreveu, sobre a
voz, que *«a conta que decide é fechada e o servidor tem todas as parcelas»*. Na
tela, a conta era fechada e o servidor tinha **uma parcela errada**.

## Contexto

`teto_do_hospedeiro` é a primeira perna do §5.1 do desenho de compartilhamento
de tela — `caminho de quem hospeda × 60% ÷ N espectadores` —, e é a única que
decide **quem entra** numa transmissão. Ela dividia `caminho_do_server`, que é o
`caminho_bps` declarado ou, na falta dele, `CAMINHO_DO_SERVER_BPS`.

Três fatos que só juntos mostram o tamanho do problema:

1. **`caminho_bps` nasce `None` e nada o escreve.** Conferido por varredura
   sobre `crates`, `apps` e `xtask`: o campo existe na `ServerConfig`, tem doc,
   e nenhuma linha deste repositório o preenche. O divisor era sempre a
   hipótese de 2 Mbps das provas.
2. **A conta que sai dali é fechada.** 1,2 Mbps repartidos, piso de 200 kbps por
   cópia: **seis espectadores**. No sétimo, `cabe_mais_uma_copia` responde não.
   Em qualquer máquina, em qualquer internet — numa fibra de 500 Mbps, seis.
3. **A medida existia.** A `Subida` mede este cano a cada segundo desde
   `cb45bc1`, e o resultado ia para um lugar só: o `HostUplink`, que atravessa o
   fio para cada cliente fazer a conta dele. Ele passava por cima do portão a
   caminho da rede.

O produto media a perna certa e não a usava onde ela decide. É a forma que «o
produto sabe e não conta» toma quando o que ele sabe é um número.

E havia um segundo defeito atrás do primeiro: a `SondaDaSubida` só aprendia
enquanto a tela transmitia. Sem transmissão, `permitido_bps` é zero, `cheia` é
falso, e toda janela morria — então a primeira tela de uma conversa abria na
hipótese e subia 25% por janela, uns catorze segundos, com o portão recusando
espectador durante eles. Em todo arranque, porque nada era guardado.

## Decisão

**O portão divide a subida medida; a sonda aprende com qualquer janela que o
cano encheu; e a medida sobrevive ao reinício.** Três partes, e cada uma tem um
porquê que não é arrumação.

**1 · A sala lê o número, e não recebe cópias dele.** `VoiceRooms` guarda a
subida num `Arc<AtomicU32>` que cada sala partilha. O comando
`VoiceRoomCommand::Subida` continua existindo, mas só para **disparar** o
reconferir — o número nunca viaja nele. A razão é que o aviso usa `try_send`,
para não segurar todas as salas atrás de uma atolada, e uma cópia entregue por
mensagem pode ser descartada em silêncio: a sala ficaria dividindo um número
velho para sempre, e ninguém saberia. Um comando perdido atrasa o aperto; nunca
envelhece o número.

**2 · Nos dois sentidos.** A medida costuma ser maior que a hipótese, mas numa
casa com menos de 2 Mbps ela é menor. Um portão que só afrouxasse trocaria a
proteção do §3.2 por um número maior.

**3 · Uma janela que entregou mais do que a estimativa é uma janela cheia.** Se
o cano carregou mais bits do que se acreditava caber, o cano é pelo menos esse
tamanho — não é palpite, é o que a máquina comprovadamente empurrou. O
`bytes_enviados` desta sonda já soma todas as conexões, porque «o cano é um só».

Duas escolhas dentro deste braço, e as duas existem para a voz:

- **vai para o que passou, e nunca um passo além.** O `SUBIDA` de 25% é
  sondagem — pedir ao cano mais do que se sabe que ele dá. É legítimo quando
  quem pede é o vídeo dentro da própria licença; não é quando o que encheu a
  janela foi outra coisa.
- **a barra é mais alta que o `doeu` das outras janelas**, que é perda ≥
  `PERDA_QUE_DOI` e nada mais. O teto do vídeo é `FRACAO_DO_CAMINHO` da
  estimativa **por cima** do que a voz já gasta; levantá-la a partir de um cano
  que já está reclamando daria ao vídeo licença sobre bits que a voz está
  usando. Exige-se perda calma **e** nenhum evento de congestionamento novo —
  o que põe `eventos_de_congestionamento` para trabalhar, depois de existir sem
  decidir nada desde que `LeituraDaSubida` foi escrita.

**4 · A medida vai para o disco**, numa linha da tabela `configuracao`, pelo
critério com que a migração 2 a criou. A ordem do arranque passa a ser **medida
lembrada, declarado, hipótese** — a mesma de `caminho_no_fio`, e pela mesma
razão: uma foi conferida contra o cano, a outra foi digitada.

## Alternativas

- **Pôr uma tela de configuração para o operador declarar a subida.** O campo já
  existe e bastaria preenchê-lo. Recusada como resposta principal pelo motivo
  que o ADR 0038 dá contra o `GetCommonLinkProperties` do UPnP: número declarado
  é número que alguém digitou, e ele discorda do medido no dia em que a rede
  muda. Continua valendo como fundo, abaixo da medida.
- **Aprender também com transferência de anexo**, que enche o cano por
  construção. Recusada **por ora, e com pena**: uma transferência longa faz o
  controle de congestionamento procurar o limite, e procurar o limite é um
  evento de congestionamento — o guarda da parte 3 a bloqueia. Aproveitá-la
  exigiria saber que aquele tráfego é em massa e que ele para quando a tela
  começa, o que é estado que `crate::tela` não tem e `transfer.rs` teria de
  emprestar.
- **Amarrar a medida lembrada à rede em que foi medida.** É o que tiraria o
  risco da parte 4. Recusada por escopo, e o risco está registrado abaixo.
- **Fazer a sala perguntar a subida a cada decisão**, em vez de partilhar o
  átomo. Seria um `await` num caminho que hoje é síncrono, dentro de uma tarefa
  que o `specs/04` manda ser dona do próprio estado sem lock partilhado.

## Consequências

Numa fibra de 50 Mbps o teto de espectadores sai de seis para cerca de cento e
cinquenta, e com seis pessoas o teto por cópia sai de 200 kbps para 5 Mbps. Numa
casa de 2 Mbps nada muda, que é o comportamento correto.

Uma conversa que enche o cano ensina a subida **antes** da primeira tela, e o
arranque seguinte não reaprende.

**O risco aceito, e ele é real:** uma medida lembrada pode ser de outra rede — a
máquina mudou de casa, de Wi-Fi, de operadora. No cliente o número equivalente
governa o próprio vídeo de quem o mediu; aqui governa o teto de **todo mundo** na
sala, e uma memória alta demais numa rede que encolheu custa a voz da sala por
uma ou duas janelas, até o `doeu` derrubá-la.

`TETO_DA_SUBIDA_BPS` sobe de 50 Mbps para 1 Gbps, e a razão é a parte 3. O doc
antigo dizia que «o número importa menos do que parece», e era verdade enquanto
subir custava 25% por janela cheia: o teto quase nunca era alcançado, então
quase nunca cortava. Com o piso demonstrado ele corta na primeira janela — e
cinquenta megabits não descrevem mais uma máquina, já que uma fibra de 500/250
tem cinco vezes isso de subida. O que ele é não mudou: uma parede contra número
absurdo, nunca uma política sobre o tamanho das casas. Nada nele promete um
gigabit a ninguém; a estimativa só chega onde o cano comprovadamente levou.

## Custo de reverter

**Baixo.** Um braço na sonda, um campo atômico, um comando e uma linha de tabela
`configuracao`. Nada disso muda protocolo, nada atravessa o fio que já não
atravessasse, e nenhuma migração foi criada. Arrancar tudo devolve o
comportamento de antes: o portão dividindo a hipótese, e seis espectadores em
qualquer casa.
