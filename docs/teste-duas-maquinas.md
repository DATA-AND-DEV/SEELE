# Teste entre duas máquinas

O que nenhum teste automático deste repositório cobre: voz real, por microfone
real, entre dois computadores numa rede real. É a validação que fecha M1.15 e
M1.16, e é a única que depende de você.

Leva vinte minutos. Anote os resultados em `docs/m1-medicoes.md`.

---

## Antes de começar

Uma máquina é o **Dogma** (servidor + cliente) e a outra é só cliente. Podem ser
os dois sistemas operacionais que você tiver — quanto mais diferentes, melhor,
porque a matriz de três SOs em CI nunca executou.

Em cada máquina:

```sh
cargo build --release --bin seeled --bin plug
```

Se a máquina tiver interface gráfica e você quiser testar o app junto:

```sh
cargo build --release -p seele-app
```

---

## 1 · Subir o Dogma

Na máquina A:

```sh
./target/release/seeled 0.0.0.0:8383
```

Ele imprime três coisas que importam:

```
seeled listening on 0.0.0.0:8383

na outra máquina:
  plug --server 192.168.x.x:8383

certificate fingerprint: 50217d68c6...
```

**Anote a impressão digital.** É o que o ADR 0003 pede que você confira: o
cliente vai fixá-la no primeiro contato e recusar a conexão em silêncio se ela
mudar depois.

Se a linha "na outra máquina" não aparecer, o `seeled` não achou um endereço de
rede — provavelmente está sem rede, ou só em loopback.

### Fechar o Dogma, se quiser

Por padrão qualquer um que alcance a porta entra — o certo para testar em rede
local, e o `seeled` avisa ao subir assim. Para fechar:

```sh
./target/release/seeled convite ayanami   # link de uso único, sete dias
./target/release/seeled senha "a senha"   # ou um segredo para o grupo
./target/release/seeled senha --remover   # volta a aceitar qualquer um
```

O convite sai como um link pronto para mandar:

```
seele://192.168.x.x:8383?fp=782cc791…&convite=2QKPAXPP97W5459H3TPA
```

Ele carrega a impressão digital do certificado, então quem receber **não
precisa conferi-la por outro canal** — o cliente compara sozinho e recusa se
não bater. Do outro lado:

```sh
./target/release/plug --url "seele://…" --nick seunome
```

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
./target/release/plug --server 192.168.x.x:8383 --nick seunome
```

Na primeira vez o cliente mostra `PRIMEIRO CONTATO — CHAVE FIXADA` com uma
impressão digital. **Confira contra a que o `seeled` imprimiu.** Se conferir, é
esse servidor. Se não conferir, alguém está no meio — e é exatamente para esse
momento que o ADR 0003 existe.

Conecte também na máquina A, com **apelido diferente**:

```sh
./target/release/plug --server 127.0.0.1:8383 --nick outronome
```

> Dois clientes com o mesmo `$SEELE_HOME` são **a mesma pessoa** — o PERSISTENCE
> vincula o apelido à identidade que o reivindicou (ADR 0017). Para serem dois
> pessoas na mesma máquina, `SEELE_HOME=~/.seele-outro`.

---

## 3 · Texto primeiro

Antes de qualquer coisa com áudio, prove que o enlace existe.

| | esperado |
|---|---|
| Os dois se veem no roster | cada um vê o outro na lista sob o Cage |
| `i`, digitar, `Enter` na máquina A | aparece na B em menos de um segundo |
| O mesmo de B para A | idem |
| `:sync` nas duas | RTT plausível para a rede (LAN cabeada: 1–5 ms; wifi: 5–30 ms) |

Se o texto não atravessar, o áudio também não vai. Pare aqui e resolva a rede.

---

## 4 · Voz

Fones nos dois lados. **Sem fones haverá realimentação**, e o cancelamento de
eco não existe neste produto (ADR 0007 adiou explicitamente).

Segure a barra de espaço no modo Normal e fale.

> Dependendo do terminal, segurar funciona de verdade ou vira trava — aperta
> para abrir, aperta de novo para fechar. kitty, foot, WezTerm e Ghostty
> relatam soltura de tecla; Terminal.app, iTerm2 e a maioria do que se alcança
> por SSH não relatam (ADR 0016). A barra de telemetria diz qual estado está
> valendo, e o `●` no roster também.

Anote:

| pergunta | o que observar |
|---|---|
| A voz chega? | inteligível, sem robotização |
| Há eco ou realimentação? | só deve haver se alguém estiver sem fones |
| O `●` acende no roster do outro? | o `speaking` vem de datagrama chegando, não de o cliente afirmar |
| `A.T. OFF` → `m` → o outro vê `A.T.`? | o mudo é anunciado, não só local |
| Qual a Taxa de Sincronização em repouso? | `:sync` nos dois lados |

---

## 5 · As três medições que faltam

Estas são M1.16, e são o motivo deste documento.

### 5.1 · Soak de 10 minutos

Fiquem os dois no Cage por dez minutos com conversa intermitente. O que se
procura é o que só aparece com tempo: **estalos**, e a deriva de clock que a
M1.8 corrige (`docs/m1-medicoes.md` tem a tabela de deriva medida).

Ao fim, `:sync` nos dois. Anote:

- estalos ouvidos, e mais ou menos quando
- Taxa de Sincronização no início e no fim
- `OPUS` na barra mudou de valor?
- a linha `LOCAL`: o `saída` **cresceu** ao longo dos dez minutos?
- a linha `RITMO`: quantos ppm, o `anel` longe do alvo, e quanto de `grampo`

As duas últimas são a pendência 2, e agora têm resposta com número. `saída`
crescendo é amostra que o dispositivo pediu e não tinha; `RITMO` diz por quê, e
cada resposta manda para um lugar diferente:

- **ppm de dezenas, anel perto do alvo, grampo zero**: a malha está segurando a
  deriva. É o normal, mesmo com o `saída` tendo crescido no arranque.
- **grampo crescendo**: a razão pedida saiu da faixa em que cristal vive, então
  a causa não é deriva — taxa diferente da anunciada (`:audio` diz a taxa), ou
  dispositivo trocado.
- **reposição crescendo**: o anel raspou o fundo de novo. Aí é a volta do laço,
  que é a pendência 15, e o número a pedir é o `LAÇO volta`.

Antes do soak, cada máquina pode dar o próprio veredito sozinha, em um minuto e
sem a outra:

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

Anote também o que a barra mostra em `LOSS`. Ela deve refletir a perda induzida;
se marcar zero com 5% de perda real, a medição está errada e isso é um defeito.

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

Some: `latência local de A` + `RTT/2` + `profundidade do jitter buffer` +
`latência local de B`. Compare com os ≈67/87 ms que o ADR 0009 orça a partir das
medições de M1.

---

## 6 · Se o app gráfico estiver na jogada

Rode o `seele-app` numa das máquinas em vez do `plug` e repita as seções 3 e 4. A
composição é a mesma de propósito: mesmos três painéis, telemetria no rodapé.

Aqui a barra de espaço segura de verdade — a janela relata soltura.

O que checar a mais:

- o app e o `plug` no mesmo `$SEELE_HOME` são o mesmo pessoa (retomada de sessão)
- o histórico aparece ao abrir a Linha, com autor e horário corretos
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
força a escada a chegar ao degrau 4):

```sh
SEELE_ENCONTRO=<endereço-da-vps>:8384 ./target/release/plug --hospedar
```

O que checar, em ordem:

1. A frase embaixo do link diz **"um ponto de encontro apresentou esta
   máquina"**. Se disser "só funciona na sua rede", o degrau 4 não subiu: o
   terminal do `plug` diz por quê, e o `--barulhento` da VPS diz se o pedido
   chegou lá.
2. O link tem um `enc=` com **duas metades** separadas por `/`, e um endereço
   público seu no `alt=`.
3. Na máquina B, na outra rede, cole o link. Ela entra.
4. Na VPS, o `--barulhento` mostra **duas** apresentações: a da máquina A se
   descobrindo, e a da máquina B chegando. Nunca mais que isso — se aparecer
   tráfego contínuo ali, alguma coisa está passando pelo ponto de encontro que
   não deveria.
5. **Desligue o ponto de encontro** e hospede de novo. O `plug` tem de subir na
   mesma velocidade de antes (mais no máximo um segundo), com o link levando os
   endereços de sempre e **sem** `enc=`. Este é o teste de que o degrau 4 não
   virou ponto único de falha.
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
| Taxa nativa relatada pelo `plug` | | |
| Latência por cabo (M1.2) | | |
| Latência no ar (M1.2) | | |
| Estalos em 10 min | | |
| Inteligível a 5% de perda | | |
| Troca de dispositivo a quente funciona | | |
| RSS do `plug` após 10 min | | |

A troca a quente é M1.14: com a chamada rodando, tire o fone USB e coloque de
volta. A chamada deve pausar e retomar, não morrer.

---

## O que fazer com os resultados

Anote em `docs/m1-medicoes.md`, que já tem as medições sintéticas de M1 e é onde
os números reais devem ficar ao lado delas. Onde a realidade divergir das specs,
`specs/10-convencoes.md` exige corrigir `00` e `03` — é a tarefa M1.17, e ela só
pode ser feita depois disto.

Se algo falhar, o mais útil é: qual seção, o que a barra de telemetria mostrava,
e o que o `seeled` imprimiu no terminal dele naquele momento.
