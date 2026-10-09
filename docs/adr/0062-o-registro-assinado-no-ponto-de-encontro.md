# 0062 — SEELE-ENC/2: o registro assinado no ponto de encontro

Status: **aceito**. A direção é a da spec de 23/09/2026 («SEELE-ENC/2: o
registro assinado»). Os valores do limite e do teto (D-enc-1), o portão no
`publicar.sh` (D-enc-2), o teto por prefixo (D-enc-3) e o cookie de ida e volta
(D-enc-4) foram decididos por quem responde pelo projeto em 2026-10-08, como
estão abaixo; os dois últimos depois de uma revisão independente.
Data: 2026-10-08
Sobre `docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md`
(«Achar a pessoa») e o spec-mestre da 0.16
(`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 «F2», §3 item 13, §4).
**Revê o §2.4 do [ADR 0047](0047-o-link-que-volta-a-funcionar-amanha.md)**, que
recusou o registro assinado.

## Contexto

O ADR 0047, no §2.4, recusou o registro assinado no quarto. O argumento era que
ele compraria pouco: quem chega confere a impressão digital dentro do TLS, então
o teto do estrago de quem ocupa uma marca é «não entrar», e quem escreveu
primeiro fica enquanto reaviva. O ADR terminava dizendo que a camada seria a
resposta certa para um problema que ainda não existia.

O código de uma pessoa ([ADR 0060](0060-o-codigo-de-uma-pessoa.md)) cria o
problema. **O link carrega um endereço; o código não carrega nada**, e a sala
pessoal só é achável pelo quarto. Com o registro de hoje:

- quem ocupa a marca de uma sala fora do ar faz quem liga ouvir «não está» de uma
  pessoa que só fechou o app há um minuto, e, com a marca da escuta, recebe pelo
  `LEVE` o IP, a porta e o instante de quem tenta ligar;
- quem escreveu primeiro fica por 60 s, então uma troca de rede de quem recebe
  leva até um minuto para valer;
- não há onde pôr as dicas de rede local, que são o que deixa duas pessoas na
  mesma casa se ligarem sem *hairpin* no roteador.

A chave Ed25519 da pessoa já existe, e o código sai dela: o ponto pode conferir
que quem registra a marca de um código é o dono do código.

## Decisão

### Duas versões ao mesmo tempo

O ponto fala o `SEELE-ENC/1` e o `SEELE-ENC/2`, separados pela magia e pelo
tamanho. Quem está na 0.15 não muda nada: o `/1` responde byte a byte como antes
(`um_ponto_de_encontro_reiniciado_responde_igual`). **A sala pessoal registra e é
procurada pelo `/2`; um servidor guardado continua no `/1`**, porque o
certificado dele é P-256, e não a chave Ed25519 da pessoa.

A sala descobre os endereços públicos pelo `/1` (`ONDE` e `LEVE`, que não guardam
nada) e se **registra** pelo `/2`.

### O fio

Todo datagrama do `/2` tem **384 bytes**, pedido e resposta, completados com
`.`: a resposta nunca é maior que o pedido, e o ponto nunca amplifica. O maior
`MORO2` possível tem 322 bytes, com o cookie. A spec falava em «até 512 bytes»; o tamanho fixo
é a regra mais simples de provar. **A folga não compra um campo novo sem versão
nova**: cada linha tem um número exato de campos, e cada motivo é um de quatro,
então um campo ou um motivo a mais faz o leitor de hoje ver lixo e calar.

| quem | linha |
|---|---|
| anfitrião → ponto | `SEELE-ENC/2 MORO2 <marca> <chave> <carimbo> <dicas> <assinatura> <cookie>` (`-` sem cookie) |
| quem liga → ponto | `SEELE-ENC/2 QUEM2 <marca>` |
| ponto → anfitrião | `SEELE-ENC/2 DESAFIO <marca> <cookie>` |
| ponto → anfitrião | `SEELE-ENC/2 MOROU <marca> <endereço visto> <cookie>` |
| ponto → quem liga | `SEELE-ENC/2 AQUI2 <marca> <endereço> <dicas>` |
| ponto → quem liga | `SEELE-ENC/2 NINGUEM <marca>` |
| ponto → quem pediu | `SEELE-ENC/2 RECUSA <marca> <motivo> <agora>` |

- **Cada campo tem uma escrita só.** A chave (32 bytes) e a assinatura (64) em
  base64url, sem `=` e sem bits sobrando; o carimbo e o `agora` em segundos Unix,
  sem sinal e sem zero à esquerda; um endereço só se o texto for o que o
  `Display` escreve. Medido: o `SocketAddr` aceita `[fe80::1%3]:80` e `:08385`, e
  o `u64` aceita `+1` e `007`.
- **As dicas são de rede local**: IPv4 privado ou IPv6 ULA, sem escopo, porta
  ≥ 1024, no máximo duas, `-` para nenhuma. O ponto recusa um registro com dica
  fora disso, e quem liga recusa uma resposta com dica fora disso: um ponto
  hostil não manda quem liga bater dentro da própria rede por uma dica
  inventada.
- **O que a chave assina** (`seele_proto::assinatura::moro2`):
  `ROTULO_MORO2 ‖ u8 len(marca) ‖ marca ‖ chave ‖ carimbo u64 BE ‖ u8 n ‖ n ×
  (família ‖ IP ‖ porta BE)`, com o rótulo `"SEELE-ENC/2 moro\0"`, distinto do S2
  e da sala. A menor mensagem tem 60 bytes, então nenhuma cabe nos 32 de um
  nonce. O endereço público não vai assinado: quem o diz é o ponto, que o vê.

### O cookie de ida e volta (D-enc-4)

Um `MORO2` sem cookie, ou com um que não confere para o IP de onde ele veio,
ouve um `DESAFIO` com o cookie. O ponto não guarda nada para isso, e o pedido
**não gasta o balde**. Só um `MORO2` com o cookie válido segue.

- O cookie são os 16 primeiros bytes do HMAC-SHA256 do segredo do ponto sobre
  `"SEELE-ENC/2 cookie\0" ‖ família ‖ IP ‖ janela`, com a janela de 120 s; vale
  a corrente e a anterior (de dois a quatro minutos). O HMAC é o `hmac` do
  RustCrypto, conferido com os vetores da RFC 4231, e a conferência é o
  `verify_truncated_left` dele, que compara em tempo constante (o `ct_eq` do
  `ctutils`). Nenhuma cripto escrita no ponto, e um guarda de fonte prende que o
  cookie não é comparado com `==`. **Não o `ring`**, que também está na árvore:
  ele compila C no build, e o ponto é Rust puro para quem o reimplanta numa VPS
  sem compilador de C.
- **Preso ao IP, e não à porta.** A sala recebe o cookie pela escuta de avisos,
  e o socket do servidor, que sai pelo mesmo IP público, usa o mesmo. O `MOROU`
  leva o cookie da janela corrente, para a sala renová-lo antes de ele vencer:
  ela só ouve `DESAFIO` na subida, depois de o ponto reiniciar, e depois de
  trocar de IP.
- **Não vai assinado**: ele prova o caminho de volta, e não a chave. A sala o
  junta ao `MORO2` sem assinar de novo.
- **O segredo** são 32 bytes sorteados do sistema na subida do ponto
  (`getrandom`), só em memória: nunca em disco, nunca no `--barulhento`, e
  ninguém configura nada. Reiniciar o ponto sorteia outro, os cookies de antes
  param de valer, e cada sala ouve um `DESAFIO` e volta no reavivamento
  seguinte.

**O que ele fecha**: quem forja a origem não recebe o cookie do IP que forja.
Ele não registra nada em nome desse IP, e não esvazia o balde de registro dele
(o achado da revisão: com o IP que o `QUEM2` entrega a quem tem o código de
alguém, uns poucos pacotes por segundo calavam a sala dessa pessoa).

### O que o ponto confere, nesta ordem

A ordem é a do custo: o cookie, um HMAC, vem antes do limite, para que pedido
forjado não gaste o balde de ninguém; o limite por prefixo vem antes da
assinatura, que é o que custa; e a assinatura vem antes do relógio, para que um
carimbo torto só seja dito a quem assinou com uma chave que confere. A hora do
ponto em si não é segredo: a `RECUSA taxa` a leva a qualquer um.

1. o cookie (acima);
2. o limite por prefixo (abaixo);
3. que a marca é a escuta ou o servidor de `Marcas::da_sala(Codigo::da_chave(chave))`
   (o aviso não mora no quarto, e não se registra);
4. a assinatura, com `verify_strict`. Com o `verify`, a chave identidade e a
   assinatura `R = identidade, s = 0` conferem com qualquer mensagem, e qualquer
   um registraria a sala do código dela; o teste prende isso;
5. `|carimbo − agora| ≤ 300 s`.

Uma assinatura que não confere é calada. O resto ouve a resposta.

### O quarto assinado

**Separado do quarto da versão 1.** O `QUEM2` só lê o assinado. Assim «o registro
assinado vence o não assinado» vale por construção: um `MORO` sem assinatura não
chega onde o `QUEM2` lê. E o `MORO` do `/1`, que é de graça, não enche o lugar das
salas.

- Para a mesma chave, **o carimbo mais novo vence na hora**, de qualquer
  endereço: uma troca de rede vale no reavivamento seguinte, sem os 60 s de trava
  do `/1`.
- **Carimbo igual ou mais velho só vale do mesmo socket**: é retransmissão, ou o
  relógio de quem registra voltou. O carimbo guardado continua o maior. De outro
  endereço, `RECUSA velho`: é um `MORO2` copiado no caminho e repetido de outro
  lugar.
- O registro responde ao `QUEM2` por 60 s depois do último aceito, como no `/1`,
  e **é lembrado enquanto o carimbo dele passaria na tolerância**: a repetição
  depois do prazo continua recusada.
- **O teto conta os registros vivos.** Um registro só lembrado não segura vaga:
  com o quarto no teto, ele sai antes de um registro novo ouvir `cheio`. Contado
  no teto, um `MORO2` com o carimbo 300 s no futuro seguraria a vaga por ~600 s,
  e quatro IPv4 encheriam o quarto para sempre (medido na revisão de
  08/10/2026). O preço é perder, sob pressão, a lembrança que recusa a
  repetição.
- Outra chave com a mesma marca (uma colisão de 75 bits) não toma o lugar
  enquanto ele vale.
- **No máximo 128 marcas vivas por prefixo** (D-enc-3): sessenta SEELE atrás de
  um CGNAT, duas marcas cada, com folga. A marca que passaria a viver num
  prefixo no teto, inclusive a que muda para ele, ouve `RECUSA cheio`; quem já
  mora ali continua reavivando. Contar os vivos do prefixo varre o quarto, e
  só se conta quando a marca não está viva ali.
- **O `QUEM2` responde sempre, dentro do limite**: `AQUI2` quando acha, `NINGUEM`
  quando não acha. O `/1` cala para a marca vazia. Calar aqui repetiria o
  próprio ADR 0047: com o ponto de produção sem o `/2`, quem liga leria «ninguém
  mora» quando a verdade é «o ponto não fala a versão 2». Com o `NINGUEM`
  explícito e o `ONDE` correndo junto, quem liga separa os casos: achou; o ponto
  fala e ninguém mora; o ponto recusou por excesso; o ponto responde ao `ONDE` e
  cala o `QUEM2` (`Enc2Calado`: é anterior ao `/2`, ou a recusa, dita uma vez
  por segundo, foi para outro pedido); o ponto está mudo. Enquanto só o `ONDE`
  respondeu, quem liga pergunta de novo dentro do prazo, e a volta depois da
  janela de um segundo ouve a recusa.

### Os valores (D-enc-1)

| | valor | o que ele aguenta |
|---|---|---|
| limite por prefixo (`/32` no IPv4, `/64` no IPv6), **só no `/2`**, um balde para o registro e outro para a pergunta | balde de 64 pedidos, que enche a 8 por segundo | 60 SEELE abertos atrás de um mesmo IP, para sempre |
| teto do quarto assinado | 16 384 marcas, separado das 4096 do `/1` | ~8 000 apps abertos; ~4,6 MB cheio |
| teto da tabela de prefixos | 65 536; um balde cheio está ocioso e sai na varredura | ~4,6 MB cheia |

**A conta do CGNAT.** Um app 0.16 aberto registra a sala sempre: dois `MORO2` a
cada 15 s, 0,133 por segundo. O que importa num CGNAT não é quantos assinantes
dividem um IPv4, e sim **quantos SEELE abertos** o dividem: o limite só corta
acima de 60. O fator do CGNAT de quem responde pelo projeto não foi medido, e
com esta folga não precisa ser. A rajada de 64 cobre dez ligações tentadas ao
mesmo tempo (seis `QUEM2` cada).

**Quem passa do limite ouve, e não é calado.** O ponto responde
`RECUSA <marca> taxa <agora>`, no máximo uma vez por segundo por prefixo; o
resto da enxurrada é calado, para a recusa não virar um refletor de quem forja a
origem. O anfitrião passa a `Recusado(Taxa)` e escreve no `seele.log` que o
limite é por endereço público, e que a rede pode dividi-lo com outras pessoas;
quem liga recebe `PontoRecusou`. Com a tabela de prefixos cheia de prefixos
ativos, o prefixo novo também ouve a recusa.

**O abuso que o limite segura**: um prefixo sozinho ocupa no máximo 128 marcas
vivas (D-enc-3), e o teto conta só as vivas, então encher as 16 384 pede 128
prefixos, e com o cookie cada um deles tem de ser de verdade. Um `/48` IPv6 tem 65 536 `/64`, e o limite por
`/64` não o segura: **um limite por `/48` é o segundo degrau**, se um dia for
preciso.

**Um balde para o registro e outro para a pergunta.** Com um balde só, nove
`QUEM2` por segundo de um vizinho de CGNAT, ou de quem forja o IP de alguém (o
IP que o próprio `QUEM2` entrega a quem tem o código dessa pessoa), calavam os
`MORO2` daquela rede, e a sala sumia do quarto em 60 s (a revisão de
08/10/2026). A pergunta é o pedido mais barato de mandar, e ela não esvazia
mais o balde do registro.

### O anfitrião diz o que aconteceu

O `MORO2` da escuta volta para a escuta, e é por ele que a sala sabe como está o
registro: `Registrado`, `RelogioTorto { diferenca_s }`, `Recusado(motivo)` ou
`SemResposta` (dois reavivamentos sem resposta: o ponto caiu, ou é anterior ao
`/2`). Cada mudança vai ao `seele.log` em `info`, uma vez, e fica num estado que
a casca lê. **Um registro calado por relógio torto é dito**, com a diferença em
segundos.

**O anfitrião não ajusta o carimbo pela hora do ponto.** Uma `RECUSA` forjada,
com uma hora falsa, faria a sala assinar carimbos do futuro, que alguém no
caminho guardaria para repetir depois. O pedido é dizer, e é o que se faz.

### A VPS vai antes do app (D-enc-2)

O ponto novo é compatível com a 0.15, e o app novo sem o ponto novo não acha
ninguém pelo código. O `sondar` fala o `/2` e mede o relógio do ponto; com
`--exige-enc2`, ele reprova quando alguma família que respondeu ao `ONDE` não fala
o `/2`, ou quando o relógio do ponto está a mais de um minuto do da máquina que
sonda. **O `empacotar/publicar.sh` roda essa conferência** antes de qualquer
passo com efeito, e para a publicação se o ponto em produção não falar o `/2`;
o corpo do release diz qual ponto foi conferido.
`--sem-ponto` pula, para a emergência que não depende do ponto, e o corpo do
release diz. A linha legada não é conferida. Nenhum teste fala com o ponto de
produção.

## Consequências

- **Privacidade.** O ponto aprende, além do que já aprendia, que uma sala está no
  ar, em que IP público, e com que dicas de rede local; quem tem o código aprende
  o mesmo, sem ligar (spec de 23/09, «Privacidade»). A alavanca de quem recebe é
  «não receber ligações». Isto vai para a página de privacidade.
- **O relógio do ponto passa a importar.** Um ponto com o relógio torto recusa o
  registro de toda sala. O `docs/ponto-de-encontro.md` pede o NTP, e o `sondar`
  mede.
- **O que sobra.**
  - Quem está no caminho entre a sala e o ponto, e entrega a cópia de um
    `MORO2` antes do original, aponta a marca para si até o reavivamento
    seguinte; com a da escuta, recebe pelo `LEVE` o IP e a porta de quem liga
    nesse tempo. A cópia repetida **depois** do original é recusada de outro
    endereço; do mesmo socket, ela só renova o registro e volta as dicas às do
    `MORO2` copiado.
  - A mensagem assinada não diz para qual ponto ela é, então um `MORO2` vale em
    qualquer ponto.
  - Quem forja a origem não registra nem esvazia o balde de registro de
    ninguém (o cookie). Ele ainda esvazia o balde de **perguntas** de um IP com
    `QUEM2` forjados: quem está nele não procura ninguém pelo código enquanto
    isso durar, e continua achável. E ele faz o ponto mandar um `DESAFIO` a um
    IP que não pediu nada, do tamanho do pedido, como o `ONDE` de sempre.
  - Quem é o ponto continua escolhendo o endereço público que devolve.
  - Em todos os casos, o TLS, com o código, recusa o servidor errado
    (`ConfereCodigo`).
- **Um servidor guardado continua com o que o ADR 0047 descreve**, no `/1`.
- O ponto ganha uma dependência, `ed25519-dalek`, que já estava na árvore.

## Alternativas recusadas

- **«Até 512 bytes, resposta ≤ pedido»**, como a spec dizia: mais regras para
  provar a mesma propriedade.
- **O `QUEM2` devolvendo o registro assinado inteiro**, para quem liga conferir a
  chave: o ponto já é confiado com o endereço, e o ganho seria só nas dicas, que
  quem liga filtra.
- **Um quarto só para as duas versões**, com o assinado desalojando o não
  assinado: o mesmo resultado com regras de convivência, e o `/1` de graça
  enchendo o lugar das salas.
- **Calar o `QUEM2` de marca vazia**, como o `/1`: o ADR 0047 de novo.
- **O anfitrião ajustar o carimbo pela hora do ponto**: ver acima.
- **O limite também no `/1`**: a 0.15 não muda nada.
- **Assinar o endereço público visto**, em vez do cookie: a sala só sabe o
  endereço do socket do servidor depois de perguntar, e um NAT que troca a
  porta deixaria a assinatura errada até o tique seguinte. O cookie prova a
  mesma coisa (o caminho de volta) sem a sala precisar saber.
- **Um limite por chave, em vez do cookie**: a chave de uma sala é de quem já
  ligou para ela (vem no certificado), e um `MORO2` forjado com ela gastaria o
  balde antes da assinatura.
- **O `verify` no lugar do `verify_strict`**: ver acima.

## Como reverter

Desligar o `/2` no ponto: ele passa a calar para a magia nova, e quem liga lê
`Enc2Calado`. O `/1` não muda.
