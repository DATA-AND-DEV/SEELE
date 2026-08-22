# Compartilhamento de tela — desenho, e a prova de que a voz sobrevive

**Data:** 2026-08-22
**Estado:** aguardando plano
**Prova:** `spikes/tela-no-transporte/`

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
respondendo uma pergunta que um documento não respondia. Esta traz a quarta.

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

**Não medido aqui, e este documento não vai fingir que foi.** O que se sabe é a
forma do problema: OpenH264 é baseline puro, feito para conversa em banda baixa,
e encode por software de 1080p a 30 quadros é trabalho de núcleo inteiro numa
máquina de escritório — mais em notebook fino que estrangula por calor.

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

**Próxima prova, e é a que falta:** `spikes/tela-no-codec` — quadros por segundo
sustentados e uso de CPU do OpenH264 em 1080p e em 720p, com conteúdo de tela
real, nas três máquinas. Sem esse número o plano não deve fixar nenhum teto.

## 3 · Transporte — a única parte que foi medida

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

### A lista de opções só é fixada depois do spike do codec

Este documento **não** enumera as resoluções e os quadros oferecidos, e a
omissão é deliberada: o spike do transporte mediu transporte, e quantos quadros
de 1080p o encoder por software entrega numa máquina comum ainda não foi
medido (§2, «CPU»). Oferecer 1080p antes desse número é oferecer uma opção que
pode não ter CPU para acontecer.

`spikes/tela-no-codec` é quem fecha esta lista.

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

1. **`spikes/tela-no-codec`** — os quadros por segundo e a CPU do OpenH264 nas
   três máquinas. Sem esse número o resto fixa tetos no chute.
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
