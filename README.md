<img src="docs/imagens/marca-cartela.png" alt="SEELE — dois nós e uma ligação" width="440">

# SEELE

**Voz e texto auto-hospedados, com o terminal em primeiro lugar.**

Você sobe um servidor na sua máquina. Seus amigos conectam nele. Não existe
serviço no meio, não existe cadastro em lugar nenhum, e o cliente principal
roda inteiro no terminal — inclusive por SSH, num terminal de 16 cores.

Não é um clone de Discord com tema escuro. É a suposição oposta: o servidor é
seu, os dados são seus, e a interface de referência é aquela que cabe em 80×24.

A marca diz o sistema inteiro: **dois nós e uma ligação.** O cheio é quem
hospeda, o vazio é quem chega, a diagonal é o enlace.

---

## Como é por dentro

```text
   plug (terminal)          SEELE.app (desktop)
          │                        │
          └────────┬───────────────┘
                   │  seele-core — sessão, protocolo, áudio, estado
                   │  (nenhuma lógica vive nas interfaces)
                   ▼
             ── QUIC / TLS 1.3 ──
                   ▼
                 seeled
```

Um **servidor** é uma instância: o banco dele, as contas dele, os canais dele.
Dentro dele há **salas de voz** e **canais de texto**. Quem entra é uma
**pessoa**, e a qualidade da conexão de cada uma aparece como **sinal** — um
número de 0 a 100 sempre visível, que é a diferença de caráter em relação a um
cliente de conversa comum.

**A regra mais importante do projeto** é que toda a lógica mora em
`seele-core`, e as interfaces só traduzem evento em pixel e entrada em comando.
Não é convenção: `cargo xtask check-deps` quebra o build se uma casca tentar
enxergar o protocolo direto.

| | |
|---|---|
| Transporte | QUIC (`quinn`), TLS 1.3 obrigatório, uma porta **UDP** |
| Confiança | TOFU — o cliente fixa a chave no primeiro contato, modelo SSH |
| Voz | Opus 48 kHz mono, quadros de 20 ms, DTX, jitter buffer adaptativo |
| Identidade | par de chaves Ed25519, guardado em disco |
| Persistência | SQLite com migrações embutidas, no próprio binário |
| Alcance | rede local, IPv6, porta no roteador por UPnP, e furo de NAT |

---

## A interface

Retratos de verdade: saem do mesmo código que desenha no terminal, rodando
`cargo run --example telas -p seele-tui`. O que o texto não mostra é a cor — e
é de propósito que nada dependa dela.

### Em operação

```text
┌ ■—□ SEELE ─ 12:04:33 ────────────────────────────────────────────────────────┐
│┌ SERVIDOR ──────┐┌ SALAS / CANAIS ──────┐┌ MENSAGENS ───────────────────────┐│
││▸ Casa do Alexa…││▼ SALA 1              ││12:01 alexandre                   ││
││                ││  ● alexandre    █ 98%││  subi o servidor aqui em casa    ││
││                ││  ○ rafa         ▒ 71%││12:03 rafa                        ││
││                ││  ○ bia     MUDO ░ 44%││  meu sinal caiu, um segundo      ││
││                ││▶ JOGOS               ││12:04 você                        ││
││                ││─ CANAL geral         ││  vendo — o atraso subiu junto    ││
││                ││─ CANAL combinados    ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││▸ _                               ││
│└────────────────┘└──────────────────────┘└──────────────────────────────────┘│
│ NORMAL │ SINAL █ 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ MUDO OFF │
└──────────────────────────────────────────────────────────────────────────────┘
```

Três painéis e uma barra de telemetria permanente. Ninguém precisa abrir menu
para saber que a conexão está ruim.

### Buscar no que foi dito

```text
┌ ■—□ SEELE ─ 12:04:33 ────────────────────────────────────────────────────────┐
│┌ SERVIDOR ──────┐┌ SALAS / CANAIS ──────┐┌ MENSAGENS ───────────────────────┐│
││▸ Casa do Alexa…││▼ SALA 1              ││12:01 alexandre                   ││
││                ││  ● alexandre    █ 98%││  subi o servidor aqui em casa    ││
││                ││  ○ rafa         ▒ 71%││12:03 rafa                        ││
││                ││  ○ bia     MUDO ░ 44%││  meu sinal caiu, um segundo      ││
││                ││▶ JOGOS               ││12:04 você                        ││
││                ││─ CANAL geral         ││  vendo — o atraso subiu junto    ││
││                ││─ CANAL combinados    ││12:05 alexandre                   ││
││                ││                      ││  o sync voltou a subir           ││
││                ││                      ││12:06 bia                         ││
││                ││                      ││  aqui o sync nem caiu            ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││/ sync_  [1/2]                    ││
│└────────────────┘└──────────────────────┘└──────────────────────────────────┘│
│ BUSCA │ SINAL █ 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ MUDO OFF  │
└──────────────────────────────────────────────────────────────────────────────┘
```

### A ajuda cabe numa tela

```text
┌ ■—□ SEELE ─ 12:04:33 ────────────────────────────────────────────────────────┐
│┌ SERVIDOR ──────┐┌ SALAS / CANAIS ──────┐┌ MENSAGENS ───────────────────────┐│
││▸ Casa do A┌ AJUDA ─────────────────────────────────────────────┐           ││
││           │h j k l / setas   navegar                           │em casa    ││
││           │Tab / Shift+Tab   alternar painel                   │           ││
││           │Enter             entrar na sala / abrir canal      │gundo      ││
││           │s                 sair da sala de voz               │           ││
││           │i                 escrever mensagem                 │u junto    ││
││           │Espaço (segurar)  falar                             │           ││
││           │m                 mudo (microfone fechado)          │           ││
││           │d                 isolamento total (surdo)          │           ││
││           │g / G             topo / fim                        │           ││
││           │/                 buscar no histórico               │           ││
││           │n / N             ocorrência seguinte / anterior    │           ││
││           │?                 esta ajuda                        │           ││
││           │:conectar <host>  conectar a um servidor            │           ││
││           │:voice_room <nome>      entrar numa sala de voz           │           ││
││           │:sync             diagnóstico detalhado             │           ││
││           │:audio            dispositivos                      │           ││
││           │:ejetar           sair do servidor e escolher outro │           ││
││           │:q                sair do programa                  │           ││
│└───────────└────────────────────────────────────────────────────┘───────────┘│
│ NORMAL │ SINAL █ 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ MUDO OFF │
└──────────────────────────────────────────────────────────────────────────────┘
```

Uma tecla, de qualquer tela, nos dois clientes. O critério é que dê para usar o
produto sabendo só ela.

### Quando o enlace cai

```text
┌ ■—□ SEELE ─ 12:04:33 ────────────────────────────────────────────────────────┐
│BATERIA INTERNA 04:47 · 3 tentativas                                          │
│ENLACE PERDIDO                                                                │
│┌ SERVIDOR ──────┐┌ SALAS / CANAIS ──────┐┌ MENSAGENS ───────────────────────┐│
││▸ Casa do Alexa…││▼ SALA 1              ││12:01 alexandre                   ││
││                ││  ● alexandre    █ 98%││  subi o servidor aqui em casa    ││
││                ││  ○ rafa         ▒ 71%││12:03 rafa                        ││
││                ││  ○ bia     MUDO ░ 44%││  meu sinal caiu, um segundo      ││
││                ││▶ JOGOS               ││12:04 você                        ││
││                ││─ CANAL geral         ││  vendo — o atraso subiu junto    ││
││                ││─ CANAL combinados    ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││                                  ││
││                ││                      ││▸ _                               ││
│└────────────────┘└──────────────────────┘└──────────────────────────────────┘│
│ NORMAL │ SINAL █ 94% │ RTT 38ms │ JIT 12ms │ LOSS 0.2% │ OPUS 32k │ MUDO OFF │
└──────────────────────────────────────────────────────────────────────────────┘
```

Cinco minutos de tentativa com espera exponencial, e o assento fica reservado
esse tempo todo: quem entrou num túnel volta para a mesma sala.

### Num terminal apertado

```text
TERMINAL 56×14 < 80×24
12:01 alexandre
  subi o servidor aqui em casa
12:03 rafa
  meu sinal caiu, um segundo
12:04 você
  vendo — o atraso subiu junto







```

Abaixo de 80×24 degrada para painel único com aviso. O que sobrevive é a
conversa, porque um cliente que não mostra o que foi dito não está degradado,
está quebrado.

### O cliente gráfico

Tauri sobre o mesmo núcleo. A composição é deliberadamente a mesma da TUI:
mesmas colunas, mesma barra permanente no rodapé. Quem usa um abre o outro e
sabe onde tudo está.

**HOSPEDAR AQUI** sobe um servidor dentro do próprio app e entra nele — quem só
quer clicar nunca precisa abrir um terminal. Ele vive enquanto a janela estiver
aberta, e o link de convite aparece no topo, pronto para copiar.

Três coisas que o app tem e o terminal ainda não:

- **a trilha de servidores**, à esquerda. O histórico de onde você já esteve;
  apertar um troca de servidor, e a troca pergunta antes porque ela derruba a
  sessão — e derruba o servidor junto, se for esta máquina que hospeda;
- **personalização**, para quem administra: nome e ícone do servidor. O ícone é
  PNG, no máximo 8 KiB e 256 px, e o limite está escrito na tela **antes** de
  você escolher o arquivo;
- **a faixa de pessoas**, fixa à direita, agrupada por sala, com o sinal de cada
  uma dentro do cartão.

---

## Alcançar de fora

Um servidor em casa está atrás de um roteador, e a internet não o alcança
sozinha. O SEELE sobe uma escada de quatro degraus e põe **todos** os endereços
que encontrar no convite — nunca só o melhor:

| degrau | como |
|---|---|
| 1 | o endereço direto, quando a máquina já tem um IPv4 público |
| 2 | IPv6 nativo, quando as duas pontas o têm |
| 3 | uma porta pedida ao roteador por UPnP |
| 4 | **furo de NAT**, com um ponto de encontro apresentando as duas pontas |

O ponto de encontro não vê nada do que é dito: ele apresenta dois endereços um
ao outro e esquece. O TLS é ponta a ponta, e ele não está nele. `seele-encontro`
é o programa, e são cinquenta linhas de trabalho útil.

O degrau 4 foi exercitado entre duas máquinas em redes diferentes — uma em casa
atrás de CGNAT, outra numa rede móvel — e o convite passa a carregar o bilhete
que abre o caminho. O ADR 0022 conta a escada inteira, e por que o degrau 5
(retransmitir pelo servidor) está fora de escopo por decisão.

---

## Instalar

### Um arquivo, tudo dentro

Na aba **Releases**, um instalador por sistema: `.dmg` no macOS, `.exe` no
Windows, `.deb` no Linux. Cada um traz as três coisas — o app gráfico, o `plug`
e o `seeled` —, então quem instala não precisa decidir nada antes de entender a
diferença.

**Nada é assinado.** No macOS o sistema vai dizer que *não consegue verificar
se o app contém malware*, e o botão que ele oferece é "Mover para o Lixo". Não
é detecção de nada: é a ausência de notarização, que exige conta paga da Apple.
A saída é uma linha, `xattr -dr com.apple.quarantine /Applications/SEELE.app`,
ou abrir pelo botão direito → **Abrir**. No Windows o SmartScreen avisa e o
caminho é **Mais informações** → **Executar assim mesmo**. As notas de release
explicam cada caso.

### Só o terminal, numa linha

**macOS e Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.sh | sh
```

**Windows**, num PowerShell:

```powershell
irm https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.ps1 | iex
```

O script confere a soma SHA-256 do que baixou contra o `SHA256SUMS` publicado
no release, e recusa instalar se não bater.

Dito isso: **você está prestes a canalizar um script da internet para dentro do
seu shell**, num produto cujo argumento é não depender de terceiros. Se isso
incomoda — e é razoável que incomode —, baixe e leia antes, ou pegue o pacote
direto na aba Releases e confira a soma à mão. As duas alternativas estão no
cabeçalho do próprio script.

Compilando do código-fonte, que é a opção que não exige confiar em ninguém:

```sh
git clone https://github.com/DATA-AND-DEV/SEELE && cd SEELE
cargo build --release --bin seeled --bin plug
```

No Windows isso pede o Build Tools do Visual Studio — ver `docs/windows.md`.

Depois de instalar pelo `.dmg`, o `plug` e o `seeled` moram dentro do app. Para
tê-los no `PATH`:

```sh
sudo ln -sf /Applications/SEELE.app/Contents/MacOS/{plug,seeled} /usr/local/bin/
```

No Linux o `.deb` já os põe em `/usr/bin`. No Windows ficam na pasta do
programa e ainda não entram no `PATH` — ver `docs/pendencias.md`.

---

## Começar

Numa máquina:

```sh
seeled 0.0.0.0:8383
```

Ele imprime o endereço para usar na outra máquina e a impressão digital do
certificado. Na outra:

```sh
plug --server 192.168.x.x:8383 --nick seunome
```

Aperte `?`.

Duas coisas que economizam meia hora:

- **A porta 8383 é UDP**, porque o transporte é QUIC. É o erro de firewall mais
  comum aqui, porque a regra que se escreve de cabeça é sempre TCP.
- **Fones dos dois lados.** Não há cancelamento de eco, e sem fones haverá
  realimentação.

### Fechar o servidor

Por padrão qualquer um que alcance a porta entra — o certo para rede local, e o
`seeled` avisa em voz alta ao subir assim. Para fechar:

```sh
seeled convite rafa        # link de uso único, vale sete dias
seeled senha "a senha"    # ou um segredo para o grupo todo
```

O convite sai como um link pronto para mandar:

```text
seele://192.168.0.7:8383?fp=782cc791…&convite=2QKPAXPP97W5459H3TPA
```

Ele carrega a impressão digital do certificado, então quem recebe **não precisa
conferi-la por outro canal** — o cliente compara sozinho: num servidor que ele
ainda não conhece, **recusa** se não bater; num servidor que ele já conhece, avisa
que o link não é daquele servidor e entra assim mesmo, porque a chave de ontem
já provou quem é. Do outro lado, `plug --url "seele://…"`.

A senha do servidor nunca viaja no link, e isso é decisão registrada: senha vale
para sempre, convite gasto não vale nada.

---


---

## Compartilhar a tela

**Desenhado e medido, ainda não construído.** A spec está em
`docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` e a prova
em `spikes/tela-no-transporte/`.

A pergunta que a prova respondeu é a que mais importa: **quando alguém
compartilha a tela e a subida da casa não dá conta, o que sobra da voz?** Um par
QUIC inteiro num processo, com um cano de 2000 kbps no meio, carga com forma de
vídeo e sem codec:

```text
cenário                        perda   p50 ms   p95 ms   vídeo kbps
voz sozinha                    0.00%     21.7     22.9            0
vídeo em fluxo, sem teto       0.10%    225.7    258.3         2030
vídeo em datagrama             16.10%   2161.4   2203.1         1981
teto em 60% do caminho         0.00%     23.1     78.9         1280
  + quadro-chave espalhado     0.00%     22.2     35.8         1200
```

Três conclusões, e nenhuma delas era óbvia antes de medir:

- **datagrama é o pior desenho.** Voz e vídeo caem na mesma fila e a voz perde
  de 16% a 98% dos quadros. É o caminho que parecia natural, porque a voz já vai
  por lá;
- **uma segunda conexão não ajuda** — 221 ms de p50, igual a não fazer nada;
- **o que protege a voz não é o transporte, é o teto de bitrate.** Com o vídeo
  limitado a 60% do caminho medido, a voz volta a 23 ms — praticamente o que ela
  custa sozinha.

A regra de aceite da v1 é uma frase: **a voz nunca cede à tela.** Quem baixa
resolução e quem para é o vídeo.

O que fica de fora da primeira versão está enumerado com motivo — doze itens,
entre eles áudio da tela, câmera, gravação e controle remoto.

---

## Em que pé está

**1.362 testes automáticos passando.** O que eles cobrem:

- dois clientes conversando por texto e voz sintética através do servidor
- pipeline de áudio sob 5% de perda induzida, e soak de dez minutos em tempo
  simulado
- reinício do servidor sem expulsar quem já estava conectado
- senha e convite fechando a porta; convite servindo a uma pessoa só
- limitação de taxa nas duas pontas: quem bate à porta em laço é recusado com
  motivo, e quem inunda de mensagens é avisado antes de ser derrubado
- a mesma sessão retomada entre o `plug` e o app, com autor e horário
- a interface: que a ajuda não prometa uma tecla que não existe, que o
  vocabulário aposentado não volte à tela, que a marca não use o vermelho de
  alerta, e que os retratos acima saiam do código que desenha

Verificado **entre duas máquinas de verdade**, em redes diferentes: o furo de
NAT abrindo caminho de uma casa atrás de CGNAT para uma rede móvel, com o ponto
de encontro apresentando as duas pontas.

Ainda **não** verificado: voz por microfone real entre duas máquinas, de ponta a
ponta, com as duas pessoas se ouvindo. É a última validação que falta, e
`docs/teste-duas-maquinas.md` é o roteiro dela.

O que está frouxo está em `docs/pendencias.md`, com nome e motivo.

---

## O repositório

| onde | o quê |
|---|---|
| `specs/` | a fonte de verdade, escrita antes do código |
| `crates/seele-proto` | tipos do protocolo, serialização, versionamento |
| `crates/seele-audio` | codec, jitter buffer, mixer, deriva de clock, simulador de rede |
| `crates/seele-core` | o núcleo: sessão, estado, voz. Tudo que pensa |
| `crates/seele-server` | `seeled`, o servidor |
| `crates/seele-encontro` | o ponto de encontro do furo de NAT |
| `crates/seele-tui` | `plug`, o cliente de terminal |
| `crates/seele-ffi` | a superfície que as cascas gráficas falam |
| `apps/seele-app` | o cliente desktop, Tauri |
| `design/marca/` | a marca, e o gerador de todos os tamanhos dela |
| `docs/adr/` | por que cada decisão difícil foi tomada assim |
| `docs/pendencias.md` | o que está quebrado e ainda não foi consertado |
| `spikes/` | provas de conceito, com a pergunta e o número que a respondeu |

As ADRs são o melhor lugar para entender o projeto de verdade: cada uma diz o
que foi decidido, o que foi descartado, e o que custa voltar atrás. O código as
cita quase seiscentas vezes, e não por formalidade — quando um comentário
explica por que uma linha é daquele jeito, a razão inteira está numa delas.

---

## Licença

Ainda não definida, mas o que a segurava saiu do caminho.

A interface usava o vocabulário de Evangelion — Dogma, VoiceRoom, Pessoa, A.T. Field,
e a assinatura em katakana —, e definir direitos com isso dentro era o tipo de
decisão que se toma errado. Uma avaliação de usabilidade mostrou que o
vocabulário também cobrava um preço de quem chegava, e as duas razões apontaram
para o mesmo lugar: os nomes passaram a ser o que as coisas são, e a marca
passou a ser dois nós e uma ligação.

Fica a estética inteira, que nunca foi o problema: mono, laranja sobre
quase-preto, canto reto, sem sombra.
