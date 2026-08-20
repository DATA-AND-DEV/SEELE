# Conectividade P2P: o degrau 4 que abre fora de hora

**Data:** 2026-08-20
**Estado:** aprovado, aguardando plano

Primeiro dos dois ciclos que a conversa de hoje abriu. O outro —
compartilhamento de tela — é subsistema separado, com spec e plano próprios, e
depende deste: sem caminho direto confiável ele não tem onde rodar.

Este spec nasceu de um documento de arquitetura escrito de fora
(`SEELE — P2P Networking Development Specification.md`, na raiz) somado a um
teste de campo que falhou. O documento de fora pede uma reestruturação da camada
de networking; o repositório mostra que quase tudo que ele pede já está
construído. O que sobra é menor, mais específico e mais grave.

## O problema

**O degrau 4 do ADR 0022 está construído, declara sucesso, e não conecta.** Um
teste de campo com duas casas falhou com o anfitrião imprimindo
`UM PONTO DE ENCONTRO ABRIU O CAMINHO` — ou seja, ele falou com o ponto de
encontro, aprendeu o próprio endereço público e o pôs no convite. A falha está
depois disso.

A causa é o relógio, e ela é visível sem rede nenhuma. Quem entra
(`seele-core/src/encontro.rs::bater`, chamado em `enlace.rs:451`) manda dois
avisos ao ponto de encontro **antes do laço de candidatos** e depois percorre a
lista em série, com `PRAZO_POR_CANDIDATO = 4 s` (`enlace.rs:369`):

```
t=0ms     LEVE #1
t=80ms    LEVE #2
t=80ms    QUIC no candidato 0 — o endereço da REDE LOCAL do anfitrião
t=4080ms  candidato 1
t=8080ms  candidato 2
t=12080ms candidato 3   ← o endereço furado costuma estar aqui
```

Quem hospeda (`seele-server/src/alcance/encontro.rs::atender` e `::furar`) fura
por `PACOTES_DO_FURO × INTERVALO_DO_FURO` ≈ 600 ms a partir do `AQUI`, e nunca
mais. O `REAVIVAR` de 15 s remanda `ONDE`/`LEVE` ao ponto de encontro para manter
o mapeamento do próprio anfitrião vivo; ele **não fura de novo**.

**O furo abre e fecha até onze segundos antes do aperto de mão que ele existia
para deixar entrar.** Numa casa com CGNAT — o único caso que o degrau 4 existe
para servir — o candidato 0 é um `192.168.x.x` que, visto de outra casa, não
devolve ICMP nenhum: ele consome os quatro segundos inteiros.

Isto é independente de NAT simétrico. Qualquer teste de campo feito antes de
consertá-lo mede as duas coisas ao mesmo tempo, e foi o que aconteceu.

### Por que nenhum teste pegou isto

`crates/seele-conformance/tests/candidatos.rs` prova que o próximo candidato é
tentado. `crates/seele-encontro/tests/apresentacao.rs` prova que o aviso chega.
**O intervalo entre as duas coisas não é de ninguém.** Nenhum teste do projeto
olha para o relógio entre o aviso e o aperto de mão, e é exatamente ali que o
defeito mora.

## O que já existe, e não muda

Vale escrever para ninguém reabrir o que já foi decidido. O documento de fora
pede as coisas abaixo como se fossem novas; elas são o repositório de hoje:

| Pedido | Onde já está |
|---|---|
| Servidor só como rendezvous, tráfego direto entre pares | ADR 0022, escada de cinco degraus |
| Zero áudio, texto, arquivo ou tela no servidor | degrau 5 (retransmissão) fora de escopo por decisão |
| Servidor determina o endpoint pelo socket; o cliente não informa | `ONDE`/`LEVE` em `seele-proto/src/encontro.rs` |
| Rendezvous sem estado | `responder` é função livre, sem `self`, sem tabela |
| Não assumir que IP/porta descobertos bastam | é a premissa do ADR 0022 |
| Sem port forwarding manual, sem IP público na mão | degraus 2, 3 e 4 |
| QUIC mantido | quinn 0.11.11, intacto |
| Servidor não confiável para os dados | TOFU + TLS 1.3 ponta a ponta |
| Custo do servidor não cresce com o número de chamadas | vale hoje |

Uma linha em que o SEELE é **mais forte** que o documento de fora, e que não pode
regredir ao construir: a §15 dele aceita o rendezvous guardar
`Room / PeerID / endpoint / last seen` com TTL. O `responder` do SEELE não guarda
nada — não porque alguém limpa, mas porque não há onde. Isso é propriedade do
tipo, e continua sendo.

## Decisões desta sessão

**Invariantes preservadas.** Quem entra nunca lê resposta do ponto de encontro;
todos os endereços que ele tenta vêm do `seele://`. Sem relay. Amplificação do
ponto de encontro ≤ 1:1. Nada de ICE bidirecional. A comparação com `iroh` que a
§8/§9 do documento de fora pede fica **respondida por decisão**, tomada em
2026-08-20: o desenho atual se mantém, e reabrir isso exige motivo novo.

**Decidido hoje:**

1. `Chegada` (o gerente de conexão) é de **uso único**; o `Motor` constrói uma
   nova por tentativa de reconexão.
2. A trilha de uma chegada **que falhou** sobrevive e atravessa o `seele-ffi`.
3. Os dois defeitos adjacentes descobertos hoje entram neste ciclo.
4. Quem entra **pode ler** o datagrama `FURO`: ele vem do anfitrião, não do ponto
   de encontro, e a invariante é sobre o ponto de encontro.
5. O demultiplexador mora num crate novo, `seele-udp`.
6. Quando o anfitrião muda de rede, o `seele://` é **regenerado, com aviso na
   tela**.
7. O convite **não** ganha campo `tipos=`; o tipo do candidato é deduzido.
8. `atender` passa a conferir a origem do `AQUI`.
9. O diagnóstico aceita N pontos de encontro e classifica NAT só com ≥ 2; com um,
   imprime `DESCONHECIDO`.
10. O jitter da tela passa a ser o de chegada (RFC 3550), não a profundidade do
    anel de reprodução.

## 1 · O gerente de conexão: `Chegada`

**Só existe do lado de quem entra**, em `crates/seele-core/src/chegada.rs`.

O ADR 0002 fecha a alternativa antes de ela ficar interessante: `seele-server`
pode depender de `seele-proto` **e mais nada**, e um gerente único teria de morar
no `seele-proto` — arrastando quinn e tokio para o crate que existe para não
depender de ninguém — ou no `seele-core`, e aí o `seeled` herdaria `seele-audio`,
`cpal` e o libopus dentro de um daemon que precisa caber em 1 vCPU.

Mas o argumento decisivo não é de arrumação: **as duas metades não são a mesma
máquina de estados.** `Escada::subir` roda uma vez, na subida do Dogma, quando
não existe par nenhum, e o que ela produz não é uma conexão — é uma frase e uma
lista de endereços para o `seele://`. Ciclo de vida com começo, tentativas e fim
só existe do lado de quem chega. A costura entre as metades continua sendo o
`seele://` e o `SEELE-ENC/1`; nenhum tipo novo atravessa a fronteira.

### Três dos estados propostos não descrevem nada que aconteça aqui

A §11 do documento de fora assume ICE. Onde ela não cabe:

- **`DISCOVERING` / `CANDIDATES_FOUND`** — quem entra não descobre candidato
  nenhum. Eles chegam prontos no `seele://`, já ordenados e já truncados. A
  descoberta é do outro lado e aconteceu antes deste processo existir.
- **`PATH_ESTABLISHED`** — não era afirmável sem mentir, porque nada deste lado
  jamais aprendia que o furo abriu. **A decisão 4 muda isso**: com o `FURO` sendo
  lido, o estado passa a existir e a ser honesto. Ele não decide para onde
  conectar — só antecipa o instante da tentativa. E marca não é autenticação;
  isso fica escrito no tipo.
- **`NAT_TRAVERSAL_FAILED` / `DISCOVERY_FAILED`** — o que se observa é "todos os
  candidatos falharam". Atribuir isso ao furo é chute, e o `plug --rede` é o lugar
  certo para responder por quê.

```rust
pub enum Etapa {
    Parada { candidatos: u8, com_bilhete: bool },
    Avisando { ponto: String },
    Tentando { candidato: u8, de: u8, onde: SocketAddr },
    CaminhoAberto { onde: SocketAddr },   // um FURO com a marca certa chegou
    Dentro,
    Desistiu(ConnectError),
}
```

Transições legais: `Parada → Avisando` (só com `enc=`), `Parada → Tentando`,
`Avisando → Tentando`, `Tentando → CaminhoAberto`, `Tentando|CaminhoAberto →
Tentando{i+1}`, `Tentando|CaminhoAberto → Dentro`, e do último candidato para
`Desistiu`.

**Uma ausência que é requisito: não existe `Avisando → Desistiu`.** Um ponto de
encontro fora do ar, um nome que não resolve, um convite sem impressão digital —
nenhum pode reprovar uma chegada, porque nenhum endereço do convite depende dele.
Hoje isso é garantido por um prazo de 600 ms; ali vira uma aresta que não se pode
escrever.

`Desistiu` carrega o `ConnectError` que já existe. Achatar `PinChanged` e
`InviteMismatch` num `QUIC_FAILED` apagaria o alarme do ADR 0003: são os dois
erros que **não são de rede**.

### Interface

```rust
impl Chegada {
    pub fn nova(destinos: Vec<Destino>, bilhete: Option<Bilhete>) -> Self;
    pub fn acompanhar(&self) -> tokio::sync::watch::Receiver<Etapa>;
    pub async fn chegar(self, chave: SigningKey, pins: Arc<dyn PinStore>)
        -> Result<Enlace, ConnectError>;
    pub fn trilha(&self) -> &[Passo];
}
```

`Passo { etapa: Etapa, em: Duration }`, um por transição — nunca por retentativa
interna. Os nomes de `Etapa` são **identificadores estáveis, nunca frases**, pela
regra de `Degrau::nome()`: a frase mora na casca. A trilha sobrevive ao fim, dê
certo ou não, e **não acrescenta conhecimento** — todo endereço nela já estava no
convite de quem a lê, o que mantém o custo de privacidade em zero.

### Migração, sem big-bang

1. Criar `chegada.rs` com `Etapa`, `Passo` e uma `Chegada` que **delega** a
   `Enlace::conectar_entre_com_bilhete` sem mover uma linha. Trocar as duas
   chamadas: `seele-tui/src/main.rs:600` e `seele-ffi/src/lib.rs:1542`. O app já
   ganha estados onde hoje tem um spinner mudo.
2. Mover o laço de `Enlace::tentar_entre` para `Chegada::tentar`.
   `Enlace::conectar_por` **fica** onde está: ele monta o `Motor`. Essa é a linha
   de corte — a `Chegada` é dona do laço e do socket furado, o `Enlace` é dono da
   sessão.
3. Dar a lista inteira de candidatos ao `Motor` (ver §7).
4. **Por último**, fechar `Enlace::conectar_entre` e `conectar_entre_com_bilhete`
   como `pub(crate)`. Oito arquivos de `seele-conformance` usam a porta pública;
   fechá-la cedo transforma um passo barato numa alteração de oito arquivos de
   teste no mesmo commit.

### O que a `Chegada` não faz

Não abre o endpoint QUIC, não decide TOFU, não é dona da bateria, não lê o
`seele://`, não mede RTT nem jitter, não sobe escada, não classifica NAT, e não
escreve a frase que a pessoa lê.

## 2 · Candidatos e coordenação do furo

### O tipo do candidato, e a ordem que sai dele

Hoje o tipo **é** a ordem: `Vec<(u8, SocketAddr)>` em `alcance.rs::decidir`, com
0 = rede de casa até 5 = ponte virtual. Isso responde "onde ele vai na lista" e
não responde "o que ele precisa para funcionar", que é a pergunta desta seção.

```rust
pub enum Tipo { Local, Global, PortaNoRoteador, Refletido, Tunel, Ponte }

impl Tipo {
    fn ordem(self) -> u8;              // Local=0 … Ponte=5
    fn precisa_de_furo(self) -> bool;  // só Refletido
    fn insubstituivel(self) -> bool;   // Local e Refletido
}
```

`Local`/`Global` são os *host candidates*; `Refletido` é o *server-reflexive*.
`PortaNoRoteador` é refletido por configuração e não por observação, e merece
variante própria porque não precisa de furo nenhum.

O `seele://` **não muda**: `alt=` continua sendo endereços nus na mesma ordem, e
a ordem escrita continua sendo a ordem derivada. O tipo é deduzido na leitura —
*é `Refletido` todo candidato não-privado num convite que traga `enc=`* —, e a
dedução acerta porque o degrau 4 só é tentado quando `mapeada.is_none()`
(`alcance.rs:698`), então nunca há dois públicos disputando. **Essa exclusividade
vira teste**, porque ela é o que sustenta a dedução; um degrau novo que a quebre
tem de reprovar antes de chegar a campo.

### Avisar tarde

`bater()` hoje faz três coisas — abre socket, resolve o nome, manda dois avisos.
Vira duas:

- `Batida::preparar(&Bilhete, impressao) -> Option<Batida>` — socket de pilha
  dupla, resolução do ponto **uma vez**, marca. Dentro do `PRAZO` de 600 ms, e
  **sem mandar pacote nenhum**.
- `Batida::avisar(&self)` — um `send_to` de 96 bytes, `WouldBlock` ignorado.

A `Batida` fica viva no laço até o fim — o mapeamento de NAT é do socket, não da
mensagem — e empresta um `try_clone` por tentativa, como já faz. `AVISOS` e
`INTERVALO` somem; entram `ESPERA_DO_FURO = 200 ms` e `AVISOS_POR_CANDIDATO = 3`
a cada 700 ms.

Os 200 ms são o que tem de caber entre o `LEVE` sair daqui e o `Initial` chegar
ao NAT de lá: uma perna até o ponto mais uma perna do ponto até o anfitrião, que
somadas dão a ida e volta que o ADR 0022 mediu em 20–200 ms. Errar para baixo
custa um PTO do quinn (~1 s, dentro dos 4 s do candidato); errar para cima é pago
sempre. Então erra-se para baixo. **Com o `FURO` sendo lido (decisão 4), a espera
termina cedo quando o furo chega antes** — os 200 ms viram teto, não piso.

**Candidato que não precisa de furo não avisa e não espera.** O caso "os dois na
mesma casa" não ganha um milissegundo.

```
QUEM ENTRA                                QUEM HOSPEDA
t=0     socket + DNS (uma vez)
t=0     cand.0 Local → QUIC, sem aviso
t=1000  cand.1 Local distante → prazo curto
t=2000  cand.2 Refletido: LEVE #1          t≈2075  o ponto manda AQUI
t=2200  QUIC neste candidato (ou antes,    t≈2100  chega AQUI → 1 FURO sai
        se o FURO chegar)                  t≈2100  o mapeamento existe
t=2900  LEVE #2 (conexão em curso)         t≈3000  AQUI #2 → mais 1 FURO
t=3600  LEVE #3                            t≈3700  AQUI #3 → mais 1 FURO
t=6200  fim do prazo deste candidato
```

### `PACOTES_DO_FURO` cai de 5 para 1

O mapeamento de NAT nasce quando o pacote **sai** do roteador do anfitrião, não
quando chega ao outro lado. Os cinco pacotes nunca compraram resistência a perda:
compravam cobertura temporal, que agora vem do aviso colado à tentativa.

Consequência de segurança, e ela melhora: um atacante gasta um datagrama de 96
bytes — um `LEVE` forjado, ou um `AQUI` forjado direto na escuta de avisos — e faz
chegar à vítima escolhida **um** datagrama de 96 bytes. **1:1 na vítima**, contra
5:1 hoje. O teto por Dogma continua sendo `FUROS_POR_JANELA = 20` a cada 10 s.
Quem repete paga 96 bytes por repetição, e essa é a propriedade estrutural.

`precisa_de_furo()` é o que torna a repetição segura: sem ele, um convite de
quatro candidatos queimaria quatro dos vinte furos da janela, e cinco pessoas
entrando juntas fechariam a janela contra gente legítima.

E `atender` passa a conferir a origem do `AQUI` (decisão 8): hoje `recv_from`
descarta o remetente em `(lidos, _)`, e forjar um `AQUI` é mais barato que forjar
um `LEVE`, porque não passa pelo ponto de encontro.

### O candidato morto

Quatro segundos num `192.168.x.x` visto de outra casa, e ele vem primeiro.
**Distinguir, e encurtar só o distinguido** — não reordenar, não encurtar o prazo
geral.

`UdpSocket::bind(":0")` + `connect(candidato)` + `local_addr()` responde qual
endereço **meu** o núcleo usaria para alcançar **aquele** destino. Isto não é a
pergunta que o ADR 0022 reprovou: aquela era "qual é o meu endereço", respondida
pela rota padrão, e uma VPN a capturava. Esta tem destino, e é a que o `connect`
realmente responde.

Se o candidato é privado (RFC 1918, ULA, e `100.64.0.0/10`) e a origem escolhida
não compartilha a rede dele, ele é de outra casa: `PRAZO_DE_CANDIDATO_DISTANTE
= 1 s`. **Nunca descartar** — um /16 configurado à mão ou uma VPN capturando a
rota dão falso negativo, e falso negativo só encurta. O pior caso cai de 12 s para
3 s antes do candidato que importa.

### Retentativa

Hoje não há: dois avisos antes do laço, e um `AQUI` perdido custa a conexão
inteira em silêncio. Com o desenho acima, a retentativa é o próprio laço — três
avisos por candidato refletido, a 700 ms, enquanto o aperto de mão corre. Cobre
perda do `LEVE`, do `AQUI` e da janela de furos cheia, e para sozinha quando a
conexão sobe.

**Não há segunda passada pela lista**: cada tentativa fixa chave, gasta o convite
de uso único do ADR 0021 e aparece no log de quem hospeda.

## 3 · Os três defeitos adjacentes

Descobertos ao desenhar, confirmados no código, e nenhum estava no escopo
original. Entram porque os dois primeiros **contaminam a medição** do furo.

### 3.1 · O degrau é declarado antes do truncamento

`alcance.rs:563` diz, em comentário: *"O degrau é lido dos candidatos que
sobraram, e não do caminho que os produziu: assim não há como dizer que se alcança
por um endereço que não está no convite."* O código logo abaixo lê
`externo.is_some()` e `furado.is_some()` — variáveis de **antes** do
`alvos.truncate(LIMITE_DE_CANDIDATOS)` da linha 551.

Numa máquina com Ethernet e Wi-Fi ligadas numa rede com IPv6 nativo há dois
endereços de ordem 0 e dois de ordem 1: **quatro**. O endereço furado (ordem 3)
sai do convite, e a escada continua anunciando `FuroDeNat`, reavivando um caminho
que candidato nenhum usa. É a forma exata de silêncio confiante que o ADR 0022
existe para não produzir, e é compatível com o relato de campo.

Há uma **segunda truncagem na mesma direção**: `LIMITE_DE_ALVOS = 4` em
`uri.rs:79`, cortando em `uri.rs:146` e `uri.rs:418`.

**Conserto:** reservar antes de cortar. Separar os `insubstituivel()` — o primeiro
`Local` e o `Refletido` —, preencher as vagas restantes pela ordem, reordenar por
`ordem()`. Na prática: no máximo dois `Local`, e o refletido nunca cai. E `degrau`
passa a perguntar `alvos.contains(&furado)`.

### 3.2 · Uma VPS lê "este link só funciona na sua rede"

`alcance.rs:698` só tenta o degrau 4 quando
`mapeada.is_none() && !tem_ipv4_global(&achados)` — correto, e o ADR 0022
justifica por escrito. Mas `decidir` **não tem ramo para "a máquina tem IPv4
global"**: sem UPnP, sem furo, sem IPv6 e sem túnel, cai no `else` final,
`SoRedeLocal`.

O `enum Degrau` tem cinco variantes e nenhuma é o degrau 1 do ADR — "endereço
direto: rede local, VPS, porta encaminhada à mão" —, que o próprio ADR chama de
*"o caminho de quem hospeda a sério"*. Quem hospeda numa VPS recebe hoje a pior
frase da escada embaixo de um link que alcança o mundo inteiro.

**Conserto:** variante `Degrau::EnderecoDireto` e frase própria em `frases.js`,
pelo mesmo caminho que o projeto percorreu ao criar `RedeLocalOuVpn`. É o mesmo
defeito do relato do Cloudflare WARP com o sinal invertido: lá a frase prometia
demais, aqui ela promete de menos.

### 3.3 · O JITTER da tela é sempre zero

`session.rs:1584` manda `jitter_ms: 0.0`, com o comentário correto de que o
servidor não pode saber um número que se mede no receptor. E
`seele-ffi/src/lib.rs:1171` lê o jitter **do relatório do Dogma** — ou seja, lê o
zero. O número real do receptor existe (`lib.rs:1820`) e vai para o Sync Ratio.

**Conserto:** a tela passa a ler o jitter de chegada da RFC 3550, do
`SourceTelemetry`. Explicitamente **não** `worst_jitter_depth_ms`: aquilo é
profundidade do anel de reprodução, e o ADR 0028 acabou de dar um alvo a ele —
mostrá-lo como "jitter" exibiria a nossa própria reserva como ruído da rede.

## 4 · `plug --rede`, o diagnóstico

Subcomando do `plug`. Não binário novo, não `xtask`, não exemplo.

O peso que o ADR 0022 cobra é a **árvore de dependências do daemon**, e este
diagnóstico não acrescenta nenhuma: `seele-tui` já depende de `seele-core` **e**
de `seele-server`, com a exceção nomeada em `xtask/src/check_deps.rs`. Duas regras
tornam isso verificável: nada entra em `seele-server` por causa disto, e nada roda
sem ser pedido. Um `[[bin]]` novo custaria um link inteiro mais três linhas em
`empacotar/` mais um `externalBin` no Tauri; um exemplo não serve porque quem tem
a rede quebrada não tem `cargo`. O `sondar` continua onde está — ele é do operador
do ponto de encontro.

### O tipo de NAT, com precisão sobre o que não dá

**Distinguir cone de simétrico com o protocolo de hoje é impossível, e não por
descuido.** A classificação exige comparar o mapeamento do mesmo socket local
visto de **dois destinos diferentes**. `ONDE` responde pelo socket que recebeu e
`LEVE` reflete a partir do mesmo socket: a origem de todo `AQUI` é
`IP-do-ponto:8384`, invariavelmente. O IPv6 do ponto não conta — comparar um
mapeamento IPv4 com um IPv6 compara dois caminhos, e no IPv6 em geral não há NAT
a classificar.

O que uma máquina só afirma com certeza ainda vale: **se o endereço observado é um
dos endereços desta máquina, não há NAT no caminho.** Cone ou simétrico fica em
`DESCONHECIDO`, e a palavra é honesta.

O diagnóstico aceita N pontos de encontro e classifica quando tiver ≥ 2 (decisão
9). Isso não custa código nem protocolo: o ADR 0022 já fez o ponto de encontro
trocável, e `docs/ponto-de-encontro.md` são dez linhas de comando. Custa metadado
a um segundo terceiro, numa execução pedida à mão — escolha de quem roda, não do
produto. **Verbo novo seria pior**: poria na mão do ponto de encontro a escolha de
para onde a segunda sondagem vai.

### O que o protocolo já dá e ninguém usa

`LEVE <meu próprio endereço global:porta alta>` faz o ponto de encontro mandar um
datagrama **não solicitado** a um socket que nunca falou com ele. Se chega,
entrada de fora funciona de verdade.

É o único teste do projeto que transforma o "chance, e não certeza" de
`Degrau::alcanca_de_fora` em fato medido: vale para o degrau 2 (firewall do
roteador) e para o degrau 3, onde pega **de fora** o sucesso mentiroso do CGNAT
que hoje só é pego por heurística sobre o endereço WAN. Limite honesto: prova que
96 bytes daquela origem chegaram àquela porta, não que o aperto de mão QUIC sobe.

### A saída

```
REDE — o que esta máquina alcança
─────────────────────────────────
daqui              192.168.1.20 (Wi-Fi) · 2804:388:…:7234 (Wi-Fi)
                   100.96.0.3 (túnel — não vale como endereço seu)
UDP sai            sim
visto de fora      191.38.227.90:64024 — não é endereço seu: há NAT no caminho
                   [2804:388:…:7234]:64106
entrada de fora    IPv6 chega · IPv4 não chega
tipo de NAT        desconhecido — só um ponto de encontro respondeu
ponto de encontro  encontro.seele.app.br — IPv4 e IPv6 responderam
QUIC               sobe nesta máquina
furo               não testado: precisa de outra máquina, em outra rede
```

O que cada linha muda: *daqui* separa o endereço da casa do de VPN, que é o
defeito de campo do ADR 0022; *UDP sai* separa "rede corporativa bloqueia" de tudo
o mais, e é a primeira bifurcação; *visto de fora* diz se encaminhar a porta à mão
vai adiantar; *entrada de fora* diz se o firewall do roteador é o culpado — a
única linha que hoje ninguém consegue responder; *tipo de NAT* diz quando parar de
tentar; *ponto de encontro* separa "o serviço caiu" de "a minha rede não deixa";
*furo* não promete nada que não mediu.

**Modo par**, reusando o que existe: `plug --rede --esperar` imprime um bilhete
`enc=<ponto>/<onde avisar>`, e `plug --rede <bilhete>` do outro lado é o degrau 4
inteiro sem Dogma atrás. **Fora desse modo a linha do furo diz `não testado`,
nunca `FALHOU`.**

## 5 · Métricas

**Do quinn, de graça** (`Connection::stats().path`): `rtt` suavizado,
`sent_packets`/`lost_packets`, `congestion_events`, `black_holes_detected`,
`current_mtu`, `cwnd`. `session.rs:1578` já lê isso.

**Jitter** — ver §3.3.

**"DIRECT" não é dizível**, e a casca não vai dizer. A escada tem cinco degraus, e
a distinção que "DIRECT" apagaria é justamente a que importa: em `FuroDeNat` a
conversa **é** direta, e alguém soube que ela existe. Quatro nomes estáveis, no
padrão de `Degrau::nome()` — `RedeLocal`, `Ipv6Direto`, `EnderecoPublico`,
`FuroDeNat` — atravessando o `seele-ffi` como `Snapshot.caminho:
Option<&'static str>` e ganhando frase em `frases.js`. **Sem informação do
gerente, a casca não escreve nada**: inventar "DIRECT" é a mentira confiante que o
ADR 0022 existe para não produzir.

**Onde aparece.** Os números ficam no rodapé `.telemetria`, onde já estão: são
números, e a regra de "só existe se muda o que a pessoa faz" é sobre frases. O
caminho é uma linha ao lado deles, escrita uma vez e calada depois. Só a
degradação vira frase — e a frase diz o que fazer, porque perda da rede e falha
desta máquina soam idênticas e têm conserto oposto.

## 6 · `seele-udp`, o demultiplexador

**O problema estrutural**, descrito pelo próprio ADR 0022: o quinn é dono do
socket do Dogma e não o empresta. Daí o **espelho do socket** (só para escrever) e
a **escuta de avisos** separada. A consequência é que o Dogma **não consegue ler**
nada no socket dele que não seja QUIC: ele fura às cegas.

**A assinatura, conferida em `quinn-0.11.11/src/runtime.rs`.** `AsyncUdpSocket`
obriga `create_io_poller`, `try_send`, `poll_recv` e `local_addr`. Três métodos
têm padrão e **os três padrões são armadilhas**: `max_transmit_segments() = 1`,
`max_receive_segments() = 1`, `may_fragment() = true`. Aceitá-los derruba o GSO de
64 segmentos para 1 e o GRO de 64 para 1 no Linux — 64× mais `sendmsg`/`recvmmsg`
num caminho que carrega áudio a 50 quadros por segundo por interlocutor — e
`may_fragment() = true` desliga o MTUD (`allow_mtud = !socket.may_fragment()`). O
custo da implementação ingênua é quase todo no Linux, que é onde o Dogma roda.

O desenho é **envelope, não reescrita**: guardar `tokio::net::UdpSocket` +
`quinn_udp::UdpSocketState` e delegar exatamente ao que `runtime/tokio.rs` faz,
inclusive `max_transmit_segments() = inner.max_gso_segments()` e
`max_receive_segments() = inner.gro_segments()`.

**A regra de reconhecimento** é a conjunção de três: `len == 96`, prefixo
`b"SEELE-ENC/1 "`, e um verbo que `analisar`/`ler_aqui` aceitem. Nada de "começa
com S". Um QUIC de cabeçalho longo sempre começa com byte ≥ `0x80`; `'S'` é
`0x53`, válido só em pacote 1-RTT — e ali os bytes 1..9 são o Connection ID que
**nós mesmos sorteamos**, então engolir QUIC exigiria sortear `EELE-ENC`, mais
`/1 ` em texto cifrado, mais 96 bytes exatos.

**A assimetria fica escrita:** falso positivo derruba a conexão sem erro nenhum —
o quinn nunca vê o pacote e a conexão morre por tempo ocioso; falso negativo só
desperdiça um datagrama, que é o que já acontece hoje. **Na dúvida, entrega ao
quinn.**

**A armadilha real é o GRO.** Um `RecvMeta` pode descrever até 64 datagramas num
buffer só, delimitados por `stride`. O demux **não pode** olhar só o começo: tem de
percorrer `len` em passos de `stride` e decidir por segmento, compactando o buffer
no lugar quando o lote vier misturado. Na prática isso quase nunca acontece — o
kernel só coalesce mesmo 4-tupla e mesmo tamanho, e um `FURO` de 96 bytes ao lado
de um `Initial` de 1200 já quebra o lote. **"Quase nunca" é exatamente o defeito
que aparece na máquina de outra pessoa**, então há teste com lote sintético.
Desligar o GRO não é saída: `UdpSocketState::new` o liga sozinho, e reportar
`max_receive_segments() = 1` sem desligá-lo faria o quinn dimensionar buffers
pequenos para pacotes coalescidos grandes — truncamento, que é pior.

**O que morre.** O espelho do socket (`Server::espelho`, `lib.rs:275`) inteiro,
junto com o `try_clone` e a variante `FalhaNoEncontro::SemSocketDoDogma` no ramo de
falha de clonagem. E a escuta de avisos — que é o ganho maior: hoje `abrir` faz
**duas** perguntas ao ponto de encontro (`ONDE` pela escuta, `LEVE` pelo socket do
Dogma) porque o socket do Dogma é cego. Passa a bastar **um `ONDE` pelo socket do
Dogma**, cuja resposta é ao mesmo tempo o candidato do convite e o endereço de
aviso. As duas metades do `Bilhete` viram o mesmo endereço; o campo `aviso`
continua no `seele://` por compatibilidade, mas deixa de ser um segundo mapeamento
de NAT que pode morrer sozinho e envelhecer o convite. O `REAVIVAR` de 15 s
continua: ele é do 4-tupla até o ponto de encontro, que o keepalive do QUIC não
toca.

**Onde mora** (decisão 5): crate novo `seele-udp`, dependendo de `quinn` e
`seele-proto`, com `enum Peneirado { Encontro(..., SocketAddr), Quic }`. Os dois
lados o importam. `seele-server` não pode importar `seele-core`, e `seele-proto`
não pode ganhar `quinn` — o crate novo é a única saída que não fura o ADR 0002.

**Cobrança**, porque o risco é grave e silencioso: um teste que manda 96 bytes de
QUIC forjado e afirma que **passa adiante**; um teste de lote GRO misturado; e uma
contagem de peneirados por classe exposta como métrica, para que "engoliu" tenha
número em vez de sintoma.

## 7 · Reconexão em mudança de rede

**O que o quinn dá de graça, conferido na fonte.** `ServerConfig.migration` é
`true` por padrão, e `Connection::migrate` roda no servidor com
`PATH_CHALLENGE`/`PATH_RESPONSE`. Isso cobre **mapeamento de NAT alterado e IP
público alterado sem trocar de interface**. O que **não** existe: migração
iniciada pelo cliente é só `Connection::local_address_changed()`, e quem chama é
`Endpoint::rebind` — nós. E **não há monitor de rede em `quinn`, `quinn-proto` ou
`quinn-udp`**. Wi-Fi→4G, Ethernet→Wi-Fi e VPN ligada/desligada são inteiramente
nossos.

**O que já existe e não se duplica.** `Enlace::tentar` (`enlace.rs:1186`) já é
reconexão de verdade: rebate no ponto de encontro — o comentário lá explica que
porta nova exige furo novo — e restaura Cage, Linha, A.T. Field e isolamento.
`battery.rs` já dá os cinco minutos e o backoff. `voz_na_reconexao.rs` cobre outra
coisa. A pendência #9 é sobre **trocar de destino**, não sobre mudar de rede.

**Falta pouco, e é delimitado:**

1. O `Enlace` guarda **um** `Destino`, não a lista de candidatos. Numa mudança de
   rede o candidato certo muda: o endereço da rede local deixa de valer, o público
   passa a valer. Com a decisão 1 isso sai de graça — o `Motor` constrói uma
   `Chegada` nova com a lista inteira.
2. Nada dispara a reconexão a não ser ping perdido: três keepalives de 5 s mais o
   `IDLE_TIMEOUT` de 20 s antes de alguém perceber que o Wi-Fi caiu, com o 4G já de
   pé.

**Detecção: `if-addrs`, um crate**, já na árvore do `seele-server`, com `libc`
como única dependência — a mesma conta que o ADR 0022 fez ao aceitar `if-addrs` e
recusar `portmapper` por 31. Uma tarefa lê o conjunto de endereços não-loopback a
cada 2 s e um resumo diferente é o sinal. Um caminho só nos três sistemas. Custa
até 2 s de latência e um `getifaddrs` por tique; a alternativa nativa
(`NWPathMonitor`, netlink, `NotifyIpInterfaceChange`) é instantânea e paga em três
caminhos separados mais uma árvore não medida. **Segundo sinal, de graça:**
`try_send` devolvendo `ENETUNREACH`/`EHOSTUNREACH` é mudança de rede sem esperar
tique nenhum — e o demux é onde esse erro passa.

**O que refazer depende do degrau:**

- **Endereço público mudou, interface a mesma:** nada. O QUIC migra sozinho. Só
  não estragar.
- **Interface trocou, quem entra:** socket novo obrigatório, então porta nova,
  então **furo novo**. `rebind` não serve: o furo é por porta e o Dogma abriu
  caminho para a porta antiga.
- **Interface trocou, anfitrião — o caso feio.** O degrau 3 tem um mapeamento UPnP
  renovado a cada `RENOVACAO = 1200 s` (`alcance/porta.rs:93`) apontando para o IP
  **local** que acabou de mudar. A renovação seguinte reafirma o mapeamento errado
  e ele continua "funcionando": sucesso mentiroso, de novo. É preciso devolver o
  mapeamento velho, pedir um novo com o IP interno atual, subir a escada de novo,
  refazer o `ONDE`, **e gerar um `seele://` novo**.

**O link que já foi mandado por WhatsApp morre.** Quem já está dentro é salvo pela
migração do QUIC; quem ainda não entrou não é salvo por nada. Decisão 6: regenerar
e **avisar na tela**, junto do link, que é onde o ADR 0022 já decidiu que essas
frases moram.

**O que a pessoa vê:** nada novo. A bateria interna já é a tela certa — contagem
regressiva, tentativas listadas, histórico legível. A mudança de rede só faz a
bateria começar em ~1 s em vez de ~20 s, e a lista ganha uma linha honesta ("a
rede mudou; procurando de novo").

## 8 · Verificação

### 8.1 · A matriz A–J na escada

| Caso | Degrau esperado | O que o anfitrião lê | Prova |
|---|---|---|---|
| A · ambos com IP público | 1 (direto) | **hoje: `SoRedeLocal` — defeito 3.2** | automático |
| B · um atrás de NAT comum | 3, ou 4 se o UPnP não abrir | `PortaNoRoteador` / `FuroDeNat` | duas máquinas |
| C · ambos atrás de NAT comum | 4 | `FuroDeNat` | duas máquinas |
| D · mesma LAN | 1 — primeiro candidato do convite | qualquer | automático |
| E · CGNAT | 4; UPnP dá sucesso mentiroso e é recusado | `FuroDeNat` | duas máquinas |
| F · NAT simétrico dos dois lados | nenhum — **fora de escopo por decisão** | `FuroDeNat`, "*deve* funcionar" | duas máquinas |
| G · dois NATs restritivos | 4, se o furo casar no tempo | `FuroDeNat` | duas máquinas |
| H · firewall bloqueando UDP | nenhum; falhar rápido e nomeado | hoje não distingue de NAT | automático |
| I · rede corporativa | 1 ou nenhum | `SoRedeLocal` | duas máquinas |
| J · IPv6 nos dois lados | 2 | `Ipv6Direto` | automático + duas máquinas |

Onde a expectativa é "pode falhar", o produto **não tem fallback e não vai ter**:
a saída escrita é encaminhar a porta à mão ou uma VPN de rede, e a frase diz "deve
funcionar", nunca "funciona". Um caso que não abre é uma linha de relato, não um
bug aberto.

### 8.2 · O que CI prova, e o que só duas máquinas provam

O furo em si — dois roteadores abrindo caminho ao mesmo tempo — **não é testável
em CI**, e nenhum truque muda isso. Testável é tudo o que decide se ele acontece:
ordem, truncamento e degrau declarado (`decidir` com `Achado`s de mentira);
amplificação (`responder` é função livre sobre bytes); o ponto de encontro ponta a
ponta numa máquina; **o tempo entre o `LEVE` e o aperto de mão** (relógio simulado
do tokio, ponto de encontro falso em processo); as transições da máquina de
estados; e queda e volta de rede.

Namespaces de rede com `iptables` dariam um NAT de mentira em CI e **não valem a
pena neste ciclo**: exigem root, são só-Linux, e reproduzem o NAT que nós
escolhermos — que é exatamente o que já sabemos.

### 8.3 · Os testes deste ciclo

| Teste | Reprova quando |
|---|---|
| `o_aviso_sai_imediatamente_antes_do_candidato_que_precisa_dele` | volta o `bater()` único antes do laço, e o intervalo de 12 s |
| `o_furo_ainda_esta_aberto_quando_o_aperto_de_mao_chega` | a distância entre aviso e handshake passa da janela do furo |
| `um_candidato_da_rede_de_casa_nao_gasta_aviso_nenhum` | metadado e orçamento de furo pagos por quem não precisa |
| `um_candidato_que_nao_volta_nao_gasta_o_furo_do_proximo` | o LAN morto volta a comer a janela do endereço público |
| `todos_os_candidatos_saem_pela_mesma_porta` | uma `Batida` que reabre socket por candidato e mata o mapeamento |
| `o_furo_manda_um_pacote_por_aviso_e_nunca_mais_que_isso` | amplificação acima de 1:1 |
| `um_aviso_forjado_nao_faz_o_dogma_mandar_mais_bytes_do_que_recebeu` | qualquer atalho que barateie o abuso |
| `um_aqui_de_origem_estranha_nao_vira_furo` | a conferência de origem da decisão 8 sumir |
| `a_janela_de_furos_continua_fechando_depois_de_vinte` | o segundo cinto sumir agora que há repetição |
| `o_endereco_furado_nunca_e_truncado_para_fora_do_convite` | o defeito 3.1, encenado em `decidir` |
| `a_escada_so_diz_furo_de_nat_se_o_endereco_furado_estiver_no_convite` | a divergência que o próprio comentário nega |
| `um_endereco_publico_nao_e_um_link_que_so_funciona_na_sua_rede` | o defeito 3.2 |
| `so_existe_um_candidato_publico_quando_ha_enc_no_convite` | a exclusividade que sustenta a dedução do tipo |
| `um_cliente_antigo_le_o_convite_novo_e_conecta_igual` | regressão de compatibilidade do `seele://` |
| `noventa_e_seis_bytes_de_quic_forjado_chegam_ao_quinn` | o demux engolindo QUIC |
| `um_lote_gro_misturado_e_peneirado_segmento_a_segmento` | o demux olhando só o começo do buffer |
| `o_jitter_da_tela_nao_e_a_profundidade_do_anel` | o defeito 3.3 voltar por outro caminho |
| `toda_transicao_de_estado_tem_uma_frase_e_nenhuma_e_um_beco` | um estado de erro novo sem entrada em `frases.js` |
| `uma_rede_nova_refaz_o_furo_antes_de_tentar_o_aperto_de_mao` | a reconexão sair de porta nova sem rebater |
| `a_reconexao_recebe_a_lista_inteira_de_candidatos` | o `Motor` voltar a tentar um endereço só |
| `um_aqui_perdido_nao_custa_a_conexao` | remoção da retentativa |

### 8.4 · O roteiro de campo

A seção 7 de `docs/teste-duas-maquinas.md` tem seis passos e um final honesto, e
**não captura o que precisava**: não pede o relógio, não pede qual candidato
venceu, não distingue as falhas, e não pede o tipo de NAT antes de falhar — só
depois, como consolo. Por isso o `--barulhento` mostrou "duas apresentações",
exatamente como o passo 4 manda, com o furo já fechado. **O roteiro atual exibe
sucesso e falha com a mesma cara.**

Roteiro revisado:

1. `plug --rede` **nas duas máquinas, antes de qualquer coisa.** Duas saídas
   coladas já classificam o caso em A–J sem ninguém adivinhar.
2. Hospedar com o UPnP desligado. Registrar a frase, o link inteiro, e a lista de
   candidatos com a ordem.
3. Colar o link na máquina B. Registrar **o log de transições da `Chegada` com
   carimbo de tempo** — é o entregável central deste ciclo, e o que substitui "não
   conectou".
4. Na VPS, `--barulhento` com carimbo: instante de cada `ONDE`, `LEVE` e `AQUI`.
5. Conectado: `:sync` nos dois, **e por qual candidato e degrau a sessão saiu**.
6. Passos 5 e 6 do roteiro atual (ponto de encontro fora do ar; queda de rede)
   ficam: são os testes de que o degrau 4 não virou ponto único de falha.

Um relato utilizável é: duas saídas de `plug --rede` + o log carimbado + a
operadora e o modelo de roteador de cada lado. Com isso, "não conectou" vira "o
furo abriu às 12:03:01.4 e o handshake saiu às 12:03:05.6" — que é uma linha de
código, não um mistério.

### 8.5 · Aceitação

Dos 15 critérios da §21 do documento de fora: **seis passam hoje** (áudio nunca no
rendezvous; rendezvous não é relay; endpoint descoberto automaticamente; servidor
só no control plane; sem port forwarding manual; sem firewall manual na maioria
dos casos). **Seis são deste ciclo** (hole punching automático; QUIC sobre o
caminho P2P; dois pares atrás de NAT residencial conectam; falhas de NAT
detectadas e reportadas; reconexão após mudança de rede; logs suficientes para
diagnosticar). **Dois ficam parciais por decisão**: os pares não trocam candidatos
pelo ponto de encontro — é a defesa, não a falta. **Um fica aberto por decisão**:
NAT simétrico dos dois lados.

Um critério aberto por decisão não é fracasso. É uma linha do documento, com saída
nomeada.

## 9 · Ordem de construção

A ordem não é de dificuldade: é de **quanto cada passo desbloqueia medição**.

1. **Os defeitos 3.1 e 3.2** — a escada não pode mentir sobre o degrau que vamos
   medir.
2. **`Tipo` de candidato, e o conserto do truncamento por `insubstituivel()`.**
3. **A `Chegada` delegando**, com a trilha atravessando o `seele-ffi`.
4. **A coordenação: `Batida::preparar`/`avisar`, aviso por candidato,
   `PACOTES_DO_FURO = 1`, origem do `AQUI` conferida, prazo curto do candidato
   distante.** Aqui o ciclo já pode ir a campo.
5. **`plug --rede`** — e refazer o teste de campo com ele.
6. **Métricas**, incluindo o defeito 3.3.
7. **`seele-udp`** — o demux, com os três testes que o cobram.
8. **Reconexão por sinal de rede**, que o demux barateia.

O passo 4 é o que fecha o defeito que originou este spec. Os passos 7 e 8 são os
que o documento de fora pede e que valem por si, mas **não** são pré-requisito
para conectar.

## 10 · O que fica de fora, por decisão

- **Retransmissão (degrau 5).** ADR 0022. NAT simétrico dos dois lados continua
  sem saída, e as saídas nomeadas são encaminhar a porta à mão ou uma VPN de rede.
- **ICE bidirecional.** Quem entra não lê resposta do ponto de encontro, e essa
  ausência é a defesa.
- **`iroh` e a comparação da §8/§9.** Respondida por decisão em 2026-08-20:
  mantém-se o desenho atual.
- **Campo `tipos=` no `seele://`.** O tipo é deduzido, e a dedução é travada por
  teste.
- **Namespaces de rede em CI.**
- **Segundo endereço no ponto de encontro do projeto.** Quem quiser classificar
  NAT aponta para dois.
- **Compartilhamento de tela.** Ciclo próprio, depois deste.

## Perguntas que continuam abertas

1. **Três avisos por candidato refletido é o número certo?** Escolhido por caber
   na janela de 4 s com folga, não medido. Só um segundo teste de campo responde.
2. **`PRAZO_DE_CANDIDATO_DISTANTE = 1 s` sobrevive a Wi-Fi ruim?** O /24 é chute
   quando a rede é /16, e aí um vizinho legítimo cai no prazo curto.
3. **O `Refletido` merece prazo maior que 4 s**, já que é ele quem carrega as
   retentativas?
4. **O teste de entrada não solicitada por `LEVE` deve migrar do diagnóstico para
   a escada do `seeled`**, trocando "chance" por "certeza" nos degraus 2 e 3 — ao
   custo de uma ida ao ponto de encontro no arranque?
5. **O `seele-app` ganha botão para o diagnóstico**, ou ele fica só no terminal?
6. **Fundir as duas metades do `Bilhete`** quando a escuta de avisos morrer é
   mudança de `seele://`. Vale a versão, ou o campo `aviso` fica repetido para
   sempre?
7. **Dois segundos de tique no `if-addrs`** são aceitáveis, ou vale medir a árvore
   dos caminhos nativos?
