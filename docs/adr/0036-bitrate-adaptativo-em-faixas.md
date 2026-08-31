# ADR 0036 — Bitrate adaptativo em faixas, sobre perda de subida medida no servidor

**Estado:** aceito
**Data:** 2026-08-30

`specs/03-audio.md` fecha o parâmetro desde a primeira redação:

> | Bitrate | 16–48 kbps, adaptativo |
>
> Bitrate adaptativo reage à taxa de perda: cai para 16 kbps sob perda > 5%,
> sobe de volta gradualmente.

Nunca foi construído. `Controls::bitrate` é escrito uma vez, na construção de
`Voice`, e nada mais o escreve: o encoder roda a 32 kbps fixos do primeiro
quadro ao último. Este ADR registra o que se faz a respeito, e — mais
importante — **o que continua não sendo possível**, porque a metade que o
[ADR 0010](0010-fec-do-opus.md) usou para adiar isto ainda vale.

## Contexto

O ADR 0010 examinou a adaptação e a recusou nomeando dois bloqueios:

> **Adaptativo por perda medida.** Atraente e provavelmente o destino final, mas
> hoje está bloqueado por dois lados: não existe canal de realimentação
> receptor→emissor até M2, e o `shiguredo_opus` (ADR 0008) não expõe setter em
> tempo de execução.

**O primeiro caiu.** O M2 existe; a sessão tem tique de telemetria e o servidor
já fala com cada cliente por ele.

**O segundo não caiu**, e uma leitura apressada deste repositório concluiu o
contrário. Conferido no fonte do binding (`shiguredo_opus-2026.2.0`, linha 628):
`OPUS_SET_BITRATE_REQUEST` é aplicado **dentro de `Encoder::new`**, num caminho
privado, e o tipo `Encoder` não expõe nada para mudá-lo depois. Trocar de
bitrate é reconstruir o encoder — que é o que `VoiceEncoder::set_bitrate` já
faz, e o que o comentário dele já dizia ser inaceitável «para uma resposta
automática de congestionamento ajustando bitrate a cada poucos segundos».

Havia ainda um erro de atribuição no código: `codec.rs` documentava a faixa de
bitrate como «o que a spec permite, **após o ADR 0010 tê-la estreitado**». O ADR
0010 trata de FEC e não menciona bitrate. A frase é removida com este ADR.

## Decisão

**Bitrate adaptativo em três faixas — 48, 32 e 16 kbps —, com histerese e tempo
mínimo de permanência, comandado por perda de subida medida no servidor a partir
de lacunas de `seq`.**

Quatro partes, e cada uma tem um porquê que não é estilo:

**1 · O sinal é medido por quem recebe, e é novo.** O `loss_fraction` que
trafega hoje vem de `stats.path` do quinn: mede a direção **servidor→cliente** e
é **cumulativo desde o início da conexão**. Não serve por duas razões que não se
corrigem uma à outra — é o download de quem escuta, não a subida de quem fala; e
uma razão monótona nunca desce, o que torna «sobe de volta gradualmente»
aritmeticamente impossível. A medida nova é contagem de lacuna de `seq` por
`ssrc`, em janela deslizante, dentro da `VoiceRoom` — que já decodifica o
cabeçalho para conferir o `ssrc` e portanto já tem `seq` na mão.

**2 · Lacuna de `seq` é perda, e nunca silêncio.** O DTX não incrementa `seq` —
o carimbo de tempo conta amostras decorridas, a sequência conta pacotes
emitidos, e essa separação é a razão de M1.9 existir. Sem ela a medida seria uma
heurística; com ela é um fato. Nada aqui decodifica payload, então
`specs/08-seguranca.md` continua valendo e E2EE continua sendo incremento.

**3 · Faixas, e não curva contínua, porque o setter não existe.** Cada mudança
reconstrói o encoder e custa um quadro sem histórico de predição. Três faixas
com histerese e permanência tornam a troca rara — um punhado por chamada, e
nenhuma numa chamada cujo regime de rede não muda. A objeção do ADR 0010 não é
contornada: é respeitada, e vira a restrição que desenha a malha. Descer é
rápido (quem perde pacote já está sendo ouvido mal); subir é lento, atrás da
permanência inteira, que é o «gradualmente» da spec. Os dois limiares são
diferentes de propósito — um limiar único faria a faixa oscilar na fronteira e
reconstruir o encoder a cada volta.

**4 · A versão de protocolo sobe para 2**, com `COMPATIBILITY_WINDOW` em 1. O
quadro que leva a perda de subida a quem fala não existe, e o postcard indexa
variante por posição: acrescentar no fim é ilegível para um cliente que não a
conhece. A janela existe para isto — um cliente v1 conecta, não recebe o quadro
novo, e roda no bitrate fixo, que é o comportamento de hoje.

Os valores também são reconciliados com a spec: `MAX_BITRATE_BPS` cai de 64 000
para 48 000, e `DEFAULT_BITRATE_BPS` sobe de 32 000 para 48 000 — começar no
teto e descer sob evidência é o que «adaptativo» quer dizer.

## Alternativas

- **Trocar o binding do Opus para ganhar um setter em tempo de execução**,
  reabrindo o ADR 0008. Daria a curva contínua e uma malha mais fina. Recusada:
  paga-se uma dependência inteira — e a auditoria que o ADR 0008 fez dela — por
  um custo que a histerese já reduz a alguns quadros por hora.
- **Usar o `loss_fraction` que já trafega.** Não custaria protocolo nem medida
  nova. Recusada por estar errada nas duas dimensões que importam, direção e
  janela, como está escrito acima. Uma malha ligada a ele estrangularia o
  microfone de quem tem download ruim, e nunca voltaria a subir.
- **Medir a perda no cliente que escuta e devolvê-la ao que fala.** É o desenho
  clássico de RTCP e mede a coisa certa ponta a ponta. Recusado por agora: exige
  um caminho cliente→cliente que não existe — o servidor é o único ponto comum —
  e o servidor já ocupa o lugar de quem recebe, com o cabeçalho na mão.
- **Quatro faixas (48/32/24/16).** Descida mais suave, ao custo de mais
  reconstruções. Recusada porque a spec nomeia dois pontos e o terceiro já é
  interpolação; um quarto seria inventar precisão que ninguém mediu.

## Consequências

A qualidade em enlace bom sobe de 32 para 48 kbps, que é o pedido de «qualidade
máxima» atendido pelo número que a spec já tinha. Em enlace ruim a voz encolhe
em vez de picotar, que é o pedido de «sem cortes».

O custo é uma medida por participante dentro da `VoiceRoom` — um punhado de
inteiros por `Member`, sem alocação — e ela entra na mesma conta que o teto de
sala. E é um quadro de protocolo a mais por sessão, no tique que já existe.

Fica **registrado como dívida** que o FEC in-band do ADR 0010 merece nova
avaliação: ele foi recusado por falta de medida de perda em internet real, e a
medida passa a existir com este ADR.

## Custo de reverter

**Baixo no código, médio no protocolo.** A malha é uma função pura e um campo
atômico que já era lido; arrancá-la devolve o bitrate fixo. A versão de
protocolo é que não volta de graça: um cliente v2 já entregue a alguém espera o
quadro novo, e baixar a versão exigiria uma janela de compatibilidade ao
contrário. O momento de discordar da subida de versão é antes do primeiro
release que a carregue.
