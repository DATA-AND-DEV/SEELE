# Task 5: Quem atende também confere, e alguém precisa atender — relatório

## Status

DONE.

## Commit

`48067d4` — "feat(par): quem atende confere quem chega, e alguém atende", em
`crates/seele-core/src/par.rs` (único arquivo tocado).

## O que foi entregue, do brief

- `pub fn passar_a_atender(ponta, identidade, impressao_de_quem_vem: String) -> Result<(), ErroDePar>`
  — terceiro parâmetro novo; troca `with_no_client_auth()` por
  `with_client_cert_verifier(Arc::new(ConfereQuemChega::nova(impressao_de_quem_vem)))`.
- `pub(crate) struct ConfereQuemChega`, implementando
  `rustls::server::danger::ClientCertVerifier`: `client_auth_mandatory() ==
  true`, `root_hint_subjects() == &[]`, `verify_client_cert` confere a
  impressão digital de quem chega contra `esperada` — os três métodos
  mecânicos (`verify_tls12_signature`, `verify_tls13_signature`,
  `supported_verify_schemes`) copiados de `ConfereImpressao`/`TofuVerifier`,
  como mandado.
- `pub async fn atender(ponta: quinn::Endpoint) -> Option<ParLigado>` — atende
  **uma** ligação, não um laço.
- Os dois testes do Step 1, escritos verbatim.
- `atender_em_segundo_plano` removido de `mod testes`: sem uso depois que
  `atender` existe, como o brief mandou.

## TDD, passo a passo

1. Escrevi os dois testes do brief, verbatim.
2. Rodei `cargo test -p seele-core --lib`: falha de compilação —
   `passar_a_atender` com aridade errada e `atender` inexistente. Como o brief
   previa.
3. Implementei o Step 3 **literalmente**: `ConfereQuemChega` exatamente como
   descrito, `atender` exatamente como descrito.
4. Rodei de novo: **os dois testes novos falharam**, e não por acidente de
   digitação — ver a seção seguinte, porque a causa é o motivo desta tarefa ter
   crescido além do brief.

## Medido, não suposto: o Step 3 literal não fecha verde

Rodar os dois testes do brief contra a implementação literal do Step 3 dá:

```
test par::testes::dois_pares_apresentados_se_ligam_pelos_dois_lados ... FAILED
  quem atendeu não devolveu a ligação: quem empresta não tem por onde mandar quadro
test par::testes::quem_atende_recusa_quem_o_servidor_nao_apresentou ... FAILED
  o intruso apresentou um certificado que o anfitrião nunca esperou, e entrou
```

Diagnostiquei com `eprintln!` temporário em vez de supor. A causa é dupla, e as
duas são propriedades do TLS 1.3/QUIC, não bugs do meu código:

1. **`config_de_cliente` nunca apresentava certificado de cliente algum**
   (`with_no_client_auth()`, inalterado pelo brief). Com `client_auth_mandatory
   == true` do lado de quem atende, **toda** discagem — inclusive a do caso
   feliz — chega sem certificado, e `rustls` recusa com
   `NoCertificatesPresented` antes mesmo de completar o aperto de mão do lado
   do servidor. `atender` corretamente devolve `None`; o problema é que o caso
   feliz também devolve `None`.
2. **O TLS 1.3, do lado de quem disca, considera o aperto de mão concluído
   assim que processa o `Finished` do par** — antes de enviar, e muito antes
   de o par validar, a própria contrapartida (o certificado de cliente que
   `ConfereQuemChega` exige). O diagnóstico mostrou `close_reason()` **já
   preenchido** (`"peer sent no certificates"`) no instante em que `ligar()`
   retornava `Ok` — a recusa do servidor chega como fechamento assíncrono da
   conexão, não como erro do `.connect()`.

Nenhuma das duas é o que os dois defeitos nomeados no início desta tarefa
descrevem — são um **terceiro e quarto** achado, na mesma veia do «Task 4
mediu em vez de supor». Reportando em vez de escondendo atrás de uma
implementação que parece seguir o brief à risca mas não fecha os próprios
testes que o brief pede.

## Os dois consertos que fecharam a lacuna (além do Step 3)

1. **`passar_a_atender` agora também registra a identidade para quando a
   mesma ponta disca.** No A1, quem empresta reaproveita a mesma ponta e a
   mesma porta para atender e para discar (é o ponto do teste
   `uma_ponta_de_cliente_passa_a_atender_sem_socket_novo`); a API pública do
   `quinn::Endpoint` não devolve identificador estável nenhum, então a chave é
   o endereço local. Um mapa privado do módulo,
   `IDENTIDADE_PARA_DISCAR: LazyLock<Mutex<HashMap<SocketAddr,
   (Vec<CertificateDer>, PrivateKeyDer)>>>`; `ligar` consulta
   `identidade_para_discar(ponta)` e, se achar, disca com
   `with_client_auth_cert` em vez de `with_no_client_auth`.
2. **`ligar` dá ao par uma folga curta antes de dar a ligação por boa.** Depois
   de `ligando.await` fechar, se `conexao.close_reason()` já estiver
   preenchido, ou se `conexao.closed()` disparar dentro de uma folga (o dobro
   do ida-e-volta que o próprio aperto de mão mediu, com piso de 20 ms), o
   motivo vira `ErroDePar::RecusadoDepoisDeLigar(String)` — variante nova, sem
   nenhum chamador exhaustivo fora deste módulo para quebrar. Sem a folga, uma
   ligação recusada pelo par ainda seria dada por boa antes de o fechamento
   chegar.

Rodei o teste da parede simétrica e o do caso feliz cinco vezes seguidas para
afastar acaso de tempo — cinco em cinco, verdes.

## Um terceiro guarda que o brief não previa, e que eu precisei provar sozinho

Com o registro de identidade em vigor, o intruso do primeiro teste **deixa de
apresentar certificado vazio** — passa a apresentar o certificado real dele
(registrado ao chamar `passar_a_atender` nele mesmo), que ainda não bate com o
que o anfitrião espera. Isso prova `verify_client_cert`, mas **não prova mais
`client_auth_mandatory`**: revertendo-o para `false` e rodando a suíte inteira,
**nenhum teste cai** — nenhum outro cenário chega ao aperto de mão com
certificado de cliente genuinamente vazio depois de o registro existir. Um
guarda que existe e não morde é o defeito que este repositório já pagou caro.

Escrevi um teste a mais, `quem_atende_recusa_quem_nao_apresentou_certificado_nenhum`:
quem disca nunca chamou `passar_a_atender` (sem identidade registrada, então
sem certificado nenhum), contra um servidor cujo certificado é o certo (a
verificação do lado do cliente passa, então a discagem chega inteira até a
decisão de `client_auth_mandatory`). Confirmei que ele passa com o código
correto e falha sozinho com `client_auth_mandatory() == false` — nenhum outro
teste da suíte se move.

## Step 5 — as três reversões do brief, e uma quarta

1. `with_client_cert_verifier(...)` → `with_no_client_auth()`:
   `quem_atende_recusa_quem_o_servidor_nao_apresentou` **FALHOU** —
   "o intruso apresentou um certificado que o anfitrião nunca esperou, e
   entrou". Desfeito.
2. `client_auth_mandatory` → `false`: o teste do brief **não caiu** (ver acima
   — passou a exercitar só `verify_client_cert`, não mais `mandatory`); o
   teste que escrevi para isto, `quem_atende_recusa_quem_nao_apresentou_certificado_nenhum`,
   **FALHOU**, e só ele — 11 outros continuaram verdes. Desfeito.
3. `atender` devolvendo `None` sem chamar `accept()`:
   `dois_pares_apresentados_se_ligam_pelos_dois_lados` **FALHOU** — desta vez
   nem `ligar()` fechou (`NaoAlcancou`, porque literalmente ninguém atende).
   Desfeito.

## Verificação final

```
$ cargo test -p seele-core --lib par::
running 12 tests ... ok. 12 passed; 0 failed

$ cargo test -p seele-core
test result: ok. 263 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt -p seele-core -- --check
(nada)

$ cargo clippy -p seele-core --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.86s
(zero avisos)

$ cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 22.62s
```

`par::config_de_cliente`, `par::ligar` e `par::passar_a_atender` não têm
nenhum chamador fora de `par.rs` no restante do workspace (`grep` confirmou) —
mudar a assinatura interna de `config_de_cliente` (ganhou um segundo
parâmetro, `pub(crate)`) e adicionar uma variante a `ErroDePar` (também sem
`match` exaustivo fora deste arquivo) não quebrou nada em outro crate.

## Preocupações

1. **A maior: esta tarefa cresceu além do que o brief descreve, porque o
   brief tinha uma lacuna real, não uma que eu inventei.** O Step 3, seguido à
   risca, não fecha os próprios dois testes que o Step 1 manda escrever — isto
   não é uma opinião de estilo, é o resultado de rodar. Consertei com (a) um
   registro de identidade por endereço local, e (b) uma folga pós-conexão em
   `ligar` mais uma variante nova de `ErroDePar`. Ambos são mudanças de
   comportamento em `config_de_cliente`/`ligar`, que o brief lista como
   «Consumes», não como algo a mudar. Quem revisar esta tarefa deveria conferir
   se concorda com a forma do conserto, não só se os testes ficaram verdes.
2. **O registro de identidade é uma tabela global, keyed por
   `SocketAddr`.** É a única informação que a API pública do `quinn::Endpoint`
   expõe de fora do crate `quinn` que identifica uma ponta de forma estável o
   bastante — `Endpoint` não implementa `Hash`/`Eq`/`PartialEq`, e
   `set_default_client_config` pede `&mut Endpoint`, incompatível com a
   assinatura `&quinn::Endpoint` que o próprio brief fixa para
   `passar_a_atender`. O ponto fraco: nada remove uma entrada quando a ponta
   correspondente é derrubada, então se um processo de longa duração criar e
   descartar muitas pontas na mesma faixa de porta efêmera, uma porta
   reciclada por uma ponta nova poderia herdar a identidade de uma ponta
   antiga e já morta — um bug funcional (uma ligação sairia com o certificado
   errado), não um vazamento de material privado para terceiros, mas ainda
   assim algo que uma tarefa futura devia resolver com mais rigor do que um
   mapa global permite.
3. **A folga em `ligar` é o dobro do RTT medido, com piso de 20 ms — um
   número escolhido, não derivado de nenhuma medida real de rede.** Funciona
   de forma estável em `127.0.0.1` (cinco rodadas seguidas, sem flakiness). Não
   testei sob RTT alto/perda de pacote real; é razoável que baste (a recusa do
   servidor sai antes de qualquer aplicação começar a trocar quadro), mas o
   roteiro de duas máquinas da Task 10 é quem pode confirmar isso com dados.
4. **`RecusadoDepoisDeLigar` carrega só uma `String`** (o `to_string()` do
   `quinn::ConnectionError`), não um motivo tipado como `ImpressaoNaoBate`. Não
   distingue «servidor recusou por falta de certificado» de «servidor recusou
   por certificado errado» de «a conexão morreu por outro motivo qualquer
   depois de conectar» — os três caem na mesma variante. Dado que o teste só
   precisa de `is_err()`, não persegui uma distinção mais fina; se uma tarefa
   futura precisar diferenciar esses casos para telemetria, esta variante
   precisa crescer.
5. **Herdadas da Task 4, ainda de pé:** a preocupação nº 3 de lá
   (`ComoChegou` não distingue hairpin de NAT) e a nº 4 (vaga envenenada de
   `Relato` degrada para `NaoAlcancou`) continuam sem conserto — fora do
   escopo desta tarefa.

---

# Task 5 — fix round 1/5

## Status

DONE.

## Commit

`5c5fd8b` — "fix(par): confirmação em vez de relógio, motivo enumerado, tabela
sai", em `crates/seele-core/src/par.rs` (único arquivo tocado).

## O que a revisão aprovou, e o que voltou

Aprovado sem ressalva: o núcleo criptográfico (`ConfereQuemChega`,
`client_auth_mandatory`) e o crescimento de escopo em si — a revisão mediu
sozinha que o guarda de `client_auth_mandatory` só morde por causa do teste
`quem_atende_recusa_quem_nao_apresentou_certificado_nenhum`, que eu escrevi
por conta própria depois de notar que o registro de identidade tinha
desarmado o teste do brief.

O que voltou foi a **forma** de dois dos crescimentos (a tabela global e a
folga por relógio), o tipo do erro novo, e dois consertos menores. Os cinco
pontos, na ordem em que a revisão os deu:

### 1 · A tabela global sai; `ligar` ganha `identidade_propria: Option<&Identidade>`

Removidos: `IDENTIDADE_PARA_DISCAR`, `registrar_identidade_para_discar`,
`identidade_para_discar`, `ChaveDeIdentidade`, `IdentidadeGuardada`. A
assinatura nova:

```rust
pub async fn ligar(
    ponta: &quinn::Endpoint,
    enderecos: &[std::net::SocketAddr],
    impressao_esperada: String,
    identidade_propria: Option<&Identidade>,
    prazo: std::time::Duration,
) -> Result<ParLigado, ErroDePar>
```

`config_de_cliente` (já `pub(crate)`, já mudada nesta tarefa) passou a receber
`Option<&Identidade>` em vez do par `(Vec<CertificateDer>, PrivateKeyDer)` que
a tabela guardava. Como `config_de_cliente` roda **fora** da tarefa spawnada —
antes de `tentativas.spawn(...)` — e devolve um `quinn::ClientConfig` já dono
dos próprios bytes, a referência emprestada de quem chamou `ligar` nunca
precisa ser `'static`; o `.clone()`/`.clone_key()` acontece uma vez, dentro de
`config_de_cliente`, e o valor que sai dali é que atravessa o `async move`.
Isso simplificou o laço de `ligar`: a dança de clonar a chave por tentativa
(porque `PrivateKeyDer` não implementa `Clone`) que existia por causa da
tabela não existe mais — não há mais nada para clonar no laço.

`Identidade` ganhou `impl Clone` manual (não dá `#[derive]`, `PrivateKeyDer`
só tem `clone_key()`). É a peça que faltava para os testes: como
`passar_a_atender` consome a identidade, todo teste em que a mesma ponta
atende e disca agora clona a identidade **antes** de entregá-la a
`passar_a_atender`, e guarda a cópia para `ligar`.

### 2 · A folga de 2×RTT vira troca de byte de aplicação

A revisão mediu: neutralizando a checagem pós-conexão, as duas paredes
(`quem_atende_recusa_quem_o_servidor_nao_apresentou` e
`quem_atende_recusa_quem_nao_apresentou_certificado_nenhum`) caem. Reproduzi —
ver a seção de reversões abaixo — antes de trocar o mecanismo, para não trocar
uma coisa que eu não tivesse visto quebrar de verdade.

`confirmar_com_quem_atende` (lado de quem disca): abre um fluxo bidirecional,
manda um byte, espera um byte de volta. `confirmar_para_quem_ligou` (lado de
`atender`): aceita o fluxo, lê o byte, devolve outro. Numa conexão que o par
recusou, o `open_bi` ou a leitura seguinte falha com o `ConnectionError` de
verdade assim que o `CONNECTION_CLOSE` chega — sem relógio, sem margem, sem
depender de o datagrama de fechamento caber dentro de uma janela chutada.
Reproduzi o mesmo experimento da revisão contra o mecanismo novo — neutralizar
a chamada a `confirmar_com_quem_atende` em `ligar` — e as duas paredes caem de
novo, exatamente como esperado de um guarda que existe de verdade; desfiz.
Isso é o Step 5 revalidado contra a nova implementação, não só o velho.

### 3 · `RecusadoDepoisDeLigar` carrega `MotivoDaRecusa`

```rust
pub enum MotivoDaRecusa {
    SemCertificado,       // alerta 116, certificate_required
    CertificadoErrado,    // alertas 40, 42, 48
    Outro(String),        // prazo, reinício, etc. — sem relação com o certificado
}
```

`motivo_da_recusa` lê `error_code` de dentro de
`ConnectionError::ConnectionClosed` e compara contra
`TransportErrorCode::crypto(116)` e `[40, 42, 48].map(TransportErrorCode::crypto)`
— os números do registro de alertas TLS, RFC 8446 §6, exatamente como a
revisão apontou. `motivo_do_erro_de_envio`/`motivo_do_erro_de_leitura`
recolhem o `ConnectionError` de dentro de `WriteError::ConnectionLost` e
`ReadExactError::ReadError(ReadError::ConnectionLost(_))`, os dois outros
lugares de onde a recusa pode emergir agora que a checagem é uma troca real de
bytes.

### 4 · `atender` diz de quem é a recusa

```rust
let chegando = ponta.accept().await?;
let remoto = chegando.remote_address(); // lido antes do `.await` que segue
let conexao = match chegando.await {
    Ok(conexao) => conexao,
    Err(erro) => {
        tracing::warn!(par = %remoto, %erro, "uma ligação que chegou não fechou o aperto de mão");
        return None;
    }
};
```

### 5 · Os três avisos de `cargo doc`

`[`ConfereQuemChega`]` virou `` `ConfereQuemChega` `` nos três lugares que
`cargo doc -p seele-core --no-deps` apontava (a doc de `ErroDePar`, de
`passar_a_atender`, e a doc antiga da folga por relógio — removida junto com
o mecanismo).

## O que rodei

```
$ cargo test -p seele-core --lib par::
running 12 tests ... ok. 12 passed; 0 failed

$ cargo test -p seele-core
test result: ok. 263 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt -p seele-core -- --check
(nada)

$ cargo clippy -p seele-core --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.83s
(zero avisos)

$ cargo doc -p seele-core --no-deps
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.95s
6 avisos, os mesmos de sempre (bomba.rs, client.rs, conhecidos.rs, enlace.rs
×2, voice.rs). Nenhum em par.rs — os três que este round fechou não aparecem
mais.

$ cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.69s
```

Rodei `cargo test -p seele-core --lib par::` cinco vezes seguidas antes e
depois do conserto — sem flakiness em nenhuma das duas rodadas, e a suíte
ficou mais rápida (0,30 s contra 0,47 s antes), porque não há mais piso de
20 ms nem folga de RTT em toda discagem que fecha rápido.

## Reversões: a nova troca de byte prova o mesmo guarda que o relógio provava

Repeti as quatro reversões do round anterior (as três do brief mais a que
escrevi para `client_auth_mandatory`) e todas continuam derrubando exatamente
o teste que deveriam. Acrescentei uma quinta, específica deste round:

5. Neutralizar `confirmar_com_quem_atende` em `ligar` (fazendo-o sempre
   `Ok(())`, como a revisão fez com o mecanismo antigo): **as duas mesmas
   paredes caíram** —
   `quem_atende_recusa_quem_o_servidor_nao_apresentou` e
   `quem_atende_recusa_quem_nao_apresentou_certificado_nenhum`, e só elas
   (10 outros testes continuaram verdes). Desfeito.

## Preocupações que continuam, e uma nova

As cinco preocupações do relatório original continuam — nenhuma foi objeto
deste round, exceto a primeira (tabela global), que este round resolveu
integralmente removendo a tabela.

Uma preocupação nova, pequena: a troca de byte adiciona um RTT de aplicação a
**toda** discagem que fecha — inclusive o caso feliz —, onde antes (na versão
já revisada) havia só o custo de checar `close_reason()`/uma folga. É o custo
que a revisão já havia estimado como aceitável («custa menos do que parece»),
e a suíte local não tem como medir o efeito numa WAN de verdade; o roteiro de
duas máquinas da Task 10 é quem pode confirmar com dados.

---

# Task 5 — fix round 2/5

## Status

DONE.

## Commit

`c562e81` — "fix(par): atender ganha prazo, motivos são afirmados, e travar
não é silêncio", em `crates/seele-core/src/par.rs` (único arquivo tocado).

## Correção do round 1: «dois testes», não «três» — e agora sim

**Errei ao arredondar.** O relatório do round 1 disse que a reversão de
`confirmar_com_quem_atende` em `ligar` (Step 5, item 5) derrubava dois testes.
O re-revisor mediu um terceiro: `dois_pares_apresentados_se_ligam_pelos_dois_lados`
falhando com `Elapsed(())` — o `tokio::time::timeout(Duration::from_secs(2),
atendendo)` do próprio teste vencendo porque o lado que atende travou.

Antes de simplesmente aceitar o número, reproduzi: criei uma worktree no
commit exato do round 1 (`5c5fd8b`), apliquei a mesma reversão, e rodei
`cargo test -p seele-core --lib par::` **quinze vezes seguidas**. Em todas as
quinze, só os dois testes já relatados caíram — a terceira queda não se
repetiu no meu ambiente.

Isso não significa que o re-revisor mediu errado. Significa que a terceira
queda é uma corrida — e o mecanismo que a explica é exatamente o **achado 1
deste round**: no código do round 1, `atender` não tinha prazo próprio para
`confirmar_para_quem_ligou`. Sob a reversão, o lado que disca ainda executa a
troca de byte de verdade (só ignora o resultado dela); se o lado que atende
demorar mais que o normal para processar `accept_bi`/ler/escrever — por carga
da máquina, por como o executor do Tokio agenda naquele instante, por
qualquer razão que não deixa rastro — **nada dentro de `atender` o resgata**.
Ele fica à mercê de quem quer que o esteja aguardando por fora, e no teste
isso é o `tokio::time::timeout` de 2 s do próprio teste, não um prazo do
produto. Minha máquina, nas quinze rodadas, nunca demorou o bastante para
cruzar esse limiar; a do re-revisor, uma vez, demorou. Os dois resultados são
consistentes com o mesmo defeito.

Correção registrada: **a reversão do round 1 derruba pelo menos dois testes
de forma determinística, e um terceiro de forma intermitente, dependente de
quão devagar o lado que atende processa a confirmação naquela execução.** Um
relatório que só contasse os dois determinísticos estaria certo sobre eles e
incompleto sobre a causa raiz — e é essa causa raiz que os três achados deste
round fecham.

## Os três achados

### 1 · `atender` ganha prazo — Important, o achado central deste round

```rust
pub async fn atender(
    ponta: quinn::Endpoint,
    prazo_de_confirmacao: std::time::Duration,
) -> Option<ParLigado>
```

Só limita a espera **depois** do aperto de mão — a espera por alguém chegar
continua sem prazo, por desenho (`ponta.accept().await`). A justificativa do
número fica em comentário na própria função: sem prazo nenhum, `accept_bi`
ficaria preso ao `max_idle_timeout` do `quinn` (30 s nos padrões 0.11.11 deste
crate) se o par ficasse calado, e **indefinidamente** se ele mandasse
qualquer coisa para manter a conexão viva — e como `atender` serve uma vaga
só, isso nega a vaga inteira de quem empresta a subida.

`tokio::time::timeout(prazo_de_confirmacao, confirmar_para_quem_ligou(&conexao)).await`
substitui a chamada direta; o braço `Err(_elapsed)` loga `par = %remoto` (o
endereço, lido de `Incoming::remote_address()` antes do `.await` que pode
falhar — o mesmo padrão do achado 4 do round 1) e devolve `None`.

Os sete pontos de chamada de teste ganharam uma constante,
`PRAZO_DE_CONFIRMACAO_NO_TESTE = Duration::from_secs(2)`, com o motivo da
escolha em comentário (generoso para `127.0.0.1`, curto o bastante para não
prender a suíte).

**Teste novo:** `atender_nao_trava_para_sempre_com_um_par_que_aperta_a_mao_e_para`
disca manualmente com `config_de_cliente` + `connect_with` — contornando
`ligar`, que sempre confirma — completa o TLS, e nunca abre fluxo nenhum.
Prova por reversão: alargar o timeout interno para 1h faz o teste **FALHAR em
~2s** com `Elapsed`, porque o wrapper externo do teste (dessa vez,
deliberadamente maior que o prazo esperado) é quem apanha — exatamente o
comportamento que motivou o achado. Desfeito.

### 2 · Os dois `is_err()` viram `matches!` na variante exata

Medido antes de escrever a asserção (não suposto): rodei as duas discagens
com um `eprintln!` temporário no valor de `tentou`.

```
DIAG tentou = Err(RecusadoDepoisDeLigar(SemCertificado))
DIAG tentou = Err(RecusadoDepoisDeLigar(CertificadoErrado))
```

Confirmado — o intruso com identidade errada produz `CertificadoErrado`
(alerta TLS 40, `handshake_failure`, porque `ConfereQuemChega::verify_client_cert`
devolve `rustls::Error::General`, que o `rustls` mapeia para esse alerta na
ausência de um `Error::InvalidCertificate`/`PeerMisbehaved` mais específico —
conferido na fonte do `rustls` 0.23.43, `common_state.rs::send_cert_verify_error_alert`).
Quem nunca apresenta certificado nenhum produz `SemCertificado` (alerta 116,
`certificate_required`, de `server/tls13.rs::CertificateRequired`).

As duas asserções trocaram `is_err()` por
`matches!(tentou, Err(ErroDePar::RecusadoDepoisDeLigar(MotivoDaRecusa::<variante exata>)))`.
Removidos os `eprintln!` de diagnóstico. Provado por reversão: fazer
`motivo_da_recusa` sempre devolver `MotivoDaRecusa::Outro(erro.to_string())`
(um `return` antecipado, com o resto da função marcada `#[allow(unreachable_code)]`)
derruba **as duas, e só as duas** — 10 outros testes continuaram verdes.
Desfeito.

### 3 · `ligar` para de mentir `NaoAlcancou` para quem travou depois do TLS

`ErroDePar` ganhou `ConfirmacaoNaoChegouATempo` (sem campos, como
`NaoAlcancou`). Um `Arc<AtomicBool>` (`apertou_a_mao`), clonado para cada
tentativa, é marcado `true` assim que `ligando.await` fecha com sucesso —
antes de `confirmar_com_quem_atende`. No fim de `ligar`, se `ultimo` ainda for
o `NaoAlcancou` de largada e a marca estiver ligada, o motivo vira
`ConfirmacaoNaoChegouATempo`: só pode significar que uma tentativa completou
o TLS e foi abortada pelo prazo antes de a confirmação terminar, nunca
chegando a atualizar `ultimo` por conta própria.

**Teste novo:** `ligar_nao_diz_naoalcancou_para_quem_apertou_a_mao_e_travou`
sobe um "anfitrião" à mão (só `ponta.accept()` + `chegando.await`, sem
`atender` — nunca chama `accept_bi`) que aperta a mão e trava com
`std::future::pending::<()>().await`. `ligar`, com prazo de 300 ms, tem de
devolver o motivo novo. Provado por reversão: `if false && matches!(...)`
no lugar da checagem faz o teste voltar a `NaoAlcancou`, e só ele cai.
Desfeito.

## O que rodei

```
$ cargo test -p seele-core --lib par::
running 14 tests ... ok. 14 passed; 0 failed
(rodado 3× seguidas depois do conserto — sem flakiness)

$ cargo test -p seele-core
test result: ok. 265 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt -p seele-core -- --check
(nada)

$ cargo clippy -p seele-core --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.04s
(zero avisos)

$ cargo doc -p seele-core --no-deps
6 avisos, os mesmos de sempre, nenhum em par.rs

$ cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.56s
```

Mais a reprodução de quinze rodadas contra o commit `5c5fd8b` documentada
acima, na correção do round 1.

## Preocupações

As cinco do relatório original continuam (a primeira, resolvida no round 1).
Uma nova, pequena: `PRAZO_DE_CONFIRMACAO_NO_TESTE` e o prazo de 300 ms usado
no teste do achado 3 são números escolhidos para este ambiente de CI/local —
generosos para `127.0.0.1`, mas não testados sob a variação de agendamento
que aparentemente já produziu a queda intermitente que este round investigou.
Nenhum teste desta suíte mede isso; só a observação, honesta, de que ela
aconteceu pelo menos uma vez do lado do re-revisor e nunca do meu.
