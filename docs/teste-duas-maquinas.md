# Teste entre duas máquinas

O que nenhum teste automático deste repositório cobre: voz real, por microfone
real, entre dois computadores numa rede real. É a validação que fecha M1.15 e
M1.16, e é a única que depende de você.

Leva vinte minutos. Anote os resultados em `docs/m1-medicoes.md`.

---

## Antes de começar

Uma máquina é o **Server** (servidor + cliente) e a outra é só cliente. Podem ser
os dois sistemas operacionais que você tiver — quanto mais diferentes, melhor,
porque a matriz de três SOs em CI nunca executou.

O cliente é o app, então as duas máquinas precisam de interface gráfica. Em
cada uma:

```sh
cargo build --release --bin seeled
cargo build --release -p seele-app
```

O que o app mede sai em dois lugares: o **rodapé da sessão** (ATRASO, JITTER,
DISTÚRBIO, CODEC, SINAL DA SALA e CAMINHO) e o **`seele.log`**, em
`~/.config/seele/seele.log` no macOS e no Linux (`$SEELE_HOME/seele.log`, se
ela estiver definida) e em `%APPDATA%\tech.datadev.seele\seele.log` no
Windows. As medidas abaixo dizem de qual dos dois cada número sai.

---

## 1 · Subir o servidor

Na máquina A:

```sh
./target/release/seeled 0.0.0.0:8383
```

Ele imprime três coisas que importam:

```
seeled listening on 0.0.0.0:8383

na outra máquina, cole no SEELE:
  seele://192.168.x.x:8383?fp=50217d68c6…   (na mesma rede)

certificate fingerprint: 50217d68c6...
```

O link já leva a impressão digital, e o app a confere sozinho no primeiro
contato. **Anote-a mesmo assim.** É o que o ADR 0003 pede que você confira: o
cliente vai fixá-la no primeiro contato e recusar a conexão em silêncio se ela
mudar depois.

Se a linha "na outra máquina" não aparecer, o `seeled` não achou um endereço de
rede — provavelmente está sem rede, ou só em loopback.

### Fechar o servidor, se quiser

Por padrão qualquer um que alcance a porta entra — o certo para testar em rede
local, e o `seeled` avisa ao subir assim. Para fechar:

```sh
./target/release/seeled convite marcela   # link de uso único, sete dias
./target/release/seeled senha "a senha"   # ou um segredo para o grupo
./target/release/seeled senha --remover   # volta a aceitar qualquer um
```

O convite sai como um link pronto para mandar:

```
seele://192.168.x.x:8383?fp=782cc791…&convite=2QKPAXPP97W5459H3TPA
```

Ele carrega a impressão digital do certificado, então quem receber **não
precisa conferi-la por outro canal** — o cliente compara sozinho e recusa se
não bater. Do outro lado, no app, **CONECTAR** e cole o link.

### Firewall

A porta 8383 é **UDP** (QUIC), não TCP. É o erro de configuração mais provável
aqui, porque a maioria das regras que as pessoas escrevem de cabeça é TCP.

- **macOS:** na primeira execução o sistema pergunta. Aceite.
- **Linux:** `sudo ufw allow 8383/udp`, ou o equivalente do seu firewall.
- **Windows:** `New-NetFirewallRule -DisplayName SEELE -Direction Inbound -Protocol UDP -LocalPort 8383 -Action Allow` num PowerShell de administrador.

Redes de convidado e alguns pontos de acesso isolam clientes entre si. Se nada
conectar e o firewall estiver liberado, é a suspeita seguinte.

---

## 2 · Conectar da máquina B

```sh
./target/release/seele-app
```

Escolha o apelido no perfil, no rodapé da entrada. Depois **CONECTAR**, cole o
link que o `seeled` imprimiu no campo de baixo e **ENTRAR**.

Na primeira vez a sessão abre dizendo `PRIMEIRO CONTATO VERIFICADO — O CONVITE
CONFIRMOU A CHAVE`, com a impressão digital embaixo: o app comparou a do link
com a que o servidor apresentou, e elas bateram. Se em vez do link você colar o
endereço cru, a frase é `PRIMEIRO CONTATO — CHAVE FIXADA`, e aí **confira a
impressão contra a que o `seeled` imprimiu.** Se conferir, é esse servidor. Se
não conferir, alguém está no meio — e é exatamente para esse momento que o
ADR 0003 existe.

Conecte também na máquina A, com **apelido diferente**: no app dela,
**CONECTAR** e `127.0.0.1:8383`.

> Dois apps com o mesmo `$SEELE_HOME` são **a mesma pessoa** — o PERSISTENCE
> vincula o apelido à identidade que o reivindicou (ADR 0017). Para serem duas
> pessoas na mesma máquina,
> `SEELE_HOME=~/.seele-outro ./target/release/seele-app`.

---

## 3 · Texto primeiro

Antes de qualquer coisa com áudio, prove que o enlace existe.

| | esperado |
|---|---|
| Os dois se veem no roster | cada um vê o outro na lista sob a sala de voz |
| Escrever no canal e `Enter` na máquina A | aparece na B em menos de um segundo |
| O mesmo de B para A | idem |
| O ATRASO do rodapé, nas duas | plausível para a rede (LAN cabeada: 1–5 ms; wifi: 5–30 ms) |

Se o texto não atravessar, o áudio também não vai. Pare aqui e resolva a rede.

---

## 4 · Voz

Fones nos dois lados. **Sem fones haverá realimentação**, e o cancelamento de
eco não existe neste produto (ADR 0007 adiou explicitamente).

Entre na sala de voz, segure **ESPAÇO** e fale. É o modo TECLA; CONFIGURAÇÕES,
em «COMO O MICROFONE ABRE», troca por VOZ ou ABERTO. A janela relata a soltura
da tecla, então segurar funciona de verdade, sem a trava que os terminais
pediam (ADR 0016).

Anote:

| pergunta | o que observar |
|---|---|
| A voz chega? | inteligível, sem robotização |
| Há eco ou realimentação? | só deve haver se alguém estiver sem fones |
| Quem fala acende no roster do outro? | o `speaking` vem de datagrama chegando, não de o cliente afirmar |
| O mudo de um aparece no roster do outro? | o mudo é anunciado, não só local |
| Qual o SINAL DA SALA em repouso? | o rodapé, nos dois lados |

---

## 5 · As três medições que faltam

Estas são M1.16, e são o motivo deste documento.

### 5.1 · Soak de 10 minutos

Fiquem os dois na sala de voz por dez minutos com conversa intermitente. O que se
procura é o que só aparece com tempo: **estalos**, e a deriva de clock que a
M1.8 corrige (`docs/m1-medicoes.md` tem a tabela de deriva medida).

Ao fim, olhe o rodapé e o `seele.log` nas duas. Anote:

- estalos ouvidos, e mais ou menos quando
- o SINAL DA SALA no início e no fim
- o CODEC do rodapé mudou de valor?
- o rodapé acendeu **ÁUDIO LOCAL FALHANDO** em algum momento dos dez minutos?
- o `seele.log` tem a linha «o laço de voz demorou mais que um quadro entre
  duas conferidas», e com que `volta_ms` e quantas `reposicoes`?
- o `seele.log` tem a linha «o reamostrador recusou a razão de compasso»?

As três últimas são a pendência 2. **ÁUDIO LOCAL FALHANDO** é esta máquina
derrubando áudio agora — amostra que o dispositivo pediu e não tinha, ou
captura que transbordou —, e não a rede. As duas linhas do `seele.log` saem uma
vez por sessão cada, e cada uma manda para um lugar diferente:

- **nenhuma das duas**: a malha está segurando a deriva. É o normal, mesmo com
  o aviso tendo acendido no arranque.
- **o laço demorou mais que um quadro**: a reprodução está sendo mantida em dia
  por reposição. É a volta do laço, que é a pendência 15, e o número a pedir é
  o `volta_ms`.
- **o reamostrador recusou**: a deriva entre esta máquina e o dispositivo ficou
  sem correção. Rode o `ritmo` abaixo nessa máquina: grampo crescendo quer
  dizer que a razão pedida saiu da faixa em que cristal vive, então a causa não
  é deriva — taxa diferente da anunciada (a linha «o aparelho … taxa» do
  `seele.log` diz a taxa), ou dispositivo trocado.

O app não mostra o ppm, o anel e o grampo em números. Quem dá os três é o
exemplo `ritmo`, e antes do soak cada máquina pode dar o próprio veredito
sozinha, em um minuto e sem a outra:

```
cargo run --release -p seele-audio --example ritmo
cargo run --release -p seele-audio --example ritmo -- --sem-malha
```

Ele dá voltas com a forma do laço de voz contra o dispositivo daquela máquina e
imprime a perda por intervalo, o fundo do anel e a deriva medida. Neste Mac,
sem a malha: 258 amostras perdidas em 60 s, fundo zero em todos os intervalos.
Com ela: zero em dez minutos.

### 5.2 · Perda induzida de 5%

Numa das máquinas, degrade a rede de propósito.

**Linux:**
```sh
sudo tc qdisc add dev <interface> root netem loss 5%
# para tirar:
sudo tc qdisc del dev <interface> root
```

**macOS** (`dnctl`/`pfctl`, requer privilégio):
```sh
sudo dnctl pipe 1 config plr 0.05
echo "dummynet out proto udp from any to any port 8383 pipe 1" | sudo pfctl -f - -e
# para tirar:
sudo pfctl -d && sudo dnctl -q flush
```

Converse por dois minutos. A pergunta é uma só: **continua inteligível?** Não
"continua perfeito" — 5% de perda deve degradar audivelmente e permanecer
compreensível. Se virar ininteligível, o jitter buffer ou o concealment não
estão fazendo o trabalho.

Anote também o que o rodapé mostra em DISTÚRBIO. Ele deve refletir a perda
induzida; se marcar zero com 5% de perda real, a medição está errada e isso é
um defeito.

### 5.3 · Latência boca-a-ouvido

O número que o ADR 0009 orça. Duas formas, da pior para a melhor:

**Rápida e grosseira.** Uma pessoa bate palma perto do microfone enquanto a
outra escuta pelos fones e bate palma ao ouvir. Grave as duas com um celular ao
lado e meça o intervalo entre as palmas no áudio. Divida por dois. Vale ±30 ms.

**Boa.** Meça a metade local em cada máquina com o rig do M1.2:

```sh
cargo run --release --example latencia -p seele-audio
```

Rode duas vezes: uma com cabo da saída para a entrada (mede a máquina) e uma no
ar (mede a experiência). A ferramenta recusa dar número quando não tem confiança
— um valor plausível vindo de cabo solto seria pior que valor nenhum.

Some: `latência local de A` + `ATRASO/2` (o ATRASO do rodapé é a ida e volta)
+ `profundidade do jitter buffer` + `latência local de B`. O app não mostra a
profundidade do jitter buffer; o JITTER do rodapé é a variação medida, e não
ela. Faça a conta com os dois extremos do ADR 0009, o piso de 20 ms e o alvo de
40 ms, e compare com os ≈67/87 ms que ele orça a partir das medições de M1.

---

## 6 · O que mais checar no app

- o histórico aparece ao abrir o canal, com autor e horário corretos
- o deslizante de volume, ao apontar uma linha do roster, muda o que se ouve

---

## 7 · Furo de NAT, o degrau 4 (duas casas, não duas máquinas)

Este é o único teste do repositório que **nenhuma máquina sozinha consegue
fazer**, e é por isso que ele está escrito aqui em vez de estar em `cargo test`:
ele precisa de duas redes **diferentes**, cada uma atrás do seu próprio NAT. Duas
máquinas na mesma casa não servem — elas se acham pela rede local, que é o degrau
1, e o degrau 4 nem chega a ser exercido.

O jeito mais fácil de conseguir duas redes: uma máquina na sua casa e a outra no
celular como roteador (4G/5G), que quase sempre é CGNAT — exatamente o caso sem
saída antes deste degrau.

**Antes**, suba um ponto de encontro numa VPS e aponte para ele — dez linhas em
[`ponto-de-encontro.md`](ponto-de-encontro.md):

```sh
# na VPS
./target/release/seele-encontro --barulhento
```

Na máquina A, em casa, com o UPnP do roteador **desligado** de propósito (é o que
força a escada a chegar ao degrau 4), abra o app com o ponto de encontro no
ambiente e aperte **HOSPEDAR AQUI**:

```sh
SEELE_ENCONTRO=<endereço-da-vps>:8384 ./target/release/seele-app
```

O que checar, em ordem:

1. A frase embaixo do link diz **«UM PONTO DE ENCONTRO ABRIU O CAMINHO»**. Se
   disser «ESTE LINK SÓ FUNCIONA NA SUA REDE», o degrau 4 não subiu: o
   `seele.log` da máquina A diz por quê, na linha «o degrau 4 não deu», e o
   `--barulhento` da VPS diz se o pedido chegou lá.
2. O link tem um `enc=` com **duas metades** separadas por `/`, e um endereço
   público seu no `alt=`.
3. Na máquina B, na outra rede, cole o link. Ela entra.
4. Na VPS, o `--barulhento` mostra **duas** apresentações: a da máquina A se
   descobrindo, e a da máquina B chegando. Nunca mais que isso — se aparecer
   tráfego contínuo ali, alguma coisa está passando pelo ponto de encontro que
   não deveria.
5. **Desligue o ponto de encontro** e hospede de novo. O **HOSPEDAR AQUI** tem
   de abrir a sala na mesma velocidade de antes (mais no máximo um segundo), com
   o link levando os endereços de sempre e **sem** `enc=`. Este é o teste de que
   o degrau 4 não virou ponto único de falha.
6. Com a conversa de pé, derrube a rede da máquina B por uns segundos e deixe
   voltar. A reconexão sai de uma porta nova, então ela bate no ponto de
   encontro de novo — a sessão tem de voltar dentro dos cinco minutos da
   bateria.

**Se não abrir:** as duas redes podem ser NAT simétrico, e aí não há o que
consertar aqui — é o caso que o ADR 0022 deixa para o encaminhamento de porta à
mão. Vale anotar qual operadora e qual roteador de cada lado, porque essa
informação é o que diz se vale a pena um degrau 5 algum dia.

---

## Checklist de plataforma (M1.15)

Uma coluna por máquina testada. Isto é o que `specs/09-roadmap.md` pede como
entregável de M1.15 — CoreAudio, WASAPI, ALSA e PipeWire.

| | máquina A | máquina B |
|---|---|---|
| SO e versão | | |
| Backend de áudio | | |
| Dispositivos (captura / reprodução) | | |
| Taxa nativa (a linha «o aparelho … taxa» do `seele.log`) | | |
| Latência por cabo (M1.2) | | |
| Latência no ar (M1.2) | | |
| Estalos em 10 min | | |
| Inteligível a 5% de perda | | |
| Troca de dispositivo a quente funciona | | |
| RSS do app após 10 min | | |

A troca a quente é M1.14: com a chamada rodando, tire o fone USB e coloque de
volta. A chamada deve pausar e retomar, não morrer.

---

## O que fazer com os resultados

Anote em `docs/m1-medicoes.md`, que já tem as medições sintéticas de M1 e é onde
os números reais devem ficar ao lado delas. Onde a realidade divergir das specs,
`specs/10-convencoes.md` exige corrigir `00` e `03` — é a tarefa M1.17, e ela só
pode ser feita depois disto.

Se algo falhar, o mais útil é: qual seção, o que o rodapé de telemetria
mostrava, o `seele.log` das duas máquinas e o que o `seeled` imprimiu no
terminal dele naquele momento.

---

## O caminho entre pares (subprojeto A da malha)

**Isto é o que nenhum teste automático deste repositório consegue produzir.** Um
furo de NAT entre dois roteadores domésticos não acontece em `127.0.0.1`, e os
dois números abaixo são a razão de o subprojeto A existir antes do B (o
desenho da árvore de retransmissão). Sem eles, a aritmética de quantas pessoas
uma sala comporta é chute — ver a seção seguinte antes de convocar ninguém.

### Confira isto antes de marcar horário com três pessoas

**O empréstimo de subida não tem hoje nenhum lugar para clicar.** A chamada que
liga isso (`Client::emprestar_subida`, em `crates/seele-core/src/client.rs`, que
manda `ClientMessage::EmprestarSubida` — `crates/seele-proto/src/control.rs`)
não é acionada por nenhum comando do app gráfico: nenhuma das funções
`#[tauri::command]` de `apps/seele-app/src/main.rs` a chama, e nenhum arquivo em
`apps/seele-app/ui/` menciona "emprestar" ou "subida". Hoje só quem a chama é o
teste de integração `crates/seele-conformance/tests/tela_por_um_par.rs`, que
roda em processo, sem rede de verdade — não é algo que se aperte um botão para
fazer.

Isto já estava anotado no diário desta malha (`progress.md`, Task 8): *"o
caminho de LAN do §3.1 fica inerte até o opt-in existir na interface."* Antes
de reunir três máquinas, confira se essa lacuna já foi fechada — procure por
`emprestar` em `apps/seele-app/src/main.rs` e em `apps/seele-app/ui/`. Se
continuar vazio, este roteiro não tem como ser executado por alguém sem abrir o
código: falta uma forma de a pessoa dizer "eu empresto", nem que seja
provisória. Registre esse achado antes de tentar rodar o resto desta seção — é
mais útil do que uma tarde perdida procurando um botão que não existe.

### A montagem, quando houver por onde ligar o empréstimo

Precisa de **três** máquinas, ou de duas mais um celular em 4G — o essencial é
que **quem empresta e quem assiste não estejam na mesma rede**. Uma máquina
sozinha (o servidor e quem compartilha) pode estar em qualquer rede; as outras
duas é que têm de ser redes diferentes entre si, ou o furo nunca é exercitado —
mesmo aviso que a seção 7 já faz para o degrau 4.

1. Numa máquina, suba o servidor (seção 1) e entre pelo app (seção 2). Entre na
   sala de voz e compartilhe a tela: o botão **COMPARTILHAR** no rodapé,
   escolha a janela ou o monitor.
2. **(bloqueado)** Numa segunda rede, entre com uma pessoa e ligue o
   empréstimo de subida nela — ver acima: hoje não tem onde clicar.
3. Numa terceira rede (ou no celular em 4G), entre com outra pessoa e clique
   no nome de quem está compartilhando para assistir. Não escolha "pelo par"
   ou "pelo servidor" — a escolha de quem serve é automática e não segue
   critério visível nenhum (nem latência, nem ordem de chegada), e é
   justamente ela que os dois números abaixo medem.

Anote, do `tracing` de quem assistiu (a terceira máquina):

| o que | onde ler | campo | anote |
|---|---|---|---|
| como a ligação chegou | evento `um par ligou` | `como` | `Local` ou `Furo` |
| ida e volta com o par | evento `um par ligou` | `ida_e_volta` | o valor já vem formatado com unidade (ex.: `12.345ms`) — não é número cru |
| quando não ligou, o motivo enumerado | evento `o par não veio; a tela vem do servidor` | `motivo` | o valor, por extenso |
| quando não ligou, o detalhe | mesmo evento acima | `erro` | a frase inteira, útil para achar a causa |

As duas linhas de `tracing` são emitidas por `crate::par` (`crates/seele-core/src/par.rs`)
no processo de quem assistiu — é lá, e não no de quem empresta, que a discagem
e a decisão acontecem.

Repita **umas dez vezes**, em redes diferentes se der. O que se quer é a
**fração** de `Furo` sobre tentativas, e ela é o número que decide o desenho da
árvore: se o furo falhar em boa parte dos pares, o subprojeto B não pode supor
que qualquer par se alcança, e vira «árvore entre quem se alcança, estrela para
o resto».

**Dez é o piso, não a meta.** É uma fração binária decidindo o desenho de uma
árvore — dez tentativas dão um intervalo de confiança largo demais para
sustentar sozinho essa decisão. Quanto mais tentativas, e quanto mais redes
diferentes entre elas, melhor: quem for ler os dois números depois precisa
saber quantas tentativas e quantas redes distintas os produziram, não só a
fração final.

### Uma ressalva que a medida não pode esconder

`ComoChegou` (`crates/seele-core/src/par.rs`) classifica por **faixa de
endereço**, não por ter havido furo de verdade — é o que `como_chegou` faz:
olha se o IP é privado, loopback, link-local ou ULA, e nada além disso. Duas
máquinas da mesma casa que se alcancem pelo endereço público — hairpin de
NAT — contam como `Furo` sem furo nenhum. Um endereço IPv6 global sem NAT,
travado só por firewall, também conta como `Furo`. **Os dois inflam o
numerador** da fração acima.

Não dá para o número corrigir isso sozinho — anote a topologia ao lado dele:
que rede era cada máquina, se havia CGNAT, se o roteador tinha UPnP ligado. Sem
essa nota, a fração medida não quer dizer o que promete.

Escreva os dois números — o custo de um salto (a mediana do `ida_e_volta`) e a
fração de furo — em `docs/m1-medicoes.md`, ao lado dos outros. **Enquanto eles
não existirem, a aritmética da malha na spec continua sendo estimativa**, e ela
já está marcada como tal em `docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md`
(§2: *"dois números que hoje são estimativa"*).

## O que a suíte local prova, e o que ela não tem como provar

**Escrito em 2026-09-10**, junto do consentimento de dois lados. A separação
existe porque as duas coisas já foram confundidas neste repositório, e a
confusão sempre anda no mesmo sentido: um teste verde em `127.0.0.1` sendo lido
como se dissesse algo sobre uma sala de gente de verdade.

### Prova local, automática, a cada `cargo test`

`crates/seele-conformance/tests/tela_por_um_par.rs` sobe um servidor de verdade
e clientes de verdade, e prende **comportamento**:

- que o quadro atravessa o par, e que o servidor **não** subiu aquela cópia;
- que quem consentiu em assistir por par é apontado, e que quem **recusa** não
  tem o endereço entregue a ninguém e continua vendo a tela pelo servidor, com
  a imagem inteira — quem recusa não fica sem tela;
- que a retirada do consentimento — dos dois lados — encerra o repasse e
  devolve quem assistia ao servidor sem perder imagem;
- que a saída de quem empresta reabre o cano de quem ficava atrás dele **sem
  depender de relato nenhum** da outra ponta;
- que a vaga do par volta à fila em todo caminho que encerra um repasse.

São três pessoas e uma transmissão, tudo na mesma máquina, pela interface de
loopback.

Cada uma dessas afirmações foi provada por **reversão**: o guarda correspondente
foi desfeito no código e o teste ficou vermelho, com a mensagem nomeando o
defeito. Um teste que passa com e sem o conserto não prova o conserto, e é esse
o filtro que separa esta lista de uma lista de nomes de teste.

### O que ela **não** diz, e nenhuma execução dela vai dizer

1. **Nada sobre furo de NAT.** Em `127.0.0.1` não há NAT para furar. É a seção
   de duas máquinas acima que produz esse número.
2. **Nada sobre a subida de ninguém.** Loopback não tem cano: o repasse entre
   dois processos da mesma máquina não gasta a internet de nenhuma delas, que é
   exatamente a grandeza que a malha existe para economizar.
3. **Nada sobre qual mecanismo salvou a imagem, quando há mais de um.** Medido
   em 2026-09-11: quando quem empresta sai da sala, o cano de quem ficou órfão
   é reaberto por dois caminhos independentes — o servidor, na própria saída, e
   o relato de fim de repasse que a máquina do órfão manda ao perceber. Em
   `127.0.0.1` o segundo volta dentro de um intervalo de quadro, então a imagem
   não pisca nem quando o primeiro é removido do código. Separar os dois exigiu
   uma ponta de teste que **não sabe relatar**; numa rede de verdade, com
   latência e perda, a diferença entre os dois caminhos é justamente o que a
   pessoa veria congelar.
4. **Nada sobre salas com mais de cinco pessoas.** O pedido que originou o
   subprojeto (§0 do desenho de 05/09) é *«numa call de 5 pessoas, 2 querem
   transmitir a tela»*, e a propriedade pedida é que **o número de pessoas
   deixe de ser um problema**. Uma propriedade sobre N não se prova com N
   fixo em três.

### A medição real, e por que ela é outra coisa

Para dizer qualquer coisa sobre a aritmética do §0 é preciso **mais de cinco
participantes em máquinas distintas, em redes distintas**, com duas
transmissões no ar. O que se anota lá é o que a suíte local não alcança:

| o que | como se lê |
|---|---|
| subida de quem hospeda, com e sem malha | o contador do servidor, e a conta de quantas cópias ele subiu |
| qualidade vista por cada pessoa | o rodapé de telemetria em cada máquina, não numa só |
| fração de furo com N pares | a seção de duas máquinas acima, repetida por par |
| o que acontece quando um par cai numa sala cheia | quem ficou órfão voltou ao servidor, e em quanto tempo |

**Enquanto essa medição não existir, nenhum número da malha pode ser
apresentado como medido.** A suíte local prova que o mecanismo funciona; ela
não prova que ele alivia. As duas frases são diferentes, e escrever a segunda
com a evidência da primeira é o erro que este documento existe para impedir.
