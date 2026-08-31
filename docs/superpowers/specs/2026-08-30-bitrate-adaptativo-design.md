# Bitrate adaptativo — desenho, e o sinal que faltava

**Data:** 2026-08-30
**Estado:** aprovado em conversa; registrado no [ADR 0036](../../adr/0036-bitrate-adaptativo-em-faixas.md)

## Por que este documento existe

`specs/03-audio.md` fecha o parâmetro na linha 50:

> | Bitrate | 16–48 kbps, adaptativo |

E detalha na linha 55:

> Bitrate adaptativo reage à taxa de perda: cai para 16 kbps sob perda > 5%, sobe
> de volta gradualmente.

**Isso nunca foi construído.** `Controls::bitrate` é escrito uma vez, na
construção de `Voice`, e nada no repositório inteiro o escreve de novo. O
encoder roda a 32 kbps fixos do primeiro quadro ao último, em LAN limpa e em
enlace ruim igualmente.

Duas afirmações menores também estão erradas no código de hoje, e saem junto:

- `MAX_BITRATE_BPS` vale 64 000, contra os 48 kbps que a spec declara. Código e
  spec se contradizem, e a spec é quem manda;
- `codec.rs` documenta a faixa como «o que `specs/03-audio.md` permite, **após o
  ADR 0010 tê-la estreitado**». O ADR 0010 trata de FEC in-band e não diz uma
  palavra sobre bitrate. A atribuição é falsa e foi lida como se fosse verdade.

## 0 · O que mudou desde o ADR 0010, e o que não mudou

O [ADR 0010](../../adr/0010-fec-do-opus.md) considerou a adaptação e a recusou
com dois motivos nomeados:

> **Adaptativo por perda medida.** Atraente e provavelmente o destino final, mas
> hoje está bloqueado por dois lados: não existe canal de realimentação
> receptor→emissor até M2, e o `shiguredo_opus` (ADR 0008) não expõe setter em
> tempo de execução.

**O primeiro caiu.** O M2 existe, a sessão tem tique de telemetria e o servidor
já fala com cada cliente por ele.

**O segundo não caiu, e é preciso dizê-lo.** Conferido no fonte do binding
(`shiguredo_opus-2026.2.0/src/lib.rs`, linha 628): `OPUS_SET_BITRATE_REQUEST` é
aplicado **dentro de `Encoder::new`**, num caminho privado. O tipo `Encoder` não
expõe método nenhum para mudar o bitrate depois. Trocar de bitrate continua
sendo reconstruir o encoder, exatamente como `VoiceEncoder::set_bitrate` faz
hoje — e o comentário dele já avisa o que isso custa:

> A rebuild resets the encoder's internal state, so the first frame after a
> change is encoded without the prediction history of the ones before it. At
> 20 ms that is one frame of slightly worse quality, which is why this is
> acceptable for a person changing a setting and **would not be** for an
> automatic congestion response adjusting bitrate every few seconds.

Este desenho não contorna essa objeção: **ele a respeita**. A seção 2 é a
consequência de ela continuar válida.

## 1 · O sinal — perda de subida, medida por quem recebe

### O número que existe hoje não serve, e não é por pouco

O `Telemetry::loss_fraction` que o servidor manda é, em `session.rs`:

```rust
let sent = stats.path.sent_packets.max(1) as f32;
let lost = stats.path.lost_packets as f32;
loss_fraction: (lost / sent).clamp(0.0, 1.0),
```

São as estatísticas de caminho do quinn para **aquela conexão vista do
servidor**. Ele é inadequado por duas razões que não se corrigem uma à outra:

1. **É a direção errada.** Mede o que o servidor mandou e se perdeu — o
   *download* de quem recebe. Baixar o bitrate do meu microfone porque o meu
   download está ruim é o oposto do que a spec pede. O que precisa encolher é o
   que **eu envio**, quando é o que eu envio que não chega.

2. **É cumulativo, e por isso não volta.** `lost / sent` desde o início da
   conexão é uma razão monótona no numerador: um trecho ruim de dez segundos
   fica no denominador para sempre e a razão só decai assintoticamente. «Sobe de
   volta gradualmente» é **aritmeticamente impossível** a partir dele.

### O número certo já passa debaixo do nariz do servidor

`VoiceRoom::forward` **já decodifica o cabeçalho de mídia** — precisa dele para
conferir que o `ssrc` não foi forjado. Com o cabeçalho na mão, `header.seq` está
disponível de graça.

Uma lacuna na sequência de um `ssrc`, numa janela deslizante, é a perda de
subida daquela pessoa, medida por quem recebe. É o ponto de vista certo: **só
quem recebe pode dizer o que não chegou.**

### Por que a lacuna de `seq` é perda, e nunca silêncio

Esta é a propriedade que faz a medida ser limpa em vez de aproximada, e ela já
está escrita no `voice.rs`:

> The timestamp counts elapsed samples whether or not anything goes out; the
> sequence counts only what does. That difference is what lets the receiver tell
> DTX silence from real loss — M1.9.

O DTX **não** incrementa `seq`. Quem cala não produz lacuna: produz ausência de
pacote com `seq` parado, e o próximo pacote continua de onde parou. Então toda
lacuna de `seq` é um pacote que saiu e não chegou. Não há ambiguidade a
desempatar, e não há heurística.

### O que isto respeita

Nada aqui decodifica payload. `specs/08-seguranca.md` proíbe o servidor tocar no
conteúdo, e a promessa de que E2EE é incremento e não reescrita depende disso. O
cabeçalho é claro e o payload é opaco — a medida vive inteira do lado claro. É a
mesma disciplina do encaminhamento por energia discutido em `specs/04`.

### Onde a medida mora e como ela viaja

- Um estimador por `Member`, dentro da `VoiceRoom`, alimentado em `forward`.
  Janela deslizante, para que a medida possa **descer** quando o enlace melhora;
- o valor sobe até a sessão daquela pessoa e volta para ela, e **só** para ela:
  a perda de subida de alguém não é assunto de mais ninguém, e difundi-la seria
  contar a todo mundo a qualidade da rede de cada um.

## 2 · O controlador — faixas, histerese e permanência

### Três faixas: 48, 32 e 16 kbps

Os extremos vêm da spec. O ponto do meio existe para que a queda de 48 sob
perda moderada não vá direto ao piso — cair ao fundo por 6% de perda gastaria
qualidade que o enlace ainda comportava.

### Por que faixas, e não uma curva contínua

**Porque cada mudança reconstrói o encoder.** É a restrição da seção 0, e ela é
o que desenha esta seção inteira. Uma malha contínua ajustando a cada medida
seria exatamente a «resposta automática de congestionamento ajustando bitrate a
cada poucos segundos» que o `codec.rs` chama de inaceitável — e teria razão.

Três faixas, com histerese e um tempo mínimo de permanência antes de subir,
tornam a mudança **rara**: um punhado por chamada, e nenhuma numa chamada em que
a rede não muda de regime. Um quadro sem histórico de predição, algumas vezes
por hora, é um custo que não se ouve.

Isso converte um bloqueio em restrição de projeto, e é a razão de este desenho
**não** reabrir o [ADR 0008](../../adr/0008-binding-opus.md). Trocar o binding
para ganhar um setter seria pagar uma dependência inteira por um custo que a
histerese já reduz a nada.

### A forma da malha

- **Descer é rápido.** Perda acima do limiar derruba a faixa na medida seguinte:
  quem está perdendo pacote já está sendo ouvido mal, e esperar para confirmar é
  esperar em cima do problema.
- **Subir é lento.** Só depois de a perda ficar abaixo do limiar de subida
  durante o tempo de permanência inteiro. É o «gradualmente» da spec.
- **Os dois limiares são diferentes**, e é isso que impede a oscilação. Um único
  limiar faria a faixa bater na fronteira e trocar sem parar, reconstruindo o
  encoder a cada vez — o pior de todos os mundos.

### Os números de partida

Estes são **ponto de partida, não conclusão**. Vêm da spec onde ela fala e de
aritmética explícita onde ela cala; a seção 7 diz o que os confirma.

| | valor | de onde vem |
|---|---|---|
| Faixas | 48 / 32 / 16 kbps | os dois extremos são da spec; o meio é o ponto médio |
| Limiar de descida | perda > 5% | `specs/03-audio.md` linha 55, textual |
| Limiar de subida | perda < 2% | histerese de 3 pontos, larga o bastante para que ruído de medida não atravesse os dois |
| Permanência antes de subir | 10 s | duas janelas inteiras: uma para medir, outra para confirmar |
| Janela de medida | 5 s | a 50 quadros/s são ~250 pacotes, e 5% de 250 são 12 — amostra grande o bastante para o limiar não ser decidido por dois pacotes |

A janela é o número que mais importa e o único cuja escolha é uma troca de
verdade: curta demais e a malha persegue ruído; longa demais e ela reage tarde.
Cinco segundos é onde a contagem fica estatisticamente honesta no limiar que a
spec escolheu — abaixo disso, «5%» vira uma frase sobre meia dúzia de pacotes.

### Onde ele mora

Em `seele-audio`, como função pura sobre uma sequência de medidas e um relógio
passado por parâmetro. `seele-core::voice` só o liga ao `Controls::bitrate` que
já existe e já é lido a cada volta do laço.

É a regra que o cabeçalho do `device.rs` cobra — «could this have gone in `rt`
instead?» — e a única forma de esta malha ter teste: histerese e permanência se
provam com um relógio falso e uma lista de medidas, e não se provam com duas
máquinas e uma rede ruim.

## 3 · O que entra no protocolo

Levar a perda de subida até quem fala exige um quadro que hoje não existe.

**`PROTOCOL_VERSION` sobe para 2, `COMPATIBILITY_WINDOW` fica em 1.**

O postcard indexa variante por posição, então acrescentar uma no fim do
`ServerMessage` é seguro para quem escreve e ilegível para um cliente que não a
conhece. A janela de compatibilidade é a maquinaria que existe para isto: um
cliente v1 continua conectando, não recebe o quadro novo, e roda no bitrate
fixo — que é exatamente o comportamento de hoje. Nada regride para ninguém.

O quadro é escrito **para uma sessão só**, e não difundido. Ver a seção 1.

## 4 · O que muda de valor, e por que não é arbitrário

| | hoje | depois | de onde vem |
|---|---|---|---|
| `DEFAULT_BITRATE_BPS` | 32 000 | 48 000 | a faixa de cima; o controlador desce quando precisa |
| `MAX_BITRATE_BPS` | 64 000 | 48 000 | `specs/03-audio.md` linha 50 |
| `MIN_BITRATE_BPS` | 16 000 | 16 000 | inalterado |

O padrão passar para o teto é o que «adaptativo» implica: começa-se onde a
qualidade é melhor e desce-se sob evidência, em vez de começar no meio por
precaução e nunca subir. É também o que responde ao pedido de qualidade máxima
sem inventar um número que a spec não tenha.

`specs/03-audio.md` **não muda**. O código é que vai até ela.

## 5 · O que este desenho não faz

- **Não liga FEC.** O ADR 0010 segue valendo e a decisão dele é sobre outra
  coisa. Reavaliar FEC com o sinal de perda de subida na mão é trabalho próprio,
  e agora há com que medi-lo.
- **Não adapta nada além do bitrate.** Largura de banda do Opus, complexidade e
  VBR ficam como estão. Cada um é uma decisão com o seu próprio custo.
- **Não mede perda de descida por fonte.** O `JitterBuffer` já a tem por `ssrc`,
  do lado de quem escuta, e ela não alimenta esta malha: o que se controla aqui
  é o que sai desta máquina.

## 6 · Como se prova

- **A malha**, em `seele-audio`: relógio falso, sequência de medidas. Que desce
  na medida seguinte ao estouro; que **não** sobe antes da permanência; que uma
  medida oscilando em torno do limiar não produz troca de faixa nenhuma.
- **A medida**, em `seele-server`: que uma sequência sem lacuna dá zero; que uma
  lacuna de `seq` conta como perda; que **silêncio de DTX não conta como
  perda** — o teste que mais importa, porque é a confusão que tornaria a malha
  um gerador de ruído;
- **A janela**, que ela desce quando o enlace melhora. É a propriedade que o
  número cumulativo de hoje não tem, e a razão de haver medida nova.
- **A ponta a ponta**, em `seele-conformance`: com perda induzida acima do
  limiar, o bitrate relatado na telemetria cai; retirada a perda, ele volta a
  subir depois da permanência.

## 7 · Riscos

- **A janela é curta demais** e a malha persegue ruído; **longa demais** e ela
  reage tarde. O valor sai de medição em `netsim`, e não de palpite — os perfis
  do M1.7 já existem em `crates/seele-audio/src/netsim.rs`.
- **Uma sala grande multiplica a medida** por quem fala. O estimador é um punhado
  de inteiros por `Member` e não aloca; ainda assim é custo por participante, e
  entra na mesma conta do teto de sala.
- **Subir a versão de protocolo** exige que o cliente e o servidor de uma mesma
  casa não fiquem em versões diferentes por muito tempo. A janela cobre isso,
  mas o pipeline de release precisa estar são — e `docs/pendencias.md` registra
  que ele está quebrado desde 22/08.
