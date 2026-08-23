# Compartilhamento de tela — desenho, e a prova de que a voz sobrevive

**Data:** 2026-08-22
**Estado:** aguardando plano
**Provas:** `spikes/tela-no-transporte/` (transporte) e `spikes/tela-no-codec/`
(codec)

O segundo dos dois ciclos que a conversa de 20/08 abriu. O primeiro
(`2026-08-20-conectividade-p2p-design.md`) fecha dizendo, na lista do que ficava
de fora: *«Compartilhamento de tela. Ciclo próprio, depois deste.»* Este é o
ciclo, e a dependência era real — sem caminho direto confiável não há onde
rodar, porque o servidor **não** vai encaminhar vídeo (§5).

O pedido, na palavra de quem pediu: *«um modo de compartilhamento de tela, onde
o usuário pode escolher transmitir um app, um monitor, etc, a la Discord.»*

## Por que desenho antes de código

**O projeto não tem nada de vídeo.** Nenhum codec, nenhuma dependência, nenhuma
mensagem de protocolo, nenhum byte de captura. O `Cargo.lock` inteiro não tem
uma linha de imagem em movimento, e `specs/02-protocolo.md` descreve o datagrama
de mídia como *«payload Opus de 20 ms»*, sem outra variante.

O que existe é o padrão desta casa para o que ainda não se sabe fazer: três
provas em `spikes/` — `device-latency`, `plug-cli`, `voice-link` —, cada uma
respondendo uma pergunta que um documento não respondia. Este ciclo traz a
quarta (`tela-no-transporte`, o §3) e a quinta (`tela-no-codec`, o §2 e o §5).

## 0 · O que este documento revoga, e o que ele não revoga

`specs/00-visao-geral.md` põe, entre os não-objetivos: *«Vídeo e
compartilhamento de tela»*. `specs/09-roadmap.md` repete no fim: *«E2EE de
mídia, áudio espacial, federação, gravação de sessão, vídeo. Listado apenas para
lembrar que **não** está em v1.»*

Este spec pede **metade** dessa linha, e diz qual metade:

- **sai do não-objetivo:** compartilhar a própria tela ou uma janela;
- **fica no não-objetivo, sem data:** câmera. Conteúdo diferente, ajuste de
  encoder diferente, permissão diferente, e uma superfície de interface que não
  existe. Um retângulo com o rosto de alguém não é um subproduto barato de
  compartilhar tela: é a segunda metade do trabalho, feita de novo.

Vale escrever a razão de revogar, porque ela não é «agora dá»: é que o ciclo de
conectividade entregou caminho direto entre casas, e compartilhar tela é o
primeiro recurso que **só** existe com ele. Sem P2P a decisão era teórica.

## 1 · Captura, por plataforma

Três APIs, uma por sistema, sem meio-termo: no macOS a ScreenCaptureKit é a
única porta desde que o `CGDisplayStream` foi depreciado; no Windows a Windows
Graphics Capture é a única que enxerga janelas com aceleração e composição; no
Linux, sob Wayland, **nenhum processo pode ler a tela** — quem lê é o
compositor, através do portal, e o que chega é um fluxo PipeWire.

### O que existe, e o que cada um arrasta

Contagens feitas com `cargo tree --edges normal --target <alvo>`, deduplicadas,
sem contar o próprio crate. «Já na árvore» é o `Cargo.lock` de hoje.

| alvo | crate | versão · licença · última mexida | crates novos | o que arrasta |
|---|---|---|---|---|
| macOS | `screencapturekit` | 8.0.1 · MIT/Apache-2.0 · 2026-07-18 | **10** | `apple-cf`, `apple-metal`, `doom-fish-utils`, `futures-util`, `libc` |
| Windows | `windows-capture` | 2.0.1 · MIT · 2026-08-08 | **31** | `windows` **0.62** ao lado do 0.61 já na árvore, mais `rayon` |
| Linux | `ashpd` + `pipewire` | 0.13.13 / 0.10.1 · MIT · 2026-07 e 2026-08 | **62 + 11** | a pilha `zbus`/`zvariant` inteira, que hoje **não existe** na árvore, e `libpipewire-0.3` como biblioteca **do sistema** |

Recusados, com o motivo:

- **`crabgrab`** (multiplataforma, 34 crates) — última publicação em
  **2024-06-14**. Dois anos parado numa API que a Apple mexeu duas vezes desde
  então. `deny.toml` não barra crate parado, mas o `[advisories]` dele já é uma
  lista de coisas herdadas que ninguém escolheu; não se acrescenta uma de
  propósito.
- **`scap`** (multiplataforma, 50–65 crates) — ainda `0.1.0-beta.1`, e é o maior
  acréscimo dos três alvos. Uma camada extra por cima das mesmas três APIs.
- **`xcap`** — mantido e popular, mas é biblioteca de **captura de imagem**; o
  próprio resumo dela diz que vídeo é *WIP*. Serve para uma foto da tela, não
  para trinta por segundo.

**Nenhum dos escolhidos puxa OpenSSL, e nenhum puxa um segundo runtime
assíncrono** — as duas coisas que o comentário de `[bans]` no `deny.toml` diz
esperar como primeiras entradas da lista de banidos. Confirmado por
`cargo tree`, não por leitura de README.

### As decisões, com o que custam

**macOS — `screencapturekit` 8.0.1.** Dez crates, licença dupla permissiva, e a
API certa. **O que custa:** ele traz uma pilha própria de FFI da Apple
(`apple-cf`, `apple-metal`) que **não** se junta ao `objc2` 0.6 que o Tauri já
põe na árvore — duas maneiras de falar com o mesmo sistema no mesmo binário — e
um dos dez é `doom-fish-utils`, utilitário pessoal do autor. Superfície de um
mantenedor só.

**Windows — `windows-capture` 2.0.1.** **O que custa:** ele pede `windows`
0.62, e o Tauri põe `windows` 0.61 — `multiple-versions = "warn"` no `deny.toml`
transforma isso em aviso, não em falha, mas são onze crates da família de
metadados duplicados, e é peso real no binário. Traz também `rayon`, ou seja um
segundo pool de threads ao lado do do tokio, **só no Windows**.
**A alternativa, nomeada:** chamar a WGC direto pelo `windows` 0.61 que já está
lá, acrescentando **zero** crates, ao preço de escrever o interop à mão — o que
exige `unsafe`, que é `forbid` no workspace, e portanto exigiria um crate com
exceção nomeada, como o `seele-ffi` e as bindings de áudio já têm. Não é para
v1; é a saída se a família duplicada incomodar.

**Linux — portal XDG (`ashpd`) + `pipewire`.** É o maior acréscimo do recurso
inteiro, e o único que muda uma propriedade do produto:
**o binário do Linux deixa de ser autocontido.** `pipewire-rs` liga contra o
`libpipewire-0.3` do sistema via `pkg-config` — biblioteca no build e `.so` na
execução — e o portal precisa de uma implementação viva no barramento da sessão.
Em qualquer desktop moderno as duas coisas estão lá (o PipeWire *é* o servidor
de áudio hoje), mas num contêiner ou numa máquina enxuta não estão, e o produto
tem de dizer isso com razão enumerada em vez de falhar.
**A alternativa, nomeada:** não compartilhar tela no Linux na v1, só receber.
Recusada — `specs/00` põe o produto exatamente entre quem vive em terminal, e
metade dessa gente está em Linux. Um recurso que não funciona justamente para o
público-alvo não é meio recurso, é nenhum.

### Uma regra que vale nos três

**A captura descarta, nunca enfileira.** Se o encoder não pegou o quadro
anterior, o novo substitui — não entra numa fila. Um quadro velho entregue tarde
é pior que um quadro perdido, e uma fila de quadros de 1080p come memória
depressa. É a mesma decisão que `specs/03-audio.md` já tomou para o áudio, pelo
mesmo motivo.

### Adendo — o Swift que o `screencapturekit` arrasta (22/08/2026)

Escrito ao construir, e não ao pesquisar. **A tabela acima contou dez crates e
não contou uma toolchain.**

O `build.rs` do `screencapturekit` roda **`swift build`**. Quem compila este
projeto no macOS passa a precisar de Swift na máquina, e não só de `cargo`. É o
mesmo formato de custo que o §2 recusou no nasm — com uma diferença que o salva:
na Apple o Swift vem com as ferramentas de linha de comando, que quem compila
aqui já tem instaladas. Não é uma ferramenta nova a instalar; é uma dependência
a mais para declarar.

E há um segundo custo, que quase passou por ser silencioso do jeito errado: o
crate publica o caminho do runtime por `cargo:rustc-link-arg`, e **o Cargo
ignora essa diretiva quando ela vem de uma dependência**. O sintoma é um
`cargo build` que **passa** e um binário de teste que não liga, reclamando de
`__swift_FORCE_LOAD_$_swiftCompatibility56`. Um `build` verde e um `test` que
nem chega a rodar é o formato de falha mais caro que existe, porque parece
sucesso.

A correção mora em `.cargo/config.toml`, que é o único lugar que o Cargo lê
para o pacote de cima, e está comentada lá. Ela precisa acompanhar o
empacotamento do `.app`.

**O que isto não muda:** a escolha do §1 continua de pé. `screencapturekit` é a
única porta desde que o `CGDisplayStream` foi depreciado, e a alternativa era
`unsafe`. O que muda é a conta — e a conta agora está escrita.

## 2 · Codec

### A conta que decide, e não é a que se espera

A pergunta óbvia é qualidade por bit. A pergunta que decide aqui é **o que o
codec faz com o build e com a licença**, porque este projeto já respondeu duas
vezes na mesma direção:

- o `rustls` entra com `default-features = false` e só `ring`, e o comentário
  diz por quê: os defaults trazem o `aws-lc-rs`, *«e ele exige CMake e NASM para
  compilar — num projeto cujo argumento inteiro é um binário auto-contido,
  arrastar uma segunda pilha de cripto em C que nunca é chamada é custo puro»*;
- o `tauri-plugin-updater` entra com `rustls-tls` porque *«a alternativa
  arrastaria OpenSSL»*.

**Todo codec de vídeo em C compilado a partir do fonte pede nasm ou yasm** para
o assembly de x86 — libvpx e OpenH264 igualmente. Sem ele o build sai, e sai
várias vezes mais lento no encode. Ou seja: ligar um codec estaticamente é
exatamente o custo que este projeto já recusou uma vez, e recusou por escrito.

### A decisão

**H.264 baseline, pelo `shiguredo_openh264` 2026.2.0, com o módulo binário do
Cisco carregado em tempo de execução.**

Quatro razões, na ordem em que pesam:

1. **Zero crates novos e zero ferramentas de build.** O `cargo tree` dele é uma
   linha: ele mesmo. Não compila C: gera as bindings com `bindgen` — que já está
   na árvore — e faz `dlopen`/`LoadLibraryW` da biblioteca do Cisco na execução.
   O build do projeto continua sendo `cargo build` nos três sistemas.
2. **É a mesma família da binding de Opus que o produto já usa.** O
   [ADR 0008](../../adr/0008-binding-opus.md) escolheu `shiguredo_opus` uma vez;
   este é o mesmo mantenedor, o mesmo trem de versões (`2026.2.0` nos dois), e o
   `rust-toolchain.toml` já carrega `llvm-tools` por causa dessa família.
3. **A patente fica respondida, não contornada.** O código do OpenH264 é
   BSD-2-Clause, mas isso não é o que resolve H.264: o que resolve é o Cisco
   pagar os royalties do pool **pelos binários que o Cisco distribui**. Compilar
   do fonte e embutir o resultado **descarta** essa cobertura. Carregar o binário
   do Cisco é o mecanismo, e é o que o Firefox faz desde 2013.
4. **É o codec que o outro lado fala.** No dia em que houver um navegador ou um
   telefone na conversa, H.264 é o que não precisa de tradução.

### O que ela custa, escrito por extenso

**O binário deixa de vir com codec, e não dá para embrulhar o codec junto.** A
cobertura do Cisco acompanha o binário que **o Cisco** entrega; redistribuí-lo
dentro do nosso `.deb`, do nosso `.app` ou do nosso instalador NSIS nos põe como
distribuidor e a cobertura não vem junto. Não é escolha de empacotamento — é a
licença. Consequências, todas obrigatórias:

- **uma busca de ~1 MB, uma vez, com consentimento na tela.** Num produto cujo
  argumento é que nada sai da sua máquina, isto tem de ser dito na cara: quem
  nunca compartilhar tela nunca baixa nada, e quem for compartilhar lê de onde
  vem e escolhe;
- **hash fixado e conferido**, com a mesma postura do
  [ADR 0026](../../adr/0026-duas-assinaturas-e-um-botao-de-atualizar.md). A
  máquina de baixar-e-verificar já existe no produto;
- **um motivo enumerado novo** — `specs/02-protocolo.md`: *«Todos os motivos de
  erro são enumerados»*. `ModuloDeVideoAusente` é estado normal, não erro de
  rede;
- **o botão de compartilhar não pode falhar.** Ou ele não está lá, ou ele
  explica o que falta. Um botão que erra depois do clique é o defeito que o
  `Info.plist` deste projeto já guarda uma vez.

### As alternativas, e por que não

- **VP8 (libvpx), estático.** Livre de royalties por concessão da Google,
  BSD-3-Clause, sem download nenhum: é o desenho mais limpo dos três, e seria a
  escolha se o custo de build fosse zero. Não é — nasm no Windows, o mesmo custo
  que o `rustls` recusou — e a binding é o elo fraco: `shiguredo_libvpx` está em
  `2026.2.0-canary.1`, pré-lançamento, e o `vpx-encode` não é publicado desde
  2022. **É o sucessor nomeado**, no dia em que a binding estabilizar e alguém
  aceitar pagar o nasm.
- **AV1 (`rav1e`), Rust puro.** Livre de royalties e sem C nenhum, mas o
  assembly dele também é nasm, e sem assembly não existe AV1 em tempo real numa
  máquina de conversa. É encoder de imagem parada e de offline, para o que
  precisamos.
- **Encoder do sistema (VideoToolbox, Media Foundation, VAAPI).** A melhor
  qualidade por watt, e a patente é problema de quem vendeu o sistema. São
  **três** integrações em vez de uma, com três conjuntos de defeitos, e o Linux
  fica sem — VAAPI depende de driver e de máquina. Fora da v1, e é o caminho
  óbvio da v2.
- **OpenH264 compilado do fonte (`openh264` 0.9.8, feature `source`).** Quatro
  crates, um binário só — e a questão de patente aberta num produto que assina e
  distribui binários. O `deny.toml` recusa uma licença mais forte sem discussão;
  deixar entrar uma dúvida de patente calada seria pior.

### CPU, e o que acontece numa máquina fraca

**O parágrafo abaixo foi escrito antes de haver medida, e a medida veio depois**
— está no fim desta seção e o desmente por mais de uma ordem de grandeza. Os dois
ficam aqui de propósito, o chute e o número lado a lado, porque a diferença entre
eles é o argumento inteiro para fazer o spike antes do plano.

> Não medido aqui, e este documento não vai fingir que foi. O que se sabe é a
> forma do problema: OpenH264 é baseline puro, feito para conversa em banda
> baixa, e encode por software de 1080p a 30 quadros é trabalho de núcleo
> inteiro numa máquina de escritório — mais em notebook fino que estrangula por
> calor.

Então a v1 não promete 1080p30. Ela promete o seguinte, que é o que dá para
sustentar:

- **a resolução segura, o quadro cede.** Entre 5 e 30 quadros por segundo,
  adaptativo. É a escolha certa para o conteúdo: quem compartilha tela está
  mostrando texto, e texto continua legível a 8 quadros e vira borrão no
  instante em que se reduz a resolução;
- **o encoder mora numa thread própria**, com prioridade abaixo do normal, e
  **nunca** no runtime que carrega os datagramas de voz nem perto do caminho de
  áudio. A prova do §3 mostra o que acontece quando a tela e a voz dividem
  qualquer fila; dividir a CPU é o mesmo erro com outro nome;
- **piso, e desistência com nome.** Se o encoder não sustenta nem o piso, o
  compartilhamento **para**, com motivo enumerado. Degradar para sempre é como
  um instrumento falso: consultado justamente quando algo deu errado.

**A prova foi feita em duas máquinas**, com a **mesma textura** nas duas:
`spikes/tela-no-codec`, num Apple M5 Pro e num AMD Ryzen 7 5800X3D com Windows,
alcançado por SSH. Textura idêntica porque a primeira corrida no Windows usou a
área de trabalho de 1024×768 que uma sessão sem monitor enxerga, e ela deu
números bonitos demais — 0% de descarte a 29 kbps. **Textura diferente é medida
diferente**, e comparar as duas teria sido comparar duas telas, não duas
máquinas.

| 1080p, uma fatia | Apple M5 Pro | Ryzen 5800X3D |
|---|---|---|
| quadros/s sustentados | 408 | 283 |
| núcleo a 30 quadros | 0,073 | **0,105** |
| descartados por falta de bits | 16,0% | 16,1% |
| kbps entregues | 1147 | 1145 |

O que ela mudou aqui:

- **A CPU não é o gargalo em nenhuma das duas, e não é nem perto.** 1080p a 30
  quadros custa **0,073 de um núcleo** no M5 Pro e **0,105** no Ryzen — o x86-64
  cobra 44% mais por quadro e ainda fica num décimo de núcleo. No núcleo de
  eficiência do Apple, com a prioridade de fundo que esta seção manda usar, são
  **0,18 a 0,21**. O parágrafo acima chutou «núcleo inteiro» e errou por mais de
  uma ordem de grandeza.
  **O que ainda falta:** o notebook fino que estrangula por calor, que é
  justamente a máquina que este parágrafo nomeia. As duas medidas são de máquina
  de mesa com dissipação sobrando, e nenhuma delas responde por essa.
- **O que aperta é o orçamento de bits**, e esta é a conclusão que a segunda
  máquina mais reforçou: no teto de 1200 kbps o controle de taxa do OpenH264
  descarta **16,0% dos quadros em 1080p no Mac e 16,1% no Ryzen**. Idêntico,
  porque quem descarta é o controle de taxa e não o processador — trocar de
  máquina não compra um quadro sequer.
  A faixa de 5 a 30 continua sendo o desenho certo — mas quem cede primeiro não
  é a CPU.
- **Uma fatia, uma thread.** Cortar o quadro em quatro fatias para usar quatro
  threads dá 2,4× de quadros por 2,5× de CPU — nenhuma eficiência — e sobe os
  quadros descartados de 16% para 24%, porque a predição não atravessa fatia.
  Numa máquina que já entrega dezesseis vezes o necessário, é qualidade jogada
  fora por latência que ninguém pediu.
- **O quadro-chave de 1080p custa 65 KiB e 8,4 ms**, quatro vezes um quadro
  comum, e 65 KiB são 446 ms do orçamento inteiro. O §3.3 tinha razão nas duas
  decisões, e agora com número.
- **Dois custos de build que a lista de razões acima não previu:** o `build.rs`
  do `shiguredo_openh264` faz `git clone` do repositório do Cisco para gerar as
  bindings — então o `cargo build` precisa de `git` e de **rede** —, e o
  `bindgen` precisa de `libclang`. Continua muito menos que nasm mais CMake, mas
  «zero ferramentas de build» está otimista por dois itens.
- **O `dlopen` do módulo do Cisco funcionou de primeira**, sem assinatura e sem
  `Entitlements`. A busca é de 471 KiB comprimidos, 1,15 MiB em disco.

## 3 · Transporte — a primeira parte que foi medida

O produto já tem tudo de que o vídeo precisa: uma conexão QUIC por par, com
datagramas para voz e fluxos para controle e texto (`specs/02-protocolo.md`).
A pergunta não era se cabe. Era **o que sobra da voz quando não cabe.**

`spikes/tela-no-transporte` monta um par QUIC inteiro num processo, com um cano
estreito no meio — banda fixa, fila com teto, descarte pela cauda —, estreitando
só a subida, porque numa casa é ela que aperta. As duas pontas leem o mesmo
relógio, então o atraso de ponta a ponta da voz é medido, não estimado.
**Nenhum codec:** a carga tem forma de vídeo (30 fps, quadro-chave 5× a cada
2 s, bitrate acima do que o cano aguenta), e é isso que o QUIC vê.

Caminho: subida 2000 kbps, fila 64 KiB (262 ms cheia), 20 ms de atraso por
sentido, 20 s por cenário, vídeo pedindo 4000 kbps salvo onde escrito.

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

Uma segunda corrida concordou linha por linha. O `README` do spike detalha.

### 3.1 · O vídeo vai num fluxo unidirecional QUIC, na conexão que já existe

E **não** em datagramas. `send_datagram` põe voz e vídeo na **mesma fila FIFO**,
que descarta o **mais velho** quando enche (`quinn-proto`,
`connection/datagrams.rs::send`). Com o buffer padrão de 1 MiB — que é o padrão
do `quinn` e o que o produto usa hoje, porque nunca o tocou — isso são ~4 s de
fila a 2000 kbps: **16,1% da voz perdida e 2,16 s de atraso.** Encolher para
32 KiB não conserta, inverte: **98,1% da voz descartada** antes de sair da
máquina, porque os pedaços de vídeo enchem a fila entre dois quadros de voz.

Com o vídeo num fluxo, a perda de voz é 0,1%, e não é sorte: o `quinn-proto`
escreve os quadros `DATAGRAM` **antes** dos `STREAM` em cada pacote
(`populate_packet`), então a voz ganha a janela de congestionamento do vídeo.

**E não numa segunda conexão.** 221,8 ms contra 225,8 ms de uma conexão só:
duas conexões QUIC competem no mesmo gargalo, e a segunda só ganha o próprio
controle de congestionamento para encher a mesma fila. Custa um aperto de mão,
um par de chaves e um estado a mais, e devolve 4 ms.

### 3.2 · O que protege a voz é o teto de banda, não o transporte

Este é o achado que muda o desenho. **A prioridade do QUIC evita a perda e não
faz nada contra o atraso**: no cenário `fluxo, cubic` a voz chega inteira e
chega com **225,7 ms** em vez de 21,7 — a fila de 262 ms do gargalo cheia o
tempo todo. A fila não está na nossa máquina; está no meio do caminho, e
prioridade de frame dentro do QUIC não a alcança.

O que alcança é não enchê-la. Com o vídeo pedindo 1200 kbps num caminho de 2000
— **60%** —, a voz volta para **23,1 ms de p50 e 0% de perda**.

Daí três regras, e as três são de produto, não de biblioteca:

1. **O vídeo tem teto, e o teto é uma fração do caminho medido**, não um valor
   fixo de configuração. 60% é o número que este spike sustenta; refinar exige
   campo.
2. **Quem mede o caminho é a voz, que já mede.** `specs/02-protocolo.md` já
   calcula RTT, jitter e perda por conexão e os transforma em **sinal**
   (ADR 0024). O teto do vídeo pendura nesse número que já existe: nada de um
   segundo medidor discordando do primeiro.
3. **A voz nunca cede à tela.** Quando o sinal cai de faixa, quem baixa é o
   vídeo; se continuar caindo, quem para é o vídeo. A frase é o critério de
   aceite do ciclo: *uma conversa com a tela travando é o produto funcionando;
   uma conversa picotando porque alguém abriu a tela é o produto quebrado.*

### 3.3 · O quadro-chave é metade do que sobra

Mesmo com teto, o pior caso era 114,9 ms — e a rajada do quadro-chave responde
por quase tudo. Espalhando o **mesmo** quadro-chave em vez de despejá-lo de uma
vez, o p95 cai de **78,9 para 35,8 ms** e o pior caso de **114,9 para 42,7 ms**,
com o mesmo bitrate entregue. Custo: nenhum. É só não mandar tudo num tique.

Então: **quadro-chave espalhado por alguns intervalos de quadro**, e
quadro-chave **sob demanda** (quando quem recebe pede) em vez de periódico —
numa conversa entre dois pares não há quem entre no meio da transmissão.

### 3.4 · Cubic fica

BBR corta a mediana quase pela metade (225,7 → 145,6 ms) e **dobra a cauda**
(pior caso 265,1 → 548,7 ms). Para voz a cauda é que dói, e com o teto do §3.2 a
diferença some. Trocar o controle de congestionamento mexeria também em toda
chamada sem tela nenhuma, o que é um custo grande por um ganho que o teto já
entrega. Fica o Cubic, que é o padrão do `quinn`. **Revisitar só com medição.**

### 3.5 · Um defeito de terceiro, e ele já ameaça a voz de hoje

O spike derrubou o processo antes de dar qualquer número, e a causa não era
dele: **`quinn-proto` 0.11.17 aborta o processo no primeiro datagrama que
estoura o buffer de envio.** Em `connection/datagrams.rs::send`, o caminho de
descarte desconta `payload_bytes` duas vezes — o `pop_front()` já desconta, e a
linha seguinte desconta de novo. O `usize` dá a volta, `memory_used()` fica
gigante, o laço esvazia a fila e o `expect` seguinte estoura.

O `Cargo.lock` do produto trava **0.11.16**, que não tem o defeito, e o spike se
prende nela para medir a mesma pilha que o produto usa.

**Isto vale independentemente de compartilhamento de tela:** é o caminho por
onde a voz sai hoje. Basta o buffer encher uma vez — uma subida que sumiu por
dois segundos — para o processo morrer em vez de perder quadros. **Não subir o
`quinn-proto` sem conferir se foi consertado**, e o dia em que subir, um teste
que encha o buffer de propósito.

### 3.6 · O que entra no protocolo

Mínimo, e no fluxo, nunca no datagrama:

- um fluxo unidirecional por transmissão, aberto por quem compartilha, com um
  cabeçalho de abertura dizendo o que é (monitor ou janela), a resolução e o
  codec — versionado pelo primeiro byte, como todo frame de controle;
- no controle, o começo e o fim da transmissão, e o pedido de quadro-chave;
- **`ssrc` não serve aqui.** Ele é o identificador de fonte de áudio atribuído
  na entrada da sala; a tela é outra coisa e ganha identificador próprio, para
  que ninguém precise reescrever a tabela de `ssrc` → pessoa.

## 4 · Escolher o que transmitir, e o que cada sistema cobra

«Um app ou um monitor» são a mesma escolha para nós e três coisas diferentes
para os três sistemas — inclusive **quem desenha a lista**.

| | macOS | Windows | Linux (Wayland) |
|---|---|---|---|
| quem lista o que existe | nós, com `SCShareableContent` | nós, ou o `GraphicsCapturePicker` do sistema | **o compositor**, e só ele |
| o SO pede consentimento? | **sim**, TCC, uma vez por app | **não**, nenhum | **sim**, a cada vez, salvo `restore_token` |
| janela isolada | sim | sim | sim, se o portal oferecer |
| monitor | sim | sim | sim |
| marca visível na tela de quem compartilha | indicador do sistema | **borda amarela, e ela fica** | a critério do compositor |

Três consequências que mudam a interface, e não são detalhe:

**No Linux não temos seletor.** Sob Wayland nenhum processo enumera janelas — o
portal abre o seletor **do compositor** e devolve só o que a pessoa escolheu.
Nossa tela de escolha, ali, é um botão que abre o seletor dos outros. Fingir uma
lista própria seria mentir sobre o que a gente sabe. O `restore_token` do portal
é o que evita perguntar de novo a cada transmissão, e ele **tem** de ser
guardado, ou o recurso vira uma caixa de diálogo por sessão.

**No Windows ninguém pergunta nada.** A WGC não tem prompt: o único consentimento
é o da nossa própria interface. Isso obriga o produto a ser mais explícito do que
o sistema exige — quem está compartilhando vê o que está saindo, e quem está na
sala vê quem está compartilhando. E **a borda amarela fica**: apagá-la exige
`GraphicsCaptureAccess.RequestAccessAsync(Borderless)`, que por sua vez exige a
capacidade `graphicsCaptureWithoutBorder` num **manifesto de pacote** — e o
`tauri.conf.json` deste projeto empacota com NSIS, sem identidade de pacote. A
borda é o preço, e não é ruim: ela diz a verdade.

**No macOS é TCC, como o microfone — e o roteiro é parecido, com uma diferença
que morde.** Ler `apps/seele-app/Info.plist` e `apps/seele-app/Entitlements.plist`
é obrigatório antes de mexer nisto: os dois carregam, no comentário, o defeito
que os produziu.

- **`Info.plist` ganha `NSScreenCaptureUsageDescription`.** Mesma forma que o
  `NSMicrophoneUsageDescription` que já está lá, mesmo motivo e mesmo estrago se
  faltar. O texto é para a pessoa que lê o alerta, não para o revisor de loja —
  a regra é do próprio comentário do arquivo, e vale de novo.
- **`Entitlements.plist` não ganha nada, e isto é a diferença.** Não existe
  direito de *hardened runtime* para gravação de tela: não há
  `com.apple.security.screen-capture` nem equivalente. A ScreenCaptureKit é
  guardada **só** pelo TCC. O `Entitlements.plist` está no repositório justamente
  para que o microfone não quebrasse de novo no dia da assinatura, e a leitura
  natural desse comentário é acrescentar um direito «por simetria». **Não
  acrescente.** Uma chave inventada não abre nada e suja um arquivo que hoje diz
  exatamente uma coisa verdadeira. Esta linha existe para impedir esse conserto.
- **O que quebra, e é diferente do microfone.** Sem a chave do microfone, o
  macOS nega **sem perguntar** — foi o defeito registrado. Com a tela o defeito
  é outro: o TCC guarda a permissão contra a **identidade assinada** do app, e
  um app não assinado que muda de binário perde a concessão a cada build. O
  sintoma é o pior possível — funcionou ontem, hoje não, e nada mudou no código.
  Conferir sempre com `CGPreflightScreenCaptureAccess` antes de oferecer o
  botão, e pedir com `CGRequestScreenCaptureAccess`, em vez de descobrir pelo
  fracasso.

**Isto não é verificável neste ambiente**, e o documento não vai fingir que é —
`docs/pendencias.md` já registra que a máquina onde este ramo roda não captura
tela e que há coisas que *«precisam de uma máquina com tela e de alguém na
frente dela»*. Vale igual aqui: a §4 é desenho, e o portão de campo dela é
`docs/superpowers/specs/2026-08-21-portao-de-campo.md` em espírito — três
máquinas, três sistemas, uma pessoa em cada.

## 5 · O que a pessoa escolhe, e por que é um teto

Decidido em 22/08/2026, a pedido de quem desenha o produto: **quem compartilha
controla o que sai da máquina dele.**

### Resolução não controla tráfego

A primeira coisa que a interface não pode deixar a pessoa acreditar. 1080p a
1 Mbps e 720p a 1 Mbps gastam **o mesmo**. O que a resolução muda é o detalhe:
no 1080p aquele megabit fica esticado sobre quatro vezes mais pixels, e o
resultado é borrão em movimento. Quem escolhe 720p não economiza banda — troca
tamanho por nitidez dentro do mesmo orçamento.

Por isso são **três controles**, e não um, cada um respondendo a uma pergunta
que a pessoa de fato tem:

| controle | responde a |
|---|---|
| teto de banda | «não quero que isso coma minha internet» |
| resolução | «o texto do meu editor está ilegível» |
| quadros por segundo | «estou mostrando um vídeo e está picotado» |

### Todos são teto, nenhum é piso

**A regra que não se negocia.** O que a pessoa escolhe é o **máximo**, e o
sistema continua livre para ficar abaixo.

Se a escolha virar piso, a regra de aceite do §3.2 cai — *a voz nunca cede à
tela* — e volta exatamente o que o spike mediu: 225 ms de atraso na voz contra
22 ms. Alguém escolhe 1080p60 numa subida de 2 Mbps, o vídeo insiste, e a
conversa fica impossível **por causa da tela**. Aí o produto fica pior com o
recurso do que sem ele.

Com teto, o comportamento continua o do §2: a resolução segura e o quadro cede,
entre 5 e 30 por segundo. A escolha da pessoa passa a ser o limite de cima
dessa faixa, e o teto automático de 60% do caminho medido continua por baixo,
como piso de proteção da voz.

Consequência de interface, e ela é obrigatória: **a tela não promete a escolha.**
Ela mostra o que está saindo agora ao lado do que foi pedido. Escolher 1080p e
receber 720p não é defeito; esconder que aconteceu, é.

### O seletor diz o que a escolha custa

O produto já mede o caminho — o sinal, o RTT, a perda (ADR 0024). Então as
opções não aparecem secas: cada uma vem com o que ela pede, contra o que esta
máquina mediu.

Onde não houve medida, `——`, como em todo o resto do produto. Uma opção com um
número inventado ao lado é pior que uma opção sem número.

### O que fica de fora

**«Qualidade da fonte»** — o *Source* do Discord. É a promessa que mais quebra
em rede doméstica, e o §3 já mostrou o formato do estrago.

### A lista, fechada em 22/08/2026 por `spikes/tela-no-codec`

Este documento deixou as resoluções e os quadros em aberto de propósito, porque
oferecer 1080p antes de medir seria oferecer uma opção que pode não ter CPU para
acontecer. Está medido, num Apple M5 Pro, com textura de tela capturada e o teto
de 1200 kbps do §3.2. O `README` do spike traz a tabela inteira.

**Resolução — três: 1080p, 720p e 540p.** O padrão é 720p.

- **1080p entra, e entra por medida:** 1080p a 30 quadros custa **0,06 de um
  núcleo** de desempenho, e **0,18 a 0,21** no núcleo de eficiência com a
  prioridade de fundo que o §2 manda usar. Ainda sobram quatro vezes. A frase
  do §2 — *«trabalho de núcleo inteiro numa máquina de escritório»* — não vale
  nesta máquina, e o que resta dela é só a cautela sobre as que faltam.
- **E entra com o que a medida também diz:** no teto de 1200 kbps, **o próprio
  controle de taxa do OpenH264 descarta 16% dos quadros** em 1080p para não
  estourar — contra 11% em 720p. Quem escolhe 1080p numa casa recebe 1080p a uns
  25 quadros, com o encoder decidindo sozinho quais cair. É legítimo, e é
  exatamente o caso para o qual a regra *«a tela não promete a escolha»* existe.
- **540p é o piso da lista, e o piso tem motivo.** Abaixo dele o encoder deixa
  de conseguir gastar o orçamento: 360p rende 416 kbps dos 1200 disponíveis.
  Descer mais torra nitidez sem devolver nada em troca — e quem quer gastar
  menos internet mexe no **teto de banda**, que é o controle desenhado para
  isso. Uma resolução mais baixa oferecida como economia seria a interface
  ensinando justamente a confusão que a primeira parte deste §5 desfaz.
- **Nada acima de 1080p**, como o §6 item 10 já dizia. A medida não mudou isso.

**Quadros por segundo — três: 30, 15 e 8.** O padrão é 30.

- **8 é o menor da lista, e não 5.** O 8 é o número que o §2 já nomeia — *«texto
  continua legível a 8 quadros»*. O 5 é o **piso da faixa automática**, não uma
  escolha: escolher o piso é escolher desistir, e desistir é o que o sistema faz
  sozinho, com motivo enumerado, quando nem o piso se sustenta.
- **Nada acima de 30**, §6 item 10 de novo.

**As três listas são teto, como manda a regra acima.** A faixa automática de 5 a
30 quadros continua rodando por baixo da escolha, e o teto de 60% do caminho
medido continua por baixo dela.

**O que o spike não fecha, e continua aberto:** a lista de **tetos de banda**.
Ele mediu com um único teto, o de 1200 kbps que o `tela-no-transporte`
sustenta; fixar uma lista de bandas exige medir quanto o caminho aguenta, que é
a pergunta 2 do §8.

## 5.1 · Quem carrega os bytes até quem assiste — **decidido: o servidor**

Levantada ao construir o §3: o plano de controle ficou inteiro e **nada bombeia
o fluxo de quem compartilha para quem assiste**. A pergunta não é de
implementação, é de desenho, e por isso está aqui em vez de ter sido decidida
por quem escrevia o transporte.

### O fato que enquadra as três opções

**Alguém sobe N cópias.** Não há como fugir disso: multicast não existe na
internet aberta, e um quadro que quatro pessoas assistem sai quatro vezes de
alguma máquina. A pergunta inteira é **de quem é essa máquina**, e se o teto do
§3.2 está sendo calculado do caminho dela.

Hoje o teto sai do caminho de **quem compartilha**. Em duas das três opções
abaixo isso está errado, e é o defeito mais caro desta seção.

### A · O servidor encaminha, como ele já faz com a voz

O `Cage` já reencaminha datagramas de áudio para cada membro
(`cage::Cage::forward`). O fluxo de vídeo entraria pelo mesmo caminho.

- **Custa:** a máquina que hospeda sobe `N × teto`. Com quatro espectadores a
  1,2 Mbps são 4,8 Mbps só de tela, mais a voz de todo mundo. Numa conexão
  doméstica brasileira típica isso é mais do que existe de subida.
- **Obriga uma correção no §3.2:** o teto passa a sair do caminho de **quem
  hospeda, dividido pelo número de espectadores** — não do caminho de quem
  compartilha. Sem essa mudança o produto mede a perna errada e estoura a que
  não mediu.
- **Ganha:** zero arquitetura nova. Funciona com o que existe, inclusive para
  quem está atrás de NAT sem furo — que é justamente quem o degrau 4 serve.

### B · Ponto a ponto, de quem compartilha para cada espectador

- **Custa um caminho que não existe.** Este produto conecta cliente↔servidor e
  **nunca** cliente↔cliente. Seria o degrau 4 do ADR 0022 aplicado entre pares,
  com um ponto de encontro por par e um furo por espectador — e o §6.5 já tirou
  o degrau 5 de escopo por decisão.
- **Não resolve o problema, troca quem paga.** Quem compartilha passa a subir as
  N cópias em vez do anfitrião.
- **Ganha uma coisa real:** quando quem compartilha **não** é quem hospeda, tira
  a carga de cima do anfitrião — que é a pessoa que não pediu para participar
  daquilo.

### C · Só quem hospeda compartilha

- **Custa** o recurso pela metade: numa conversa entre amigos quem mostra a tela
  costuma ser quem está explicando, não quem tem o servidor.
- **Ganha** previsibilidade: a subida que sofre é a de quem escolheu hospedar, e
  o teto sai de um caminho que essa máquina já mede.
- É a opção que **não precisa** da correção do teto, porque as duas pernas são a
  mesma máquina.

### A decisão: **A**, com a correção do teto (22/08/2026)

O servidor encaminha, como já faz com a voz. A alternativa B pedia um caminho
cliente↔cliente que este produto nunca teve e só trocaria quem paga; a C
entregava o recurso pela metade.

**A correção que ela obriga não é opcional.** O teto do §3.2 deixa de sair do
caminho de quem compartilha e passa a sair de:

```
teto = min(
    caminho de quem HOSPEDA × 60% ÷ N espectadores,   ← o que o servidor sobe
    caminho de quem COMPARTILHA × 60%,                ← o que a fonte sobe
    o que a pessoa escolheu (§5),                     ← sempre teto, nunca piso
)
```

A primeira linha é nova e é a que faltava. Sem ela o produto mede uma perna e
estoura a outra.

### A resolução acompanha o teto, e não a contagem de gente

Pedido de quem desenha o produto: «se tiver mais que 4 pessoas vai para 720p,
10 vai para 480p». A intenção está certa e o gatilho está errado, pelo motivo
que o §5 já escreve: **resolução não controla tráfego.** Dez pessoas numa
conexão de fibra cabem em 1080p; quatro numa subida ruim não cabem em 720p.
Amarrar a resolução à contagem degradaria a primeira à toa e ainda estouraria a
segunda.

O gatilho é o **teto**, que já tem N dentro dele. A resolução é a maior que
ainda compra alguma coisa no orçamento que sobrou, e `spikes/tela-no-codec` diz
onde está esse ponto — mesmo teto de 1200 kbps, quadros perdidos por falta de
bits:

| resolução | kbps entregues | quadros perdidos |
|---|---|---|
| 1080p | 1146 | **16,2%** |
| 720p | 872 | 11,1% |
| 540p | 796 | 12,4% |
| 360p | 416 | 2,2% |

O que a tabela mostra é que **uma resolução alta demais para o orçamento perde
quadros sem entregar nitidez**: a 1200 kbps o 1080p joga fora um sexto do que
captura, e o que chega do lado de lá é uma imagem grande e trêmula. O 720p no
mesmo teto perde menos e anda melhor.

**Os degraus são provisórios, e isto está escrito de propósito.** A tabela mede
um teto só. Os limiares certos saem de uma corrida por teto — 1200, 800, 500,
300 kbps —, e ela ainda não foi feita. Enquanto não for, os degraus vêm da
única medida que existe e o código os nomeia como estimativa.

### O que a pessoa vê, e é a metade que o pedido acertou

O pedido queria previsibilidade, e ela é legítima: «mais de quatro pessoas» é
algo que dá para planejar, um número de kbps não é.

Então a **razão** aparece na tela, mesmo o gatilho sendo o teto:

```
720p · 6 pessoas assistindo
```

E quando aperta, a tela diz que apertou e por quê. Um compartilhamento que cai
de resolução porque entrou a quinta pessoa e não explica é o produto sabendo
algo que quem está na frente dele não sabe.

### Uma pendência que este pedido abriu

O pedido cita **480p**, e a lista fechada do §5 é 1080p / 720p / 540p — o 540p
foi escolhido como piso porque abaixo dele a resolução deixa de comprar
nitidez. Ou o piso desce e o motivo escrito no §5 muda, ou a lista fica e o
degrau mais baixo é 540p. **Fica em aberto**, e a corrida por teto acima é quem
tem o número para decidir.

## 6 · O que não entra na primeira versão

Lista honesta, com o motivo de cada uma. Uma promessa larga aqui custaria mais
que o recurso.

1. **Áudio da tela ou do app.** Permissão separada nos três sistemas, mistura
   com o microfone, e eco. No Linux quase não existe. É um segundo recurso.
2. **Câmera.** §0 explica: é a outra metade do trabalho, não um subproduto.
3. **Mais de uma tela ao mesmo tempo na mesma sala.** Uma transmissão por sala
   de voz na v1. Duas dobram a subida de quem recebe e triplicam a interface.
4. **Encoder de hardware** (VideoToolbox, Media Foundation, VAAPI). Três
   integrações e três conjuntos de defeitos. É o caminho da v2, e o que fará a
   promessa de 1080p30 caber.
5. **Encaminhamento pelo servidor** (degrau 5 do
   [ADR 0022](../../adr/0022-alcancar-um-dogma-pela-internet.md)). Continua fora,
   e agora por uma razão a mais: *«o custo do servidor não cresce com o número de
   chamadas»* deixaria de valer no dia em que ele carregasse vídeo. **Sem
   caminho direto, não há compartilhamento** — e a mensagem tem de dizer isso.
6. **E2EE de mídia.** Fora da v1 para a voz (`specs/09`); o vídeo herda.
7. **Gravar a transmissão.** Também fora para a voz. Mesmo motivo, mesma frase.
8. **Região da tela** — um retângulo à escolha. A ScreenCaptureKit tem; a WGC
   não; o portal depende do compositor. Um recurso que existe num sistema só
   ensina a pessoa errada.
9. **Controle remoto.** Outro produto.
10. **HDR, mais de 1080p, mais de 30 quadros.** Nada disso cabe no §2 nem no §3.
11. **Compartilhar de dentro do `plug`.** O cliente de terminal não tem como
    mostrar tela. Ele **recebe o aviso** de que alguém está compartilhando e
    **diz que não consegue mostrar** — nunca ignora em silêncio.
12. **Cursor desenhado à parte, anotação, ponteiro.** Depois.

## 7 · Ordem de construção

A ordem é de **quanto cada passo desbloqueia medição**, como no ciclo anterior.

1. **`spikes/tela-no-codec`** — os quadros por segundo e a CPU do OpenH264.
   **Feito no macOS** (§2 e §5); faltam o Windows e um x86-64. O que ele fechou
   já está nas duas seções, e a lista de opções da interface saiu do chute.
2. **Captura no macOS, sozinha**, imprimindo tamanho e cadência de quadro. É a
   plataforma de quem desenvolve, e é a que tem TCC — o passo que descobre se a
   §4 está certa.
3. **O fluxo de vídeo no protocolo**, com o teto de banda pendurado no sinal que
   já existe, e o quadro-chave espalhado. Aqui o §3 vira código.
4. **O par completo em duas máquinas na mesma casa**, com o `plug` de um lado
   dizendo que não mostra.
5. **Windows.** O **Linux fica fora da v1**, por decisão de 22/08/2026: o
   portal exige `ashpd` + `pipewire` e com eles o binário do Linux deixa de ser
   autocontido (§1), que é uma das propriedades que este produto vende. É
   reversível e não quebra promessa nenhuma — a v1 sai com macOS e Windows, e o
   Linux fica nomeado como pendência em vez de trocado por baixo.
6. **Portão de campo**, duas casas de verdade, com voz e tela juntas, medindo o
   sinal da voz durante a transmissão. O número a bater é o do §3.2.

## 8 · Perguntas que continuam abertas

1. **60% do caminho é o teto certo?** É o que este spike sustenta num cano
   determinístico. Wi-Fi ruim tem perda esporádica e atraso que anda sozinho, e
   nenhum dos dois está na prova.
2. **Como se mede «o caminho» quando ninguém está enchendo?** O sinal da voz
   diz que está bom a 40 kbps; ele não diz quanto cabe. Subir devagar até doer é
   o que todo mundo faz e é o que faz a voz doer.
3. **Quem escolhe o teto quando duas pessoas compartilham em salas diferentes na
   mesma máquina?** A conta é da máquina, não da sala.
4. **O módulo do Cisco baixa quando?** No primeiro clique, ou numa página de
   ajustes onde a pessoa decide antes de precisar?
5. **`shiguredo_libvpx` sai de canary a tempo de mudar a decisão do §2?** Se sair
   antes do passo 3, a conta muda e vale refazer.
6. **A borda amarela do Windows aparece em captura de monitor também**, ou só de
   janela? Muda o que a interface promete.
7. **O `restore_token` do portal sobrevive a troca de compositor?** Se não, a
   pessoa é perguntada de novo e não vai entender por quê.
8. **Uma tela para quatro pessoas é quatro vezes a subida.** A conta não foi
   medida, e o §5.3 a adia sem resolvê-la.
