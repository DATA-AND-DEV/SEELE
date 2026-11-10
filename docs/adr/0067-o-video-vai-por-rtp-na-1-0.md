# 0067 — Na 1.0 o vídeo da tela e da câmera vai por RTP, e a sala passa a carregar quadros

Status: **aceito** · decidido por quem opera em 10/11/2026, depois da medida
Data: 2026-11-10
Substitui em parte: [0066](0066-a-tela-fica-no-canal-confiavel-na-1-0.md) (a parte do canal; a do base64 continua valendo)

## Contexto

O 0066 manteve o vídeo no canal confiável `u` e deixou o RTP para depois da 1.0. Depois dele o produto de verdade foi medido sob perda (`docs/medidas/2026-11-10-tela-do-produto-sob-perda.md`), cinco defeitos de controle de taxa foram consertados, e o que sobrou a 3% e 5% de perda é do transporte: 25% e 47% do tempo com a imagem parada em regime, a 234–268 kbps e 200 kbps.

O RTP foi então medido no mesmo emulador (`docs/medidas/2026-11-10-rtp-sob-perda.md`): a 3% e 5% de perda, atraso mediano de 55 e 93 ms com 0% congelado em regime, a 2,37 Mbps; e a voz deixa de sentir o vídeo (p95 de 34 ms contra 1,5 s). Quem opera decidiu: **a 1.0 tem de ser um produto viável, e o vídeo por RTP entra nela.**

A medida do RTP é um salto só, entre dois `Rtc` com SDP em memória. O produto não tem SDP (ADR 0063), tem um servidor que repassa para vários espectadores, um cliente que serve tela a outro, e uma página de navegador. Este ADR decide como o RTP entra nisso.

## Decisão

### 1. A sala carrega quadros, e cada perna escolhe o transporte

Hoje o servidor repassa **bytes às cegas** de um canal `u` para outro. Passa a repassar **quadros**: a transmissão de uma pessoa é uma sequência de quadros (chave, comum, som), e cada perna — a de quem transmite e a de cada espectador — leva esses quadros pelo transporte que negociou:

- **perna RTP** (protocolo 14 em diante, cliente nativo): o `Rtc` da perna desempacota e empacota. O servidor lê o quadro inteiro de `Event::MediaData` de quem transmite e escreve com `writer().write()` no `Rtc` de cada espectador. NACK, RTX e estimativa de banda são **de cada perna**: a perda no caminho de um espectador não custa nada aos outros.
- **perna de canal** (protocolo 13 ou menos, e o navegador enquanto não mudar): o formato de hoje, `[tipo|len]` + corpo num canal `u`, com a fila por espectador e o corte de hoje.

O servidor é quem traduz. Uma transmissão que chega por canal sai por RTP para quem fala 14, e o contrário. **Nenhuma combinação de versões fica sem imagem.**

Não é um repasse de pacotes RTP (SFU puro). O repasse por quadro custa ao servidor a espera de reordenação da perna de entrada, e em troca dá a tradução entre transportes, que é o que deixa a 1.0 sair com clientes antigos e navegador no ar.

### 2. Sem SDP: as duas pontas declaram a mesma mídia

O enlace continua sem SDP. Depois do DTLS, as duas pontas de uma sessão em protocolo 14 declaram, pela API direta do `str0m`, **a mesma tabela fixa**:

- uma mídia de vídeo H.264 e uma de áudio Opus (o som da tela) em cada sentido, com `mid`, SSRC, SSRC de RTX e tipos de carga **constantes do protocolo**, escritos em `seele-proto`;
- as extensões de cabeçalho na ordem padrão do `str0m`, com o retorno de TWCC ligado.

A tabela é parte do protocolo 14: mudar um número dela é mudar de versão. Uma pessoa transmite uma coisa por vez (tela **ou** câmera), então uma mídia de vídeo por sentido basta; o `ScreenId`, a fonte e as medidas, que hoje vão no cabeçalho do canal, passam a ir numa mensagem de controle antes do primeiro quadro.

A triagem do soquete (`ponta.rs`) passa a entregar ao `Rtc` também os bytes de RTP e RTCP (128–191, RFC 7983), que hoje vão para a fila do ponto de encontro.

### 3. A taxa de quem transmite sai da estimativa do RTP

Em perna RTP, a «perna de quem transmite» do `TetoDeVideo` é a estimativa de banda do `str0m` (TWCC), e a `Sonda` por Ping fica só para servidores antigos. O resto do teto não muda: mínimo com a perna do hospedeiro e com a banda escolhida, piso de 200 kbps.

A espera de reordenação e o teto de retransmissão não ficam no padrão do `str0m` (2 s e 15%): a medida mostrou que são eles que custam as quebras a 5%. Os valores entram com medida do produto, não com a do protótipo.

### 4. O pedido de quadro-chave continua um só

O PLI de um espectador (`Event::KeyframeRequest` na perna dele) vira o mesmo `KeyFrameRequested` de hoje para quem transmite. Quem entra no meio de uma transmissão é ligado no próximo quadro-chave, como hoje, só que por quadro e não por contagem de bytes.

### 5. O que fica no canal nesta etapa

- **A página do navegador** (inclusive o celular, que só assiste). Ela codifica e decodifica com WebCodecs, e o navegador não deixa pôr quadro já codificado numa trilha RTP: levá-la ao RTP é trocar o caminho de imagem dela por trilhas de mídia. É a última etapa, e só entra na 1.0 se a medida do nativo confirmar o ganho com o servidor no meio.
- **A tela servida por um cliente a outro** (`par.rs`) e **a chamada navegador–navegador** (`rtc-chamada-web.js`).

Enquanto isso, quem assiste pelo navegador numa rede com perda continua com o comportamento de hoje **na perna dele**. O produto diz isso onde já diz o estado da transmissão; não finge.

## Ordem de entrega

Um commit por etapa, cada uma com seu teste:

1. enlace: RTP chega ao `Rtc`; mídia declarada sem SDP; API de quadro (`mandar`, `receber`, pedido de chave, estimativa) — provada entre duas `Ponta` pelo `Cano`;
2. protocolo 14 e a mensagem que abre a transmissão;
3. servidor: a sala por quadros, com as duas pernas e a tradução;
4. núcleo: emitir e receber por RTP, teto pela estimativa;
5. medida do produto de novo, nos mesmos cenários — é ela que diz se a etapa valeu;
6. página do navegador, se o item 5 confirmar.

## Consequências

- O servidor passa a ler o vídeo que repassa (os limites dos quadros, não o conteúdo). Um quadro malformado deixa de derrubar a transmissão por contagem de bytes e passa a ser um quadro recusado.
- O servidor gasta mais por espectador: empacotar, cifrar SRTP e guardar o cache de retransmissão de cada perna. Não foi medido; a etapa 5 mede.
- Um espectador lento deixa de ser **cortado**: a perna RTP descarta e pede chave. O corte continua existindo na perna de canal.
- A perda do caminho passa a ser um número que o servidor tem (RTCP), e a faixa de sinal deixa de ficar parada em 100.
- **O que a medida do protótipo não cobriu e este desenho assume**: a API direta do `str0m` para mídia (o protótipo usou SDP), o repasse por quadro com N espectadores, e a convivência do RTP com os canais SCTP no mesmo `Rtc` do produto. A etapa 1 prova a primeira; a 5, as outras duas. Se a etapa 1 falhar, este ADR volta à mesa antes de qualquer outra etapa.
