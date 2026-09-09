# Task 4: Os dois discam, e o resultado é enumerado — relatório

## Status

DONE.

## Commit

`a6eee0d` — "feat(par): os dois discam, e o resultado diz como a ligação
chegou", em `crates/seele-core/src/par.rs` (único arquivo tocado).

## O que foi entregue

- `pub enum ComoChegou { Local, Furo }` e `pub struct ParLigado { conexao, como, ida_e_volta }`
  (com `#[derive(Debug)]` — o `unwrap_err()` do segundo teste exige, e o brief
  não previa).
- `pub async fn ligar(&quinn::Endpoint, &[SocketAddr], String, Duration) -> Result<ParLigado, ErroDePar>`
  — todos os endereços em paralelo num `JoinSet`, `timeout_at` sobre o prazo
  inteiro, primeira conexão que fecha vence e `abort_all()` derruba o resto.
  `ImpressaoNaoBate` retorna na hora; o silêncio só vira `NaoAlcancou` no fim.
- `fn como_chegou(SocketAddr) -> ComoChegou` e `fn classificar(&Relato, &quinn::ConnectionError) -> ErroDePar`.
- Limpeza pedida: os dois `#[allow(dead_code)]` (sobre `ConfereImpressao` e
  `config_de_cliente`) saíram, junto com os parágrafos de doc que diziam «a
  Task 4 ainda não existe» — que passaram a ser mentira neste commit.
  `ConfereImpressao::nova` ganhou doc e `#[must_use]`, no molde de
  `TofuVerifier::new`.

## A decisão sobre `classificar`: o caminho limpo deu certo

**A comparação de texto de mensagem de erro foi apagada.** Não sobrou nenhum
`contains` no módulo.

O que funcionou foi a primeira sugestão do brief, com uma diferença que importa:

```rust
pub(crate) type Relato = std::sync::Arc<std::sync::Mutex<Option<ErroDePar>>>;
```

- `ConfereImpressao` ganhou um campo `relato: Relato`, criado por `nova`.
- `verify_server_cert` **guarda o `ErroDePar` tipado na vaga antes** de o
  achatar em `rustls::Error::General(String)` para devolver ao `rustls`.
- `config_de_cliente` passou a devolver `(quinn::ClientConfig, Relato)` — é a
  diferença que importa. Sem ela não há como alcançar a vaga de fora: quem
  constrói o verificador é ela, e o `Arc` morre lá dentro. Devolver o par
  também amarra vaga e configuração: **uma vaga por discagem**, criada no laço
  de `ligar`, então dois endereços tentados em paralelo nunca disputam a mesma.
- `classificar` lê a vaga: cheia é `ImpressaoNaoBate` com os dois hashes
  inteiros; vazia é `NaoAlcancou`.

Isso é estritamente melhor que a versão do brief, e não só por estética: a
`classificar` que comparava texto devolvia `ImpressaoNaoBate { esperada: "",
veio: "" }` — os dois campos vazios. Ela sabia que alguém respondeu com o
certificado errado e **não sabia dizer qual**, que é justamente o dado que um
evento de segurança precisa carregar. O caminho tipado traz os dois hashes, e
o teste passou a afirmar isso.

O `quinn::ConnectionError` não é jogado fora: `classificar` o registra num
`tracing::debug!` no braço de `NaoAlcancou`, porque a variante não tem onde o
guardar e perdê-lo em silêncio é o defeito de sempre deste repositório.

Única mudança de assinatura em relação ao brief: `config_de_cliente` devolve
tupla. Nenhum outro chamador existe (era `dead_code` até hoje), então nada
quebrou.

## O que foi medido e obrigou a mexer nos testes do brief

**Os dois testes do brief, escritos exatamente como estão lá, falham** — os
dois com `NaoAlcancou` no fim dos 5 segundos. Medido, não suposto:

```
test par::testes::um_par_com_a_impressao_errada_nao_liga ... FAILED
  a recusa saiu com o motivo errado: NaoAlcancou
test par::testes::dois_pares_se_ligam_e_o_teste_sabe_como ... FAILED
  os dois pares não se ligaram: NaoAlcancou
```

Causa: **o `quinn` 0.11 não responde nada a uma tentativa que chega até alguém
chamar `Endpoint::accept()`.** Ele enfileira um `Incoming` e fica calado.
`passar_a_atender` (Task 2) só instala o `ServerConfig`; ninguém atende. Nos
dois testes a ponta `a` era muda, então a discagem de `b` morria no prazo — e
no segundo teste o verificador de `b` nunca chegava a ver certificado nenhum,
que é por que o motivo saía `NaoAlcancou` em vez de `ImpressaoNaoBate`.

Conserto: um auxiliar de teste, `atender_em_segundo_plano(ponta)`, que roda
`while let Some(chegando) = ponta.accept().await` e **segura** as conexões
aceitas (largar uma `quinn::Connection` a fecha, e o primeiro teste pergunta a
quem discou se ela continua viva). Três linhas nos dois testes, tudo o mais
verbatim do brief.

## Uma asserção a mais no segundo teste, e ela é o que faz o guarda morder

O `assert!(matches!(erro, ImpressaoNaoBate { .. }))` do brief **não prende o
guarda que o Step 5 manda provar**. Com `ligar` discando com `String::new()`
em vez da impressão pedida, o verificador continua recusando (`""` não bate com
hash nenhum) e o `matches!` continua passando. O teste ficaria verde com a
parede errada no lugar.

Acrescentei, depois do `matches!` do brief:

```rust
if let ErroDePar::ImpressaoNaoBate { esperada, veio } = &erro {
    assert_eq!(esperada, &"f".repeat(64), "não foi a impressão pedida");
    assert_eq!(veio.len(), 64, "o que veio não é SHA-256 em hex");
    assert_ne!(veio, esperada, "os dois lados do erro são o mesmo hash");
}
```

Ela prova duas coisas de uma vez: que `ligar` discou com a impressão que lhe
deram, e que o motivo chegou com os campos preenchidos — o que só o caminho
tipado consegue.

## TDD, passo a passo

1. Escrevi os dois testes do brief, verbatim.
2. Rodei: `error[E0425]: cannot find function 'ligar'` / `error[E0433]: cannot
   find type 'ComoChegou'`, como o brief previa.
3. Implementei.
4. Rodei: os dois falharam com `NaoAlcancou` (ver acima). Diagnostiquei,
   acrescentei o laço de atendimento, rodei de novo: **6 testes em `par::`,
   verdes** — os 4 das Tasks 1–3 mais os 2 desta, exatamente a contagem que o
   brief previa.

## Step 5 — os dois guardas provados por reversão

1. `config_de_cliente(String::new())` no lugar de `config_de_cliente(impressao_esperada.clone())`:
   `um_par_com_a_impressao_errada_nao_liga` **FALHOU** —
   `assertion 'left == right' failed: não foi a impressão pedida; left: "";
   right: "ffff…"`. (E `dois_pares_se_ligam_e_o_teste_sabe_como` também, com
   `ImpressaoNaoBate { esperada: "", veio: "f40bc…" }` — a discagem legítima
   deixa de fechar quando a impressão é ignorada.) Desfeito.
2. `como_chegou` devolvendo sempre `ComoChegou::Furo`:
   `dois_pares_se_ligam_e_o_teste_sabe_como` **FALHOU** —
   `left: Furo; right: Local`, e só ele. Desfeito.

## Verificação final

- `cargo test -p seele-core` — **257 passed, 0 failed** (eram 255; +2 desta
  tarefa). `par::` sozinho: 6 passed.
- `cargo fmt -p seele-core` — aplicado, sem sobra.
- `cargo clippy -p seele-core --all-targets` — **zero avisos**.
- `cargo doc -p seele-core --no-deps` — 6 avisos, todos pré-existentes e em
  outros módulos (`bomba.rs`, `tofu.rs`, `enlace.rs`, `sessao.rs`). Nenhum em
  `par.rs`.

## Preocupações

1. **`ligar` só disca, e o furo simétrico precisa de quem atenda — e esse laço
   ainda não tem dono.** É a preocupação séria desta tarefa. O plano (linha
   1169) manda os dois lados chamarem `passar_a_atender` **e** `ligar`, «as duas
   tentativas simultâneas são o furo». Como o `quinn` não responde a quem chega
   sem um `accept()`, dois pares que só chamem `ligar` **nunca se ligam**: cada
   um disca, cada um enfileira o `Incoming` do outro, e ninguém responde. O laço
   de atendimento é obrigatório e não está em nenhuma tarefa deste plano que eu
   consiga apontar. Quem escrever a Task 8 precisa dele antes de qualquer outra
   coisa.
2. **Quem atende não confere identidade nenhuma.** Foi por isso que **não** pus
   o `accept()` dentro de `ligar`, apesar de ser tentador («a primeira que fecha
   vence» incluiria a que chegou). O `ServerConfig` de `passar_a_atender` usa
   `with_no_client_auth()`: uma conexão *aceita* não passa por
   `ConfereImpressao` nem por nada parecido. Quem empresta a subida estaria
   mandando quadro de tela para qualquer um que alcançasse a porta — exatamente
   o que `ImpressaoNaoBate` existe para impedir na direção contrária. A parede
   simétrica (certificado de cliente, ou um segredo em banda que o servidor
   apresenta aos dois) é uma decisão de desenho que este plano ainda não tomou,
   e ela tem de ser tomada antes de a Task 8 repassar o primeiro quadro.
3. **`ComoChegou` só olha o endereço, não a rota.** `como_chegou` classifica
   por faixa de IP (privado/loopback/link-local/ULA). Um par atrás de CGNAT
   (`100.64/10`) cai em `Furo`, o que está certo; mas duas máquinas na mesma
   LAN que se alcancem pelo endereço público (hairpin de NAT) seriam contadas
   como `Furo` sem furo nenhum. Para a medida que o subprojeto A quer tirar —
   *que fração dos pares o furo alcança* — isso infla o numerador. O roteiro de
   duas máquinas da Task 10 deve registrar a topologia junto do resultado, ou o
   número não vai querer dizer o que promete.
4. **A vaga envenenada degrada para `NaoAlcancou`.** Se o `Mutex` do `Relato`
   estiver envenenado (só acontece se o verificador entrar em pânico, o que não
   deveria), `classificar` devolve `NaoAlcancou` em vez do motivo real —
   silencia um evento de segurança. `unwrap_used` é `deny`, então não havia como
   afirmar o contrário; o comportamento está comentado no código. Menor, mas
   registrado.
5. **`config_de_cliente` devolve tupla agora.** Se alguma tarefa posterior do
   plano supunha a assinatura antiga (`-> Result<quinn::ClientConfig, ErroDePar>`),
   ela precisa desempacotar. Nenhum chamador existia até este commit.

---

# Task 4 — fix round 1/5

## Status

DONE.

## Commit

`5497c97` — "fix(par): a impostura de um endereço deixa de matar a discagem
inteira", em `crates/seele-core/src/par.rs` (único arquivo tocado).

## 1 · O rastro passa a dizer qual endereço (Important)

Três buracos, três consertos:

- **A falha não nomeava o candidato.** `classificar` nem recebia o
  `SocketAddr`; a linha dizia «um endereço deste par não fechou aperto de mão»
  numa discagem com N candidatos em paralelo. Agora cada tarefa devolve
  `(endereco, resultado)`, `classificar(&relato, endereco, &erro)` põe
  `par = %endereco` nas duas linhas que emite, e a mensagem virou «este
  endereço do par não fechou aperto de mão».
- **A vitória não dizia por onde entrou.** É literalmente a falta que o
  `e56dbb2` consertou na corrida de candidatos do servidor, e o comentário no
  código aponta para lá. `tracing::info!(par = %conexao.remote_address(), ?como,
  ?ida_e_volta, "um par ligou")`. Junto, um `debug!` com as tentativas
  abandonadas, quando há alguma.
- **O prazo vencia em silêncio total.** As tentativas ainda no ar eram
  abortadas e o `ConnectionError` delas sumia. Não dá para recuperar o que elas
  diriam — elas não terminaram —, mas dá para dizer **quais** eram: `no_ar`
  acompanha os candidatos que ainda não responderam (removido um a cada
  resposta), e o fim emite `?prazo, ?no_ar` antes do `abort_all()`.

O `while let` virou `loop` explícito: ele não distinguia «venceu o prazo» de
«todas responderam», e a linha nova mentiria nesse segundo caso. Agora há um
`venceu_o_prazo` e a linha só sai quando é verdade. O braço de `JoinError`
(tarefa que morre sem devolver nada) ganhou o próprio `warn!` — é o único caso
em que o rastro não sabe de quem fala, e ele diz isso.

## 2 · A preempção saiu, e o guarda dela está escrito

Você tem razão, e o revisor também: a justificativa era de precedência no fim e
o código fazia preempção. Agora:

- `classificar` emite `tracing::warn!(par = %endereco, %motivo, "alguém
  respondeu no lugar do par")` **na hora em que detecta**, que é onde a
  impostura é notícia. Ela é registrada mesmo quando a ligação acaba vencendo
  por outro endereço e o erro nunca é devolvido a ninguém.
- O laço não retorna mais na hora. `ultimo` guarda o motivo com a precedência
  que o brief queria — `ImpressaoNaoBate` nunca é sobrescrito por `NaoAlcancou`
  nem por `Escuta` — e só é devolvido se nenhuma tentativa vencer.

**Teste novo, e ele precisou de um detalhe para prender de verdade:**
`um_impostor_na_lista_nao_derruba_a_discagem_inteira` — dois endereços, um
impostor e um legítimo, a ligação tem de fechar com o legítimo
(`ligado.conexao.remote_address() == endereco_legitimo`). O legítimo atende com
150 ms de atraso **de propósito**: sem isso as duas tentativas correm juntas em
`127.0.0.1`, quem chega primeiro é moeda ao ar, e o teste passaria por acidente
metade das vezes. Com o atraso, o impostor falha antes — que é exatamente a
ordem em que a preempção destruía a discagem. Para isso o auxiliar
`atender_em_segundo_plano` ganhou um parâmetro `atraso` (`Duration::ZERO` nos
outros dois testes).

## 3 · `fe80::/10` entrou em `como_chegou`

O lado v4 conferia `is_link_local()`; o v6 conferia só `fc00::/7`. Um
link-local v6 contava como `Furo`, inflando exatamente a medida que o
subprojeto A existe para produzir. A conferência agora é
`s & 0xfe00 == 0xfc00 || s & 0xffc0 == 0xfe80`, com um comentário dizendo por
que é máscara na mão (`is_unicast_link_local` do `std` é API instável).

Não mexi no IPv6 global sem NAT nem no hairpin, como você mandou.

## Testes que rodei

```
$ cargo test -p seele-core --lib par::
running 9 tests
test par::testes::o_que_e_da_mesma_rede_nunca_conta_como_furo ... ok
test par::testes::a_impressao_e_a_do_certificado_e_no_formato_de_sempre ... ok
test par::testes::cada_identidade_e_nova_e_a_impressao_a_distingue ... ok
test par::testes::o_par_certo_passa_e_o_errado_e_recusado_com_o_motivo_certo ... ok
test par::testes::uma_ponta_de_cliente_passa_a_atender_sem_socket_novo ... ok
test par::testes::um_par_com_a_impressao_errada_nao_liga ... ok
test par::testes::dois_pares_se_ligam_e_o_teste_sabe_como ... ok
test par::testes::um_impostor_na_lista_nao_derruba_a_discagem_inteira ... ok
test par::testes::o_prazo_manda_quando_ninguem_responde ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 251 filtered out

$ cargo test -p seele-core
test result: ok. 260 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo clippy -p seele-core --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.75s
(zero avisos)

$ cargo doc -p seele-core --no-deps
6 avisos, os mesmos de antes, todos em bomba.rs/tofu.rs/enlace.rs/sessao.rs.
Nenhum em par.rs.
```

`cargo fmt -p seele-core` aplicado antes de cada rodada.

## Os dois consertos novos, provados por reversão

1. **Preempção restaurada** (`Err(erro @ ImpressaoNaoBate { .. }) => return Err(erro)`):
   `um_impostor_na_lista_nao_derruba_a_discagem_inteira` **FALHOU** —
   `o impostor derrubou a discagem inteira: ImpressaoNaoBate { esperada: "87b06…",
   veio: "6e125…" }`, e só ele. Desfeito.
2. **`fe80::/10` removido de `como_chegou`**:
   `o_que_e_da_mesma_rede_nunca_conta_como_furo` **FALHOU** —
   `[fe80::1]:9 é da mesma rede e foi contado como furo; left: Furo; right: Local`,
   e só ele. Desfeito.

Terceiro teste novo, `o_prazo_manda_quando_ninguem_responde`: dois endereços das
faixas de documentação (`192.0.2.0/24`, `198.51.100.0/24`), prazo de 300 ms,
afirma `NaoAlcancou` e que a chamada volta dentro de 3 s. É o único que exercita
o braço do prazo vencido com tentativas no ar. **O que ele não prova, e está
escrito no próprio teste:** que a linha de rastro saiu — ele não instala
assinante de `tracing` e não lê log nenhum. Preferi dizer isso a fingir
cobertura.

## O que continua de pé das preocupações anteriores

As duas primeiras, intactas e não endereçadas por este round: **`ligar` só
disca e o laço de atendimento não tem dono em nenhuma tarefa do plano**, e
**quem atende não confere identidade nenhuma** (`with_no_client_auth()`). A
segunda ficou mais visível com este conserto: a discagem agora sobrevive a um
impostor na lista, mas o lado que *atende* continua abrindo a porta para
qualquer um.
