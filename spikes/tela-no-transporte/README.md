# spike `tela-no-transporte`

**Descartável.** Fora do workspace, como `device-latency`, `plug-cli` e
`voice-link`. Existe para responder **uma** pergunta antes de
`docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` escolher
um transporte para o vídeo. Nada pode depender dele.

## A pergunta

> Quando alguém compartilha a tela e a subida da casa não dá conta, **o que
> sobra da voz** — e o desenho do transporte muda essa resposta?

O produto já tem uma conexão QUIC por par: voz em datagramas, controle e texto
em fluxos (`specs/02-protocolo.md`). O vídeo pode ir num fluxo dessa mesma
conexão, em datagramas dela, ou numa segunda conexão. **A spec não consegue
escolher sozinha**, porque a diferença entre as três não está no RFC: está em
como o `quinn` 0.11 ordena o que sai dentro da janela de congestionamento, em
quanto ele deixa enfileirar antes de descartar, e em quanto de fila o gargalo
do caminho acumula.

### Por que esta pergunta, e não a do codec

O custo de CPU de um H.264 ou de um VP8 é consultável, varia com a máquina, e
erra para um lado que se percebe na hora: a tela fica ruim. Já a voz engasgando
**porque alguém compartilhou a tela** erra para o lado que ninguém percebe até
estar acontecendo, e transforma o recurso num prejuízo — a conversa era o
produto. Nenhum documento responde isso: ou se mede nesta pilha, ou se chuta.

## O que o binário monta

Um par QUIC inteiro dentro de um processo, com um **cano estreito no meio**:

```text
cliente ──▶ [ banda fixa · fila com teto · atraso ] ──▶ servidor
        ◀────────── só atraso, sem estreitar ──────────
```

Só a **subida** é estreitada: numa casa é ela que aperta, e compartilhar tela é
subida quase pura. A fila do cano descarta pela cauda quando enche, que é o que
um roteador de casa faz.

As duas pontas moram no mesmo processo **de propósito**: assim leem o mesmo
relógio, e o atraso de ponta a ponta de cada quadro de voz é medido, não
estimado por metade do RTT.

**Não há codec aqui, e é decisão.** A carga tem forma de vídeo — 30 quadros por
segundo, quadro-chave a cada dois segundos com cinco vezes o tamanho, bitrate
alvo acima do que o cano aguenta — e é isso que o QUIC vê. Um encoder de
verdade no meio acrescentaria uma variável que não está sob prova.

## Como rodar

```text
cargo run --release                          # a matriz inteira
cargo run --release -- --segundos 20         # amostra maior
cargo run --release -- --modo folga          # um cenário (casa com o nome)
cargo run --release -- --banda-kbps 1000 --fila-kib 128 --atraso-ms 40
```

## A resposta

Caminho: subida 2000 kbps, fila 64 KiB (262 ms de enfileiramento cheio), 20 ms
de atraso por sentido, 20 s por cenário, vídeo pedindo 4000 kbps salvo onde
escrito. `voz sozinha` é a linha de base.

```text
cenario                       env    rec   perda   p50 ms   p95 ms   p99 ms   pior ms video kbps
voz sozinha                  1000   1000   0.00%     21.7     22.9     23.3      25.7          0
fluxo, cubic                 1000    999   0.10%    225.7    258.3    260.8     265.1       2030
fluxo, bbr                   1000   1000   0.00%    145.6    152.1    348.6     548.7       1979
folga 60%, cubic             1000   1000   0.00%     23.1     78.9     99.2     114.9       1280
folga 60%, chave espalhada   1000   1000   0.00%     22.2     35.8     36.7      42.7       1200
datagrama, buffer 1MiB       1000    839  16.10%   2161.4   2203.1   2411.7    2573.1       1981
datagrama, buffer 32KiB      1000     19  98.10%    269.4    314.5    315.5     315.5       1970
segunda conexao, cubic       1000   1000   0.00%    221.8    253.8    256.8     258.7       1931
```

Uma segunda corrida de 20 s deu as mesmas conclusões com os mesmos números até
a casa das unidades — 222,0 / 219,0 / 28,1 / 16,6% / 97,4% —, salvo o p95 da
linha do quadro-chave espalhado, que veio 42,1 ms em vez de 35,8 ms. A ordem
das linhas não mudou em nada.

Em uma frase: **o vídeo vai num fluxo da mesma conexão, e o que protege a voz
não é o transporte — é o teto de bitrate.**

### 1 · Datagrama para vídeo é o pior desenho possível

`send_datagram` do `quinn` põe voz e vídeo na **mesma fila FIFO**, e quando ela
enche descarta o **mais velho** (`quinn-proto`,
`connection/datagrams.rs::send`). Com o buffer padrão de 1 MiB isso são ~4 s de
fila a 2000 kbps: **16,1% da voz perdida e 2,16 s de atraso**. Encolher o buffer
para 32 KiB não conserta, inverte — os pedaços de vídeo enchem a fila entre dois
quadros de voz e **98,1% da voz é descartada antes de sair da máquina**.

O problema não é o QUIC: é pôr duas prioridades numa fila só.

### 2 · Fluxo na mesma conexão: a voz não se perde, mas espera

Com o vídeo num fluxo, a perda de voz fica em 0,1%. Isso não é sorte: o
`quinn-proto` escreve os quadros `DATAGRAM` **antes** dos `STREAM` em cada
pacote (`populate_packet`), então a voz ganha a janela de congestionamento do
vídeo. O que a prioridade **não** resolve é a fila que o vídeo cria no gargalo,
que é fora da máquina: o atraso da voz vai de 21,7 ms para **225,7 ms**, ou
seja, a fila de 262 ms do cano praticamente cheia o tempo todo.

Prioridade dentro do QUIC não vale nada contra bufferbloat no meio do caminho.

### 3 · A segunda conexão não protege ninguém

221,8 ms — a mesma coisa que uma conexão só. Duas conexões QUIC competem no
**mesmo gargalo**, e a segunda apenas ganha o próprio controle de
congestionamento para encher a mesma fila. Custa um aperto de mão, um par de
chaves e um estado a mais, e devolve 4 ms.

### 4 · O que funciona é teto de banda, e o número é ~60%

Com o vídeo pedindo 1200 kbps num caminho de 2000, a voz volta para **23,1 ms de
p50 e 0% de perda**. É a única linha da tabela que fica perto da linha de base.

### 5 · O quadro-chave é metade do que sobrou

Mesmo com folga, o pior caso era 114,9 ms — e a rajada do quadro-chave (5× o
quadro comum, a cada 2 s) responde por quase tudo: espalhando esse mesmo
quadro-chave em vez de mandá-lo de uma vez, o p95 cai de **78,9 ms para 35,8 ms**
e o pior caso de **114,9 ms para 42,7 ms**, com o mesmo bitrate entregue.

### 6 · Cubic ou BBR: troca mediana por cauda, e não decide nada

BBR corta a mediana quase pela metade (225,7 → 145,6 ms) e **dobra a cauda**
(p99 260,8 → 348,6; pior 265,1 → 548,7). Para voz a cauda é que dói. E com o
teto de banda do item 4 a diferença some.

Numa corrida curta ele fica pior ainda — com `--segundos 6` a linha do BBR deu
6% de perda de voz, contra 0% do Cubic —, porque a rampa de sondagem dele ocupa
uma fatia grande de uma janela pequena. Não é ruído: é o que uma chamada de dois
minutos com a tela ligada e desligada várias vezes veria o tempo todo. O produto fica no Cubic, que é o
padrão do `quinn` — trocar o controle de congestionamento mexeria também nas
chamadas sem tela nenhuma.

## Um defeito de terceiro achado no caminho

**`quinn-proto` 0.11.17 aborta o processo no primeiro datagrama que estoura o
buffer de envio.** Em `connection/datagrams.rs::send`, o caminho de descarte
desconta `payload_bytes` duas vezes — `pop_front()` já desconta, e a linha
seguinte desconta de novo. O `usize` dá a volta, `memory_used()` fica gigante, o
laço esvazia a fila e o `expect` seguinte estoura:

```text
panicked at quinn-proto-0.11.17/src/connection/datagrams.rs:47:22:
datagrams.outgoing.payload_bytes desynchronized
```

Reproduz em menos de um segundo com `--modo "datagrama, buffer 32KiB"` na
0.11.17. A 0.11.16 — que é a que o `Cargo.lock` do produto trava — não tem o
defeito, e é por isso que o `Cargo.toml` daqui prende `quinn-proto = "=0.11.16"`.

Vale para o produto mesmo sem tela nenhuma: **este é o caminho que a voz usa**.
Basta o buffer de datagramas encher uma vez — uma subida que sumiu por dois
segundos — para o processo morrer em vez de perder quadros. Não subir o
`quinn-proto` sem conferir se isso foi consertado.

## O que este spike **não** responde

- **Nada sobre codec.** Nem CPU, nem qualidade, nem tamanho de quadro real.
- **Nada sobre captura.** Não abre tela nenhuma nem pede permissão a ninguém.
- **Nada sobre rede de verdade.** O cano é determinístico e simétrico no atraso;
  Wi-Fi ruim tem perda esporádica e atraso que anda sozinho, e nenhum dos dois
  está aqui. Os números são de **desenho**, não de campo — a mesma distinção que
  `docs/superpowers/specs/2026-08-21-portao-de-campo.md` faz.
- **Nada sobre mais de dois pares.** Uma tela para quatro pessoas é quatro vezes
  a subida, e essa conta não foi medida.
