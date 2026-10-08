# O que a F4 pede ao contrato de fio — itens 6 e 17 a 20

**Para a F1, pela orquestradora.** É a Tarefa 0.6 do plano da F4
(`docs/superpowers/plans/2026-10-08-0.16-F4-web.md`).

**De onde vêm os números:**
- a medição 1 (`spikes/ed25519-do-navegador/`);
- a medição 2 (`spikes/proto-em-wasm/`);
- a bancada da medição 3 (`spikes/alpn-no-mesmo-socket/`);
- uma medição de tamanhos feita para este documento, com o `rcgen` 0.14.8 e o
  `postcard` 1.1.3 do `Cargo.lock`.

**O que foi lido:** o branch `0.16/protocolo` em `99caa92`. As âncoras de
`crates/` sem branch são de `versao/0.16` (`d4eb6f1`), conferidas em 08/10. Os nomes de tipo e
de campo abaixo são **propostas**: quem escolhe é a F1.

## Resumo

| # | item | o que a F4 pede | já está no `0.16/protocolo` | falta |
|---|---|---|---|---|
| 6 | o S2 | nada a mais na fórmula. **Cabe no navegador**, medido com o rótulo de verdade | `assinatura.rs` (`a5af6ec`), o cliente nativo e o servidor (`6761914`, `33d2340`) | uma linha de doc em `server.rs` e um guarda no `h3` (ver abaixo) |
| 17 | os hashes futuros e a conferência antes do `Hello` | **duas mensagens novas**: a dos hashes, depois da sessão, e o pedido e a resposta da certidão, antes do `Hello` | — | tudo |
| 18 | o ALPN `h3` e o certificado curto | três constantes no `seele-proto` | — | as constantes; o resto é do `seeled` (F4, Tarefas 2.4 e 2.5) |
| 19 | o link `https://…#…` | o corpo do fragmento igual ao do `seele://`, mais `wh=` e `ate=`, e as regras de validação | — | o analisador (item 19 do contrato) |
| 20 | o enquadramento e o cabeçalho de mídia no `seele-proto` | **nada a mais** | `quadro.rs` (`enquadrar`, `Desenquadrador`) e o `MEDIA_HEADER_VERSION` (`f3aaf9c`) | — |

---

## 6 · O S2 cabe no navegador

**Medido.** A página monta a mensagem de `prova_de_cliente`:
`"SEELE/1 prova-de-cliente\0"` (25 B), mais a impressão (32 B) e o nonce
(32 B). Ela assina com uma `CryptoKey` Ed25519 não extraível.
- No Chromium 151, no Safari 27.0.1 do Mac e no Safari do Simulador iOS 27.0,
  **50 de 50** assinaturas passam pelas linhas do `verify` do `seeled` e pelo
  `verify_strict`, com o `ed25519-dalek` 3.0.0 do `Cargo.lock`.
- As adulteradas são recusadas, 0 de 80.
- O Safari assina de forma aleatória (10 assinaturas distintas para a mesma
  mensagem), e isso não muda nada no `verify`.
- O nonce de exatamente 32 bytes (`a5af6ec`) também cabe.

**Um pedido pequeno.** O doc do campo `Server::impressao`, em `server.rs`, diz
«o SHA-256 do DER que o TLS apresenta».
- No `h3` isso deixa de ser verdade: o TLS apresenta o certificado **curto**, e
  a página não sabe qual curto foi, porque o navegador não mostra.
- A página assina com o `fp=` do link, que é o do **longo**.
- O valor está certo: `impressao` sai de `identity.digest()`, na subida, e não
  da conexão. **O que precisa mudar é a frase**, para ninguém «consertar» o
  servidor para conferir contra o certificado da conexão:

  > A impressão do certificado **de longa duração** deste servidor (a
  > identidade). No `seele/1` é o que o TLS apresenta; no `h3` o TLS apresenta
  > um certificado curto, e a prova continua conferida contra esta.

O guarda que prende isso (uma sessão `h3` cuja prova confere contra o longo, e
não contra o curto apresentado) é da F4, na Tarefa 2.6, porque ainda não existe
`h3` no `seeled`.

**O helper de assinatura é tudo o que o `seele-web` precisa**: ele roda em
`wasm` (a medição 2 rodou o `seele-proto` inteiro num `.wasm` sem cola). A
assinatura em si é do WebCrypto, porque a chave não sai dele.

---

## 17 · Os hashes futuros, e a conferência antes do `Hello`

### A conta que muda a forma

Medido com o `rcgen` 0.14.8, com a chave P-256 do `seeled`:

| o quê | bytes |
|---|---|
| um certificado DER (o longo ou um curto) | **352–354** |
| o longo e 3 curtos, numa mensagem | **1 420** |
| o longo e 30 curtos | 10 978 |
| o longo e 60 curtos | **21 598**: passa do teto de 16 KiB (`control.rs:50`) |
| 3 janelas de hash (hash + duas datas `u64`) | 128 |
| 60 janelas de hash | **2 522** |
| 300 janelas de hash | 12 603 |

A spec temia que 60 DERs não coubessem num quadro, e eles não cabem. **Mas a
conferência não precisa dos DERs futuros.** Os motivos:
- O navegador só aceita certificado válido **agora**. Ele recusou o longo, que
  vale de 1975 a 4096, mesmo com o hash certo, na bancada da medição 3.
- Então a página só passa ao `serverCertificateHashes` os hashes das janelas
  vigentes, no máximo 3.
- A conferência só precisa dos DERs desses hashes. Os DERs das janelas futuras
  chegam no dia em que elas valerem, pela mesma certidão.

**Conclusão: os hashes futuros viajam sozinhos, com as datas. Os DERs viajam só
os da janela vigente, numa mensagem só, sem partes.**

### As janelas (constantes no `seele-proto`)

Proposta, para a F1 pôr em `transport.rs` ao lado de `KEEPALIVE` e `SESSION_GRACE`:

| constante | valor | por quê |
|---|---|---|
| `PASSO_DA_JANELA` | 6 dias | uma janela nova a cada 6 dias |
| `ANTES_DA_JANELA` | 1 dia | o `not_before` recua um dia, pelo relógio torto do celular (o spike faz assim) |
| `VALIDADE_DO_CURTO` | 13 dias | do `not_before` ao `not_after`. Os navegadores recusam acima de 14 (`docs/ios-sem-assinatura-2026-10-07.md:206`) |
| `JANELAS_ANUNCIADAS` | 60 | cerca de um ano à frente: quem entrou uma vez volta por um ano sem link novo |

Com isso, a qualquer momento duas janelas valem ao mesmo tempo, com sobreposição
de 7 dias. O `seeled` pré-gera as 60 e guarda os DERs, que com a mesma chave
somam cerca de 21 KB por servidor. Isso é da F4 (Tarefa 2.5) e não muda o fio.

### A mensagem dos hashes: depois da sessão, nos dois transportes

```rust
/// Os certificados curtos do `h3`, para o navegador guardar por `fp`.
ServerMessage::HashesDoNavegador { janelas: Vec<JanelaDoNavegador> }

pub struct JanelaDoNavegador {
    /// SHA-256 do DER do certificado curto.
    pub hash: [u8; 32],
    /// Segundos Unix, UTC.
    pub nao_antes: u64,
    pub nao_depois: u64,
}
```

- **Quando:** uma vez, logo depois do `Session`. Não precisa de pedido.
- **Para quem:** toda sessão da 1.0, **nos dois transportes**. Um membro do
  desktop com permissão de convite monta links `https` com `wh=`, e precisa dos
  hashes. O nativo que não os usa os ignora. Quem só fala `seele/1` e é o
  anfitrião tira os hashes do próprio banco e não precisa da mensagem.
- **Validação (`Validate`):**
  - no máximo 64 janelas;
  - `nao_depois > nao_antes`;
  - `nao_depois − nao_antes ≤ 14 dias`;
  - a lista em ordem crescente de `nao_antes`.

  São 2,5 KB para 60 janelas, longe do teto.
- **O que o cliente web faz com ela:**
  - guarda **por `fp`, nunca por endereço** (ADR 0057, emenda ao 0003);
  - para um `fp` conhecido, os hashes guardados e ainda válidos têm precedência
    sobre os do link;
  - o hash do link só é usado quando nenhum guardado vale.

### A certidão: antes do `Hello`, para o `fp` voltar a ser a âncora

Hoje o cliente fala primeiro, e o `Hello` leva o convite e a chave pública
(`crates/seele-core/src/client.rs:1940`, em `versao/0.16`). No navegador, isso entrega o convite antes de qualquer
conferência do `fp`, porque a página não vê o certificado. O ADR 0057 exige:
**nenhum segredo sai antes da conferência da lista inteira.**

```rust
/// O cliente pede, antes do Hello. Não leva nada: nem convite, nem chave, nem apelido.
ClientMessage::PedidoDeCertidao

/// O servidor responde com o certificado longo e os curtos que podem estar em uso.
ServerMessage::Certidao {
    /// O DER do certificado de longa duração. O SHA-256 dele é o `fp`.
    longo: Vec<u8>,
    /// Os DERs das janelas vigentes e da próxima.
    curtos: Vec<Vec<u8>>,
}
```

**A ordem, no `h3`:**
1. A página abre a sessão WebTransport, passando ao `serverCertificateHashes`
   os hashes que ela tem para aquele `fp` e que valem agora (no máximo 3).
2. Ela abre o fluxo de controle, o primeiro bidirecional, e manda
   `PedidoDeCertidao`. **O byte 0 do quadro já é a versão**, então um servidor
   de outra versão recusa ali, com os bytes-ouro `[v, 11, 0]`, antes de qualquer
   segredo.
3. O servidor responde `Certidao`: o longo, mais os curtos de **toda janela
   válida agora e da próxima**. São 3 a 4 DERs, cerca de 1,4–1,8 KB, um quadro
   só.
4. **A página confere tudo antes do `Hello`:**
   - `sha256(longo) == fp`, e o `fp` vem do link ou do que está guardado;
   - para **cada** hash que ela passou ao navegador, há em `curtos` um DER com
     aquele SHA-256;
   - cada um desses DERs tem o mesmo `SubjectPublicKeyInfo` do longo.

   Se qualquer uma falhar, ela fecha sem mandar o `Hello` e diz que não
   conseguiu conferir o servidor.

   A prova é esta: o TLS provou a posse da chave do certificado apresentado, e
   todos os certificados que o navegador aceitaria têm a chave do `fp`. Logo,
   quem respondeu tem a chave do `fp` (ADR 0057, emenda ao 0003). **Conferir só
   parte da lista não prova nada**, porque a página não sabe qual certificado
   foi apresentado.
5. Só então o `Hello`, e o resto como hoje: `Challenge`, a prova do S2,
   `Session`.

**O que fica com cada lado:**
- **A F1, no fio:** as duas variantes e a `Validate` da `Certidao`: no máximo 8
  curtos, cada DER com no máximo 2 KiB, e o longo também. Elas entram no
  vocabulário, ao lado de `HashesDoNavegador`.
- **O servidor:** responde `PedidoDeCertidao` nos dois transportes. No
  `seele/1` não é preciso, porque o pino do TLS já confere, mas responder não
  custa nada e mantém uma ordem só. Ele **não** pode exigir o pedido antes do
  `Hello`: quem confere é o cliente, e um servidor não tem como saber se ele
  conferiu.
- **A F4, no `seele-web`:** a função que confere, com um analisador mínimo de
  DER que tira o `SubjectPublicKeyInfo` do `TBSCertificate`. Ela fica fora do
  `seele-proto`, porque é lógica de cliente.

**Se a medição 3 recusar a mesma chave no iPhone:**
- cada janela ganha chave própria;
- a conferência do passo 4 deixa de existir, e com ela a `Certidao`;
- os hashes ficam sendo a única âncora no navegador, e o ADR 0057 escreve esse
  custo antes do código.

A `HashesDoNavegador` continua. **Por isso a `Certidao` deve entrar no fio só
depois da medição 3 do dono**, e a `HashesDoNavegador` pode entrar já.

### Quantos hashes no `serverCertificateHashes`

A bancada aceitou 500 no Chromium 151 e no Safari do Simulador. Com as janelas
acima, a página passa **no máximo 3**: o limite do navegador não restringe o
desenho. A medição 3 do dono confirma no iPhone, e o número só entraria no
contrato se o iPhone aceitasse menos de 3, o que seria surpreendente.

---

## 18 · O ALPN `h3` e o certificado curto

Três constantes em `transport.rs`, para o `seeled` e o `seele-web` lerem do
mesmo lugar:

```rust
/// O ALPN do navegador. O `seeled` anuncia os dois e escolhe o certificado por ele.
pub const ALPN_DO_NAVEGADOR: &[u8] = b"h3";
/// A validade máxima de um certificado curto (ver `VALIDADE_DO_CURTO`).
/// E as quatro constantes de janela do item 17.
```

O resto é do `seeled`, e por isso é da F4:
- o resolvedor por ALPN (Tarefa 2.4);
- os curtos guardados no banco e herdados (Tarefa 2.5).

**Dois fatos medidos que o protocolo não precisa mudar:**
- **Os temporizadores são herdados.** A conexão que o `wtransport` adota herda
  os 20 s de ocioso e os 5 s de keepalive do `Endpoint`. Um navegador morto sem
  fechar cai em 24,6 s no servidor (os 20 s, recomeçados pelo keepalive).
- **O tamanho de datagrama do navegador não vale.** O Safari diz
  `maxDatagramSize` 65535, e o servidor diz 1295 ou 1358. A página usa o
  `media::MAX_DATAGRAM_LEN` (1286) do `seele-proto`, e não o número do
  navegador. A voz de 32 kbit/s em quadros de 20 ms fica muito abaixo disso.

---

## 19 · O link `https://…#…`

**Proposta: o fragmento é o corpo do `seele://`, sem o esquema.** Assim o
analisador é um só (ADR 0057, emenda ao 0006) e reaproveita as chaves que o
`uri.rs` já conhece (`fp`, `convite`, `alt`, `room`, `v`):

```text
seele://198.51.100.7:8383/?fp=<64 hex>&convite=7K4M…&alt=192.168.0.10:8383
https://<origem>/<caminho>#198.51.100.7:8383/?fp=<64 hex>&wh=<64 hex>,<64 hex>&ate=1792540800&convite=7K4M…&v=1.0
```

O `analisar` reconhece a forma `https` da origem configurada, troca
`https://<origem>/<caminho>#` por `seele://` e segue pelo mesmo caminho. Depois
aplica as regras da forma `https`.

**Os campos novos:**
- **`wh=`**: os hashes dos curtos, em hex, separados por vírgula, como o `alt`.
  São no máximo 4: as janelas vigentes e a próxima, no momento em que o convite
  foi feito.
- **`ate=`**: o `nao_depois` mais tardio entre os `wh=`, em segundos Unix UTC.
  É o que deixa a página dizer «este link venceu em DD/MM» em vez de uma recusa
  muda. No Safari, a recusa de certificado volta como `WebTransportError` sem
  texto, e a de porta fechada chega igual (bancada da medição 3).

**As regras, cada uma com um teste:**
1. Na forma `https`, **`fp=` e `wh=` são obrigatórios**. Na `seele://`, o `fp`
   continua opcional.
2. **Cada hash é validado com o rigor do `fp`**: exatamente 64 hex. Truncado ou
   torto é recusado, nunca ignorado.
3. **O convite nunca vai na query.** Uma forma `https` com qualquer query antes
   do `#` é recusada.
4. **Sem `enc=` na forma `https`**: o bilhete do degrau 4 não serve ao
   navegador (ADR 0057). Uma tensão fica aberta: o ADR quer «um link só para as
   duas plataformas», e o `enc=` serviria ao desktop que abrisse o link `https`.
   A recomendação é seguir o ADR (sem `enc=`) até o dono decidir o caminho do
   link junto com o domínio.
5. **O `v=` é a versão do servidor** (item 22, da F6) e **nunca é concatenado
   num caminho**. O filtro de hoje deixa passar `..` (`uri.rs:504-512`).
6. **Um link sem `wh=` é de Servidor 0.15**, ou de um desktop. O analisador
   devolve um erro próprio (por exemplo `ErroDeUri::SemHashesDoNavegador`),
   diferente de «link torto», para a página dizer «Este convite é de um
   Servidor 0.15, que o celular não alcança» sem discar. **Na tela, nunca
   «linha»** (ADR 0033 e o guarda `nenhum_termo_aposentado_volta_para_o_texto_da_interface`).
7. **A origem é uma constante** que fica vazia até o D-web-2. Os testes usam
   `origem.invalid`. O caminho (`/e` ou a raiz) é decidido junto com o domínio.

Os casos de teste propostos estão no plano da F4, Tarefa 2.3. O tamanho de um
link com 3 hashes, `fp`, convite e um `alt` fica em cerca de 450 caracteres, o
que ainda cabe num QR legível.

**O desktop 0.16 também abre o link `https`**: o mesmo analisador, com o `fp`,
ignorando `wh=` e `ate=`. A v0.15.1 publicada o recusa (o teste em `uri.rs:710`), e só
uma página que redirecione para `seele://` o atenderia. Isso é decidido com o
domínio.

---

## 20 · O enquadramento e o cabeçalho de mídia

**Nada a pedir.** O `0.16/protocolo` já tem:
- `quadro.rs`, com `enquadrar` e `Desenquadrador`. É o que a página precisa,
  porque a WebTransport entrega pedaços e não tem `read_exact`. O teto é
  conferido no prefixo, antes de alocar;
- o `MediaHeader` (`media.rs`), puro, com o número próprio da voz
  (`MEDIA_HEADER_VERSION = 1`, `f3aaf9c`).

Uma observação, e não um pedido: o `Desenquadrador::empurrar` aceita qualquer
tamanho, e a garantia «nunca passa de um quadro mais o último pedaço» depende de
quem chama chamar `proximo` a cada pedaço. O cliente web vai chamar.

---

## O que mais o celular precisa do protocolo

1. **Nada para a reconexão.** A reserva de 5 minutos é por `PersonId`
   (`server.rs:615-633`), e não por conexão nem endereço. O navegador não
   migra conexão, e cada troca de rede vira uma conexão nova que cai no mesmo
   lugar.
2. **Nada para os MODs.** Com o executor AB (spec-mestre `00935da`), o celular
   roda a API 6 como o desktop. O item 16 sai, e o `Hello` declara as mesmas
   APIs (D-fio-7).
3. **Nada para a tela compartilhada**, que fica fora do celular na 0.16 (o bug
   do WebKit em 16 MB ou 7600 fluxos). Quem decide não mandar tela a uma sessão
   `h3` é o servidor, pelo transporte, sem mudar o fio. A casca do celular diz
   por que a tela não aparece.
4. **Um pedido para a sessão do Plano 3, e não para o vocabulário.** Hoje o
   convite é gasto (`session.rs:678-687`) antes do `NicknameTaken`
   (`:750-757`). No celular isso é pior que no desktop, porque a identidade
   presa à origem se perde com mais facilidade (ADR 0057, «O que isto custa»).
   **Conferir o apelido antes de gastar o convite**, nos dois transportes,
   evita que a pessoa perca o convite por um apelido ocupado.
5. **O campo `client` do `Hello`** leva `seele-web/<versão>`, para o registro
   de quem hospeda. Não precisa de mudança.
6. **O `Origin` do pedido WebTransport** chega ao servidor (bancada da medição
   3). Ele é só para o registro, porque se o `seeled` o confere é decisão do
   dono, e não muda o fio.

## O que a F4 entrega em troca

- o crate `seele-web` (Tarefa 2.1), com os bytes-ouro do `tests/fio_1_0.rs`
  conferidos no `wasm`;
- a conferência da certidão, com o analisador de DER;
- os testes da forma `https` do analisador (Tarefa 2.3), para a F1 incorporar.
