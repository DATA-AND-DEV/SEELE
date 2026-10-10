# Pendência 61 — a medida, e a proposta

Pedido da orquestradora em 10/10: **isso alcança alguém de verdade?** Primeiro a
medida, depois a proposta. Nada do comportamento mudou. Medido em `48ce70a8`
(a `versao/0.16` de 10/10), num Mac com carga de 7 a 35 (as outras sessões
trabalhando).

O instrumento fica no repositório, fora da bateria (`#[ignore]`), para a
medida ser refeita depois de qualquer conserto:

```sh
cargo test -p seele-conformance --test medida_do_barramento -- --ignored --nocapture --test-threads=1
```

## 1. O que é preciso para cair

**Mais de 1 024 mensagens num lote só do escritor.** O escritor junta tudo o
que chega em 200 ms, grava numa transação e anuncia uma `MessagePosted` por
mensagem num laço sem `await` (`server.rs`, `flush`). O lote não tem teto. Um
anúncio maior que o anel passa à frente de **toda** sessão viva no mesmo
instante — inclusive de quem lê depressa e de quem nem está no canal: o
barramento é um só.

Com o cliente de verdade (`seele_core::Client`), `N` pessoas mandando 58
mensagens cada (a rajada que o balde de quadros deixa), juntas:

| pessoas × 58 | total | rodadas | sessões passadas para trás | mensagens não gravadas |
|---|---|---|---|---|
| 8, 12, 16 | 464 a 928 | 3 | 0 | 0 |
| 17 | 986 | 3 | 0 | 0 |
| **18** | **1 044** | 3 | **todas (19 de 19)**, cada uma 20 eventos atrás | 0 |
| 20 | 1 160 | 3 | todas | 0 |
| 24 | 1 392 | 3 | todas (25 de 25) | **182**, 0, 0 |
| 24, corpo de 4 KiB | 1 392 | 3 | todas | **117**, **69**, 0 |

A linha é nítida: 986 nunca, 1 044 sempre. Quem fica para trás não lê nem o que
ainda estava no anel: o `Lagged` vem antes. As mensagens não gravadas são a
cauda de quem ainda estava sendo lido quando a sessão se despediu.

**O que o balde deixa uma pessoa fazer** (`taxa.rs`): rajada de 60 quadros e 20
por segundo, por conexão. Então um lote de mais de 1 024 pede **18 conexões
despejando a rajada inteira nos mesmos 200 ms**, ou **256 conexões no teto
sustentado** (4 por lote cada). Medido: 24 pessoas no teto sustentado por 3 s
(1 440 mensagens) não passaram ninguém para trás.

A linha é a do lote, e não a da rajada: numa rodada da medida pela FFI, 18 × 58
mais 58 dela se dividiram em dois lotes e ninguém caiu (1 de 5).

**Quem chega lá:**
- **uma pessoa digitando, não**: 58 mensagens em 200 ms não sai de um teclado, e
  o app não repete envios sozinho (abaixo);
- **uma colagem grande, não**: a casca recusa acima de 4 KiB, não corta nem
  divide (`tela-sessao.js:3151-3159`; o FFI recusa com `TooLong`,
  `lib.rs:1929-1940`). Uma colagem é uma mensagem;
- **um robô de MOD, não**: nenhuma metade de servidor grava mensagem (não há
  `server.post` em `mods/`). O que um MOD põe no barramento são avisos, 4 por
  segundo por MOD (`mods/aviso.rs:36`), direto, sem passar pelo escritor;
- **uma sala cheia, não pelo lote**: o teto é 15 por sala (`voice_room_limit`),
  e o que uma sala põe no barramento sai aos poucos, não num anúncio só;
- **18 conexões de um script, sim.** Não há teto de sessões num servidor comum
  (`lib.rs:842-865`), o balde de apertos de mão deixa 30 por endereço de uma
  vez (`taxa.rs:54`), e uma chave nova é de graça num servidor aberto. **Uma
  pessoa com 18 conexões derruba todo mundo do servidor, quantas vezes
  quiser**, e cada vez leva junto a cauda do que quem estava escrevendo mandou.

**O outro caminho, o que o `Lagged` foi feito para pegar:** um par que parou de
ler. Com 24 pessoas conectadas e ninguém escrevendo, o barramento anda 24,1
eventos por segundo (só `PersonState`, um por sessão por segundo). Um par
parado é passado para trás em ~1 024 / N segundos: 42 s com 24 pessoas, ~20 s
com 50, antes com uma sala falando.

## 2. O que a pessoa vê

Medido pela FFI, com a janela inscrita como a do app (`subscribe`, como o
`Bridge` de `main.rs:1312`), uma pessoa sentada na sala 1 e no canal 1, e a
rajada de 18 ou 24 pessoas num **outro** canal. Em 16 rodadas, 15 passaram à
frente dela:

- **A tela de fim.** `Ended { reason: FellBehind }` chega à janela 125 a 223 ms
  depois da rajada. No app é «ENLACE ENCERRADO · ESTE ENLACE FICOU PARA TRÁS DO
  SERVIDOR», com RECONECTAR e SAIR (`frases.js:58-59`, `tela-fim.js:43-80`).
  Não há faixa de reconexão.
- **As camadas discordam.** Para o núcleo, `FellBehind` é queda recuperável
  (`enlace.rs:471-482`, `a_sessao_acabou_aqui`); ele repassa a despedida e
  reconecta pela bateria. O `Room` transforma **toda** `Disconnecting` em fim
  (`seele-sessao/src/state.rs:1361-1364`), o FFI emite `Ended`
  (`seele-ffi/src/lib.rs:6007-6014`), e o `Bridge` revoga a geração: a janela
  descarta o que vem depois. Medido: **78 a 129 eventos** da FFI depois do fim,
  todos jogados fora pela janela.
- **Ela continua na sala.** 4 s depois da tela de fim, o servidor a tem sentada
  na sala 1, em todas as 11 rodadas que olharam: a bateria a reconectou e o motor a sentou de
  novo, por baixo da tela de fim, até ela apertar um botão.
- **A voz reabre — lido, não medido.** Na volta, o FFI reabre a voz com os
  mesmos aparelhos (`lib.rs:4710-4733`, `porta.na_volta` → `reopen`), e nada
  entre o `Ended` e o RECONECTAR a fecha (`mostrarFim` não chama o nativo; o
  `Bridge` só revoga a geração; o laço do FFI só para em `Aviso::Encerrado`).
  Pelo código, **quem estava falando volta a transmitir com a tela dizendo
  «encerrado»**. Não medi porque medir abre o microfone de verdade desta
  máquina: o aparelho de mentira de `seele-audio` não chega pela FFI. Medir isto
  é o primeiro passo de qualquer conserto.
- **As mensagens dela.** Quando o que ela mandou já tinha sido lido, tudo foi
  gravado, mas a janela as deixa em `SemResposta` (57 ou 58 de 58). Quando ela
  mandava no instante em que o lote passou, **o servidor perdeu parte ou
  tudo**:

| caso | ela mandou | o servidor gravou | depois do RECONECTAR ela vê |
|---|---|---|---|
| 24 pessoas, junto | 58 | **52** | as 50 mais novas; as 6 perdidas não aparecem |
| 24, ela 150 ms depois | 20 | **5** | 5; **15 sumiram** |
| 24, ela 200 ms depois | 20 | **0** | **nada** |

  Antes do RECONECTAR elas estavam em `SemResposta`, debaixo da tela de fim. O
  RECONECTAR larga a conexão velha e o `Room` com elas; a caixa de escrever foi
  esvaziada no envio (`tela-sessao.js:3191`). **Nada diz que se perderam, e não
  há como repetir.**
- **Nada é repetido sozinho.** O núcleo não guarda o `Dizer` para a volta
  (`enlace.rs:5275-5326`); o que vira `SemResposta` espera o TENTAR DE NOVO. O
  texto da pendência («o cliente de verdade repete o resto com a mesma chave»)
  não é o que o código faz.
- **Repetir seria seguro.** Medido: a mesma chave duas vezes grava **uma**, e
  quem mandou recebe o eco duas vezes (o índice `messages_idempotency`,
  `schema.rs:129-131`).
- **Nada busca o buraco.** Na volta, o núcleo só reassina o último canal
  (`enlace.rs:5894-5896`); o histórico só vem quando a pessoa abre um canal, e
  só a página mais nova, de 50 (`lib.rs:6235-6252`). O que passou durante a
  queda e não está nas 50 mais novas não volta.

## 3. O desenho

**O anel e o `Lagged` são o desenho certo para quem parou de ler**, e
continuariam sendo: evento não tem endereço, o servidor não sabe dizer quais
faltaram, e despedir e ressincronizar é o caminho. **São o desenho errado como
resposta a uma rajada de tráfego legítimo**: hoje um lote grande passa à frente
de quem lê depressa, de todo mundo de uma vez.

Contrapressão em quem escreve, em vez de desligá-lo, cabe — e por um caminho
que não bloqueia o barramento: cada sessão já vê as próprias mensagens voltarem
pelo barramento, então pode contar as que tem **no ar** (postadas e ainda não
anunciadas) e parar de ler a entrada daquela conexão enquanto passar de um teto,
com uma pré-condição no ramo do `select!`. O que sobra espera no controle de
fluxo do QUIC, do lado de quem mandou, e nada se perde. O lote fica limitado a
`teto × sessões`.

## 4. A proposta, em ordem

1. **O cliente, que é defeito em qualquer decisão.** O `Room` não pode tratar
   como fim uma despedida que o núcleo trata como queda: só as razões de
   `a_sessao_acabou_aqui` encerram, e as outras mostram a faixa da bateria. Na
   volta, buscar o histórico do canal aberto e repetir sozinho o que está em
   `SemResposta`, com a mesma chave (medido: idempotente). Sem isto, até o caso
   legítimo do `Lagged` — a rede que travou — vira tela de fim, pessoa sentada
   por baixo e mensagem perdida. **Antes**: medir a voz reabrindo debaixo da tela
   de fim, com um aparelho de mentira que atravesse a FFI.
2. **O lote com teto, no servidor.** O escritor grava quando der 200 ms **ou**
   quando o pendente chegar a um teto bem abaixo do anel (256, por exemplo).
   Nenhum anúncio passa à frente de quem lê depressa. Pequeno, só no servidor, e
   fecha o «18 conexões derrubam todo mundo» no caso em que as sessões leem.
3. **Contrapressão em quem escreve.** O teto de mensagens no ar por conexão
   (seção 3). Sem ele, sessões ocupadas — no cadeado da `persistence` que o
   escritor segura, como a F5 mediu sob carga — ainda podem ser passadas para
   trás por lotes seguidos.
4. **O `Lagged` fica** para quem parou de ler, com a despedida que já tem.

**A minha leitura sobre defeito ou contrato:** desligar quem parou de ler é
contrato. Desligar **todo mundo** porque um lote passou do anel é defeito, e num
servidor aberto é uma porta de negação de serviço para quem tiver 18 conexões. E
o que o cliente faz com a despedida é defeito de qualquer jeito.

## Achado de passagem, não verificado

Lido pelo agente que mapeou o cliente, e não medido por mim: TENTAR DE NOVO numa
mensagem recusada por tamanho (`TooLong`) derruba o enlace. `reenviar_mensagem`
(`lib.rs:1964-1986`) não confere o tamanho, o `encode` falha no núcleo, e
`enlace.rs:3808-3810` chama `cair()`. Fica para quem é dono da casca.
