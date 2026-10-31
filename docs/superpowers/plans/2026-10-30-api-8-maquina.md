# API 8, parte 3: a metade de máquina e o aceite do dono da máquina — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deixar a metade de servidor de um MOD pedir «rodar o servidor de jogo X na máquina de Fulano», mostrar esse pedido numa tela **do produto** na máquina de Fulano, e só depois do aceite dele carregar ali a metade de máquina do MOD, com o poder `porta` (`maquina.porta.servir`). O aceite prende o que foi mostrado (um resumo de 32 bytes); mudar qualquer coisa do que foi mostrado pede aceite novo. O dono da máquina desliga a qualquer momento, por um botão do produto. Servir túnel deixa de depender de um interruptor do servidor (R9) e passa a depender desse aceite — o que fecha também o «primeiro a pedir leva o nome».

**Architecture:** O protocolo 12 (ainda não publicado) ganha, no fim das listas, o pedido do servidor à máquina (`PedidoDeMaquina`, com uma `OfertaDeMaquina` que já carrega o que a parte 4 vai prender: artefatos, origens, comando, disco, termos), o aceite/recusa/parada da máquina, o pedido da metade de máquina à metade de servidor e o aviso de volta. O servidor guarda num livro (`seele-server/src/maquinas.rs`) quem foi chamado e quem aceitou cada servidor de jogo, e só aceita `ServirTunel` de quem aceitou, para os túneis que estavam na oferta. A metade de servidor ganha `mundo.maquinas.pedir/avisar` e três momentos. No núcleo, o `Enlace` recusa sozinho todo pedido de máquina (`SemTela`) a menos que quem o usa diga que tem tela — o app diz, o CLI e o navegador não —; o app guarda o aceite em disco (`seele-core/src/aceites_de_maquina.rs`), roda a metade de máquina num executor QuickJS próprio, sem janela, e a casca mostra a tela do aceite e uma faixa com DESLIGAR.

**Tech Stack:** Rust (`seele-proto`, `seele-server` com `rquickjs`, `seele-core`, `seele-ffi`, `seele-web`, `seele-sessao`, `seele-motor`, app Tauri), JavaScript da casca (`apps/seele-app/ui`), testes de texto em `apps/seele-app/tests/frontend.rs`, bancadas `.cjs`, conformância em `crates/seele-conformance`, `cargo xtask check-api`.

**Spec:** `docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md` — «A API 8» item 2 (`poderes`, `maquina`, a metade de máquina), item 4 (o aceite da máquina), o parágrafo do item 5 sobre a conta do usuário («avisar e isolar depois»), item 7 («ser dono da máquina não se compra com cargo»); «Como provar que funciona» (o aceite); «A ordem de entrega», passo 3. Terceiro de oito planos. Fica para depois: `programa.baixar`/`rodar` (parte 4, que consome o registro do aceite daqui), o MOD (parte 5), o `.mrpack` e `SeeleUI.salvar` (parte 6), a reserva (parte 7).

**Decisões herdadas:** `docs/superpowers/plans/2026-10-29-api-8-tunel-decisoes.md` (R9, R12 e as pendências T1–T9/Final).

## Desvios do pedido e decisões do plano

1. **O protocolo continua 12.** Conferido em 30/10 pelo `curl` do CLAUDE.md: v0.18.1 (`ae0f2715`), v0.18.2 (`13918258`) e v0.18.3 (`17554c28`) saem todas com `PROTOCOL_VERSION = 11`, e nenhuma carrega `api/v8.json`. O 12 nunca foi publicado; tudo daqui entra nele, no fim das listas. *Custo, se errado:* uma release com o 12 já fora obrigaria a subir para 13.
2. **O resumo do que foi mostrado é o portão.** `resumo_da_oferta(mod_id, hash_do_pacote, servidor, &oferta)` = SHA-256 do quadro do `seele-proto` (codificação do próprio fio, que é determinística) desses quatro. O hash do pacote entra: uma versão nova do MOD traz código novo de metade de máquina e pode trazer receita nova, então pede aceite novo. *Custo:* toda atualização do MOD pede aceite de novo à máquina.
3. **Um só caminho para o aceite novo e para o de sempre.** A metade de servidor manda `PedidoDeMaquina` sempre que quer a máquina (inclusive depois que ela reconecta). O app compara o resumo com o aceite guardado: igual, responde `AceiteDeMaquina` sem tela (a faixa mostra que a máquina está servindo); diferente, mostra a tela, com o que mudou.
4. **O servidor confia na escolha da metade de servidor, e só nela.** Só a pessoa a quem a metade de servidor mandou o pedido pode aceitar, e só com o mesmo resumo; só quem aceitou serve, e só os túneis da oferta (`nome`, `protocolo`, `porta`). Ninguém mais toma o nome (fim do «primeiro a pedir»). Um cliente modificado que aceita sem tela é escolha do dono daquela máquina sobre a própria máquina.
5. **O `seeled` sem tela não é máquina de jogo por construção:** ele não tem sessão de pessoa. O CLI e todo uso do `seele-core` sem tela recusam sozinhos (`MotivoDaRecusaDeMaquina::SemTela`); o navegador recusa com `SoNoApp`. A metade de servidor fica sabendo pelo momento `MaquinaRecusou`.
6. **`ServerConfig::servir_tuneis` some** (R9). O Task 3 acrescenta o portão do aceite ao lado do interruptor; o Task 7 tira o interruptor depois que a conformância passa pelo aceite de verdade.
7. **Teto de canais repassados por pessoa** (R12): `MAX_CANAIS_REPASSADOS_POR_PESSOA = 16` vivos ao mesmo tempo, por pessoa e por servidor. Acima, o canal é cortado, contado e dito a quem joga (`RecusaDeTunel::CanaisEsgotados`).
8. **As medidas do túnel na tela do MOD** (R12): entram do lado da máquina, porque cabem sem nada novo (`maquina.porta.quedas(nome)` lê `Enlace::quedas_do_tunel_servido`). Do lado de quem joga, ficam de novo para depois: pedem um evento novo de `SeeleUI` e uma decisão de frequência que é da tela do MOD (parte 5).
9. **Duas regras novas no manifesto:** `poderes` sem `maquina` → `PoderesSemMaquina` (um túnel que ninguém serve é o produto sabendo e não contando); `maquina` sem `server` → `MaquinaSemServidor` (a metade de máquina só conversa com a de servidor).
10. **A pasta do servidor de jogo** é `<config>/jogos/<chave-do-servidor>/<mod-id com / trocado por _>/<servidor>/`, criada só pela parte 4. Ela é dita no aceite («onde»). Fica dentro do diretório de configuração, mas nunca é ele: `..` e caminho absoluto já são recusados pela parte 4. *Custo:* um jogo lê o que a conta lê de qualquer jeito (Desvio 5 da spec, «avisar e isolar depois»).

## Global Constraints

- `PROTOCOL_VERSION` continua **12**. Variante nova só no fim de cada lista; campo novo em mensagem que existe, nunca (ADR 0058). Toda mensagem nova tem braço em `vocabulario::desde`/`desde_do_cliente` igual a 12, e só vai a quem negociou 12. `tests/fio_2_0.txt` só ganha linhas.
- Ordinais novos: `ClientMessage::AceiteDeMaquina` (50), `RecusaDeMaquina` (51), `MaquinaParada` (52), `PedidoDaMaquina` (53); `ServerMessage::PedidoDeMaquina` (59), `AvisoParaMaquina` (60), `MaquinaDispensada` (61). `RecusaDeTunel::ForaDoAceite` no fim de `RecusaDeTunel`.
- `api/v8.json` ainda é editável (nenhuma release a carrega). Toda mudança nela refaz a linha da v8 em `api/congeladas.sha256` (`shasum -a 256 api/v8.json`) e passa `cargo xtask check-api`.
- **Nada da metade de máquina roda antes do aceite.** O código dela nem é lido do pacote antes de o dono aceitar.
- **O aceite diz, com estas palavras:** «o servidor do jogo roda com a sua conta deste computador, e pode ler o que ela lê.» Sem caixa para desligar o aviso.
- **O dono desliga a qualquer momento:** DESLIGAR encerra a metade de máquina, para os túneis dela e avisa a metade de servidor (`MaquinaParou`). REVOGAR faz o mesmo e apaga o aceite guardado.
- **A metade de máquina não tem tela:** no executor dela não existem `SeeleUI` nem `SeeleMods`; existem `maquina` e `console`.
- Recusa é variante de enum com nome em kebab-case para a casca e para o MOD; nunca texto livre (`specs/02-protocolo.md`).
- «O produto sabe e não conta» (CLAUDE.md): toda recusa e toda parada tem motivo enumerado, vai para o log com `mod_id=` e `servidor=`, e chega a quem precisa: ao dono da máquina pela faixa, à metade de servidor pelos momentos, à metade de máquina pela recusa da promessa.
- Os testes rodam no Mac: nada atrás de `#![cfg(windows)]`.
- Provar cada guarda revertendo o conserto e vendo o teste falhar (CLAUDE.md, «Existir não é funcionar»); cada prova vai para o relatório do task e, no fim, para o arquivo de decisões.
- Comentários em português, o porquê e não o quê.

## Review Focus

1. **Sem aceite, nada serve:** `ServirTunel` de quem não aceitou, ou de quem aceitou outro servidor de jogo, ou para um túnel fora da oferta, ou com outra porta/protocolo, é `RecusaDeTunel::ForaDoAceite`. Testes no Task 3 (`servir_sem_aceite_e_fora_do_aceite`, `outra_pessoa_nao_toma_o_nome`) e no Task 7.
2. **O aceite é de quem foi chamado, com o resumo mostrado:** um `AceiteDeMaquina` de outra pessoa, ou com outro resumo, ou sem pedido pendente, não vale. Teste no Task 3 (`so_quem_foi_chamado_aceita_e_so_o_que_foi_mostrado`).
3. **Sem tela, recusa:** um `Enlace` sem `aceita_pedidos_de_maquina` responde `RecusaDeMaquina { SemTela }` sozinho, e o navegador `SoNoApp`. Testes no Task 6 e no Task 7.
4. **Mudar o que foi mostrado pede aceite novo:** o app não aceita sozinho um pedido cujo resumo difere do guardado, e a tela diz o que mudou. Testes no Task 8 (`um_resumo_diferente_nao_se_aceita_sozinho`) e no Task 9 (bancada, caso 5).
5. **Desligar desliga:** DESLIGAR para a metade de máquina, os túneis caem com `MaquinaParou` para quem joga, e a metade de servidor recebe `MaquinaParou { motivo: "pelo-dono" }`. Testes no Task 7 e no Task 9.
6. **A metade de máquina não sai do que foi aceito:** `maquina.porta.servir` de um túnel fora da oferta é recusado no app, antes do fio, com `fora-do-aceite`. Teste no Task 8.

---

### Task 1: O vocabulário da máquina no protocolo 12

**Files:**
- Create: `crates/seele-proto/src/maquina.rs` (e `pub mod maquina;` em `crates/seele-proto/src/lib.rs`)
- Modify: `crates/seele-proto/src/control.rs` (`ClientMessage` depois de `TunelFalhou` :2010, `ServerMessage` depois de `TunelRetirado` :2916, os `Validate`, o teste `o_ultimo_verbo_de_cada_lista_esta_onde_esta_versao_o_deixou` :5581 e o parágrafo do 12 :5689), `crates/seele-proto/src/vocabulario.rs` (`desde` :83, `desde_do_cliente` :176), `crates/seele-proto/src/tunel.rs` (`RecusaDeTunel::ForaDoAceite` no fim), `crates/seele-proto/src/version.rs` (o parágrafo «12 desde 29/10/2026» ganha a máquina)
- Test: `crates/seele-proto/tests/fio_2_0.rs` + `.txt`, `crates/seele-proto/tests/sementes_do_fuzz.rs` + `fuzz/sementes/control_frame/`, os módulos de teste de `control.rs`, `vocabulario.rs`, `maquina.rs`

**Interfaces:**
- Produces, em `seele_proto::maquina`:
  - `pub const MAX_SERVIDOR: usize = 32;` e `pub fn servidor_valido(s: &str) -> bool` (`[a-z0-9-]`, não vazio, como `tunel::nome_valido`).
  - `pub enum AlgoritmoDeHash { Sha1, Sha256, Sha512 }` e `pub struct HashDeArtefato { pub algoritmo: AlgoritmoDeHash, pub hex: String }` (hex minúsculo, 40/64/128 caracteres conforme o algoritmo).
  - `pub struct TunelOferecido { pub nome: String, pub protocolo: ProtocoloDeTunel, pub porta: u16 }`
  - `pub struct ArtefatoOferecido { pub nome: String, pub url: String, pub tamanho: u64, pub hash: HashDeArtefato }`
  - `pub struct ComandoOferecido { pub comando: String, pub argumentos: Vec<String> }`
  - `pub struct TermoOferecido { pub titulo: String, pub url: String }`
  - `pub struct OfertaDeMaquina { pub titulo: String, pub receita: String, pub versao_da_receita: String, pub versao_do_jogo: String, pub tuneis: Vec<TunelOferecido>, pub artefatos: Vec<ArtefatoOferecido>, pub pack: Option<HashDeArtefato>, pub origens: Vec<String>, pub dominios_do_jogo: Vec<String>, pub rodar: Option<ComandoOferecido>, pub disco_maximo: u64, pub termos: Vec<TermoOferecido> }` — com `Validate`.
  - Tetos: `MAX_TITULO = 64`, `MAX_TUNEIS_NA_OFERTA = 8`, `MAX_ARTEFATOS = 16`, `MAX_ORIGENS = 16`, `MAX_DOMINIOS_DO_JOGO = 16`, `MAX_ARGUMENTOS = 64`, `MAX_TERMOS = 4`, `MAX_TEXTO = 256` (cada string), `MAX_URL = 512`, `MAX_DISCO = 64 GiB`.
  - `pub fn resumo_da_oferta(mod_id: &str, hash_do_pacote: &str, servidor: &str, oferta: &OfertaDeMaquina) -> [u8; 32]` e `pub fn resumo_em_hex(&[u8; 32]) -> String`.
  - `pub enum MotivoDaRecusaDeMaquina { PeloDono, SemTela, SoNoApp, PacoteDiferente, OfertaInvalida }`
  - `pub enum MotivoDaParadaDeMaquina { PeloDono, Revogada, AppFechou, MetadeFalhou }`
  - `pub enum MotivoDaDispensa { ModDesligado, OutroPedido, SemAceite }`
  - `pub fn nome_da_recusa(&MotivoDaRecusaDeMaquina) -> &'static str` etc. (kebab-case: `pelo-dono`, `sem-tela`, `so-no-app`, `pacote-diferente`, `oferta-invalida`, `revogada`, `app-fechou`, `metade-falhou`, `mod-desligado`, `outro-pedido`, `sem-aceite`).
- Produces, no fio:
  - `ClientMessage::AceiteDeMaquina { mod_id: String, servidor: String, resumo: String }` (50) — `resumo` em hex de 64.
  - `ClientMessage::RecusaDeMaquina { mod_id, servidor, motivo: MotivoDaRecusaDeMaquina }` (51)
  - `ClientMessage::MaquinaParada { mod_id, servidor, motivo: MotivoDaParadaDeMaquina }` (52)
  - `ClientMessage::PedidoDaMaquina { request: u32, mod_id, servidor, payload: String }` (53) — responde-se com o `ModReply` de sempre; mesmo teto de `payload` que `ModRequest`.
  - `ServerMessage::PedidoDeMaquina { mod_id, hash: String, servidor, pedido_por: PersonId, oferta: OfertaDeMaquina }` (59) — `hash` é o do pacote ligado (`e_hash_de_conteudo`).
  - `ServerMessage::AvisoParaMaquina { mod_id, servidor, carga: String }` (60) — mesmo teto do `ModAviso`.
  - `ServerMessage::MaquinaDispensada { mod_id, servidor, motivo: MotivoDaDispensa }` (61)
  - `RecusaDeTunel::ForaDoAceite` (atravessa o fio; vai **depois** de `SemResposta`, o último hoje). O `Validate` continua recusando ao decodificar só as três locais.

- [ ] **Step 1: Escrever os testes que falham**

No teste dos ordinais de `control.rs`: `ultima_variante::<ClientMessage>() == 53`, `ultima_variante::<ServerMessage>() == 61`, e a linha do histórico do 12 ganha «e a máquina: o pedido, o aceite e a parada (30/10)». Em `vocabulario.rs`, um teste `a_maquina_nasceu_no_doze` que confere `desde`/`desde_do_cliente` = 12 para as sete. Em `maquina.rs`:

```rust
#[test]
fn o_resumo_muda_com_cada_coisa_mostrada() {
    let base = oferta_de_exemplo();
    let r = resumo_da_oferta("x/jogo", HASH_A, "mc-1", &base);
    assert_eq!(r, resumo_da_oferta("x/jogo", HASH_A, "mc-1", &base.clone()));
    // um caso por campo: titulo, receita, versao_da_receita, versao_do_jogo, tuneis (porta),
    // artefatos (hash), pack, origens, dominios_do_jogo, rodar (um argumento), disco_maximo,
    // termos; e o hash do pacote, o mod_id e o servidor — cada um muda o resumo.
}

#[test]
fn a_oferta_fora_dos_tetos_e_recusada() { /* um caso por teto; servidor "MC_1"; hex de sha1 com 64 caracteres; url sem https:// */ }
```

Em `fio_2_0.rs`, um exemplar por variante nova (a oferta com todos os campos preenchidos) e `os_enums_aninhados_nao_mudam_de_ordem` ganha os três motivos, `AlgoritmoDeHash` e `RecusaDeTunel` com a variante nova.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-proto`
Expected: FAIL (não compila)

- [ ] **Step 3: Implementar**

`Validate` da oferta: `servidor_valido`, `id_de_mod_valido`, cada teto, `tunel::nome_valido` em cada túnel (sem nome repetido), `url` começando por `https://`, cada origem e domínio um nome de host sem esquema nem `/`, `hex` do tamanho certo. O doc de `OfertaDeMaquina` diz por que ela já carrega o que a parte 4 vai usar: campo novo em mensagem que existe nunca entra (ADR 0058), e o 12 sai com ela inteira.

- [ ] **Step 4: Regerar o ouro e as sementes, e ler o diff**

Run: `SEELE_REGERAR_FIO=1 cargo test -p seele-proto --test fio_2_0 && git diff --stat crates/seele-proto/tests/fio_2_0.txt && git diff crates/seele-proto/tests/fio_2_0.txt | grep '^-[^-]' | wc -l`
Expected: só acréscimos (a última contagem é 0).

Run: `SEELE_REGERAR_SEMENTES=1 cargo test -p seele-proto --test sementes_do_fuzz`
Expected: entram `servidor-pedido-de-maquina` e `cliente-aceite-de-maquina`.

- [ ] **Step 5: Rodar e ver passar**

Run: `cargo test -p seele-proto && cargo test -p seele-conformance --test a_api_dos_mods_nao_diverge`
Expected: PASS

- [ ] **Step 6: Provar o guarda**

Tire `oferta.disco_maximo` do que `resumo_da_oferta` codifica; `o_resumo_muda_com_cada_coisa_mostrada` tem de falhar no caso do disco. Volte.

- [ ] **Step 7: Commit**

```bash
git add crates/seele-proto fuzz/sementes
git commit -m "Add the machine request, acceptance and stop to protocol 12"
```

### Task 2: As regras novas do manifesto e as capacidades da metade de máquina

**Files:**
- Modify: `crates/seele-proto/src/mods.rs` (`Refused` :~787, `read_manifest` :975, `capacidades_de` :~261, uma função nova ao lado), `crates/seele-sessao/src/mods.rs:45` (`refusal_name`) e `ModInstalado` (:119, ganha `maquina: bool`), `apps/seele-app/ui/frases.js:711` (`RECUSAS_DE_PACOTE`), `api/v8.json` (`sentido.poderes`, `sentido.maquina`, `recusas`), `api/congeladas.sha256`
- Test: o módulo de testes de `mods.rs` (ao lado de `a_api_8_le_poderes_e_maquina` :1720), `crates/seele-conformance/tests/a_api_dos_mods_nao_diverge.rs`, `cargo xtask check-api`

**Interfaces:**
- Produces:
  - `Refused::PoderesSemMaquina` (`poderes-sem-maquina`) e `Refused::MaquinaSemServidor` (`maquina-sem-servidor`), conferidas depois de `PortaSemEntrarNoJogo`.
  - `pub fn capacidades_da_maquina(manifest: &Manifest) -> Vec<&'static str>`: `["maquina"]`, mais `"maquina.porta"` com `Poder::Porta`. **Nenhuma** capacidade de `capacidades_da_api` entra: a metade de máquina não tem tela. `Programa` não dá capacidade nesta parte (parte 4).
  - `ModInstalado::maquina: bool` (o manifesto declara a metade de máquina).
  - Em `api/v8.json`, dentro de `compatibilidade`: `"capacidades-da-maquina": { "base": ["maquina"], "porta": ["maquina.porta"] }`.

- [ ] **Step 1: Escrever os testes que falham**

```rust
#[test]
fn poderes_pedem_metade_de_maquina_e_ela_pede_a_de_servidor() {
    // "poderes": ["porta"] sem "maquina" -> PoderesSemMaquina
    // "maquina" com "poderes", sem "server" -> MaquinaSemServidor
    // o PORTA_COMPLETA (com server e maquina) continua lido
}

#[test]
fn a_metade_de_maquina_nao_ganha_capacidade_de_tela() {
    let m = read_manifest(PORTA_COMPLETA).unwrap();
    assert_eq!(capacidades_da_maquina(&m), vec!["maquina", "maquina.porta"]);
    for c in capacidades_da_api(8) { assert!(!capacidades_da_maquina(&m).contains(c)); }
}
```

Em `a_api_dos_mods_nao_diverge.rs`, **a pendência T3 da parte 2**: um teste `as_capacidades_por_poder_da_v8_sao_as_do_codigo` que lê `compatibilidade.capacidades-por-poder` e `capacidades-da-maquina` da `api/v8.json` e compara com `capacidades_de`/`capacidades_da_maquina` de um manifesto com cada poder.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-proto --lib mods:: && cargo test -p seele-conformance --test a_api_dos_mods_nao_diverge`
Expected: FAIL

- [ ] **Step 3: Implementar e espelhar**

Ajuste `PORTA_COMPLETA` e os manifestos de teste da parte 2 que declaram `porta` sem `maquina` (procure `"poderes"` em `crates/` e `apps/`): ganham `"maquina"` e `"server"`. Uma frase por nome novo em `RECUSAS_DE_PACOTE`, no tom das de `poderes`. Refaça a linha da v8 em `congeladas.sha256`.

- [ ] **Step 4: Rodar e ver passar**

Run: o comando do Step 2, e `cargo xtask check-api && cargo test -p seele-sessao && cargo test -p seele-server --lib && cargo test -p seele-app --test frontend`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

Mude, em `capacidades-por-poder` da `v8.json`, `"tunel"` para `"tunel2"` (sem refazer o hash): o teste novo tem de falhar com o nome. Volte. Comente a conferência de `PoderesSemMaquina`: o primeiro teste tem de falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-proto/src/mods.rs crates/seele-sessao crates/seele-conformance apps/seele-app/ui/frases.js api
# e os manifestos de teste ajustados no Step 3 (git status mostra quais)
git commit -m "Require a machine half for powers and a server half for the machine half, and name its capabilities"
```

### Task 3: O livro das máquinas no servidor, e servir só com aceite

**Files:**
- Create: `crates/seele-server/src/maquinas.rs` (e `pub mod maquinas;` em `lib.rs`)
- Modify: `crates/seele-server/src/server.rs` (`Server` ganha `maquinas: Mutex<Maquinas>`; `Event` ganha `Event::PedidoDeMaquina { para, .. }`, `Event::AvisoParaMaquina { para, .. }`, `Event::MaquinaDispensada { para, .. }` e `Event::Maquina(MudancaDeMaquina)`), `crates/seele-server/src/session.rs` (os braços dos quatro verbos novos ao lado de `ServirTunel` :2797; a saída da sessão; `translate` :6063 estreita os três eventos endereçados por `para`, como `SirvaTunelPara`), `crates/seele-server/src/tuneis.rs` (`atender_servir` :719 — o portão do aceite **ao lado** do `servir_tuneis`; `poderes_do_mod` :678), quem desliga um MOD (o mesmo lugar onde a parte 2 chama `mod_desligado` dos túneis)
- Test: o módulo de testes de `maquinas.rs`; `crates/seele-server/tests/maquina_no_servidor.rs` (novo, quadros crus no molde de `tests/tunel_no_servidor.rs`)

**Interfaces:**
- Consumes: Task 1 inteiro; `Tuneis` (parte 2).
- Produces:
  - `pub struct Maquinas`, com:
    - `pedir(&mut self, mod_id, hash, servidor, quem: PersonId, pedido_por: PersonId, oferta) -> Pedido` — guarda o pendente por `(mod_id, servidor)`; um pedido novo para o mesmo servidor de jogo **substitui** o pendente e, se havia uma máquina aceita que **não** é `quem`, devolve a dispensa dela (`MotivoDaDispensa::OutroPedido`). O mesmo `quem` com o mesmo resumo e já aceito não muda nada.
    - `aceitar(&mut self, quem, conexao: u64, mod_id, servidor, resumo_hex) -> Result<Aceita, MotivoDeAceiteInvalido>` — só se há pendente para esse par, de `quem`, com o mesmo resumo; aceito, o pendente sai e a máquina entra (`conexao` é a da sessão, para a sessão nova da mesma pessoa não herdar).
    - `recusar(..)`, `parar(&mut self, quem, conexao, mod_id, servidor) -> Option<Aceita>`, `saiu(&mut self, quem, conexao) -> Vec<(String, String)>`, `mod_desligado(&mut self, mod_id) -> Vec<(PersonId, String)>`.
    - `tunel_permitido(&self, quem, conexao, mod_id, nome, protocolo, porta) -> bool` — `true` só se `quem`/`conexao` é a máquina aceita de algum servidor de jogo daquele MOD cuja oferta tem um `TunelOferecido` igual.
    - `maquina_de(&self, quem, conexao, mod_id, servidor) -> bool`, para o `PedidoDaMaquina` (Task 5).
  - `pub enum MotivoDeAceiteInvalido { SemPedido, OutraPessoa, OutroResumo }` — vai para o log, e o aceite inválido não muda nada (o cliente não recebe resposta: quem aceita é quem foi chamado, e um aceite sem pedido não tem a quem responder).
  - `pub enum MudancaDeMaquina { Aceitou { mod_id, servidor, quem }, Recusou { mod_id, servidor, quem, motivo }, Parou { mod_id, servidor, quem, motivo: &'static str } }` — no barramento, para os momentos do Task 5. Os motivos de `Parou` são os de `MotivoDaParadaDeMaquina` mais `saiu`, `mod-desligado` e `outro-pedido`.
  - Tetos: `MAX_PEDIDOS_PENDENTES_POR_MOD = 16`, `MAX_MAQUINAS_POR_MOD = 8` (o mesmo `MAX_TUNEIS_POR_MOD` da parte 2). Acima, o pedido é recusado e registrado (o Task 5 o devolve à metade de servidor).
  - `RecusaDeTunel::ForaDoAceite` em `atender_servir` quando `servir_tuneis` está desligado **e** `tunel_permitido` é `false`. O teste de unidade e o de quadros crus usam `servir_tuneis = false`.

Regras: parar ou sair (sessão que cai) tira a máquina do livro **e** para os túneis que ela servia (o `Tuneis::parar` da parte 2, motivo `MaquinaParou` para quem estava dentro; `saiu` já é `MaquinaSaiu`). `MaquinaDispensada` vai à máquina antes de os túneis pararem. Para a ordem dos mutexes, siga o doc de `apontar_um_par` (`session.rs`): `maquinas` antes de `tuneis`, e nunca os dois com o banco aberto.

- [ ] **Step 1: Escrever os testes que falham**

Em `maquinas.rs`: `so_quem_foi_chamado_aceita_e_so_o_que_foi_mostrado` (outra pessoa → `OutraPessoa`; outro resumo → `OutroResumo`; sem pedido → `SemPedido`; o certo → aceito), `um_pedido_novo_a_outra_pessoa_dispensa_a_aceita`, `sair_tira_a_maquina_e_devolve_os_servidores`, `tunel_permitido_so_para_o_que_estava_na_oferta` (nome, protocolo e porta). Em `tests/maquina_no_servidor.rs`, com um MOD de teste ligado (`poderes: ["porta"]`, `maquina`, `server`) e um pendente posto pelo livro direto (o caminho da metade de servidor é do Task 5):

```rust
#[tokio::test]
async fn servir_sem_aceite_e_fora_do_aceite() { /* ServirTunel sem aceite -> TunelRecusado { ForaDoAceite } */ }

#[tokio::test]
async fn outra_pessoa_nao_toma_o_nome() {
    // A aceita "mc-1" com o túnel "mc" (Tcp, 25565); B, com 12 e no mesmo servidor, pede ServirTunel "mc" antes de A:
    // B recebe ForaDoAceite; A serve e recebe TunelConcedido.
}

#[tokio::test]
async fn parar_a_maquina_derruba_os_tuneis_dela() { /* A aceita e serve; C entra; A manda MaquinaParada { PeloDono }; C recebe TunelEncerrado { MaquinaParou } */ }

#[tokio::test]
async fn um_par_onze_nunca_recebe_pedido_de_maquina() { /* o livro manda um pedido a uma sessão 11: nenhum quadro chega */ }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server maquina`
Expected: FAIL

- [ ] **Step 3: Implementar**

O braço de `AceiteDeMaquina` publica `Event::Maquina(Aceitou)`; o de `RecusaDeMaquina`, `Recusou`; o de `MaquinaParada`, `Parou`. Todo aceite inválido vai ao log em WARN com `mod_id=`, `servidor=`, `quem=` e o motivo. Em `atender_servir`, o log da recusa diz qual das três coisas não bateu (não há máquina aceita; há, mas o túnel não estava na oferta; estava, mas com outra porta ou outro protocolo).

- [ ] **Step 4: Rodar e ver passar; rodar o crate inteiro**

Run: `cargo test -p seele-server && cargo test -p seele-conformance --test tunel_de_jogo --test tunel_pela_ffi`
Expected: PASS (a conformância ainda passa pelo `servir_tuneis = true` que ela liga)

- [ ] **Step 5: Provar o guarda**

Faça `tunel_permitido` ignorar a porta; `tunel_permitido_so_para_o_que_estava_na_oferta` tem de falhar. Volte. Faça `aceitar` não conferir `quem`; `so_quem_foi_chamado_aceita…` e `outra_pessoa_nao_toma_o_nome` têm de falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server
git commit -m "Keep a ledger of requested and accepted game machines and serve tunnels only from an accepted one"
```

### Task 4: O teto de canais repassados por pessoa

**Files:**
- Modify: `crates/seele-server/src/tuneis.rs` (`repassar` :1024, a constante nova ao lado de `MAX_TUNEIS_POR_SERVIDOR` :55, um contador legível ao lado de `canais_sem_concessao` :661)
- Test: o módulo de testes de `tuneis.rs`, `crates/seele-server/tests/tunel_no_servidor.rs`

**Interfaces:**
- Produces: `pub const MAX_CANAIS_REPASSADOS_POR_PESSOA: usize = 16;`, `Server::canais_acima_do_teto(&self) -> u64`. O canal acima do teto é cortado antes de abrir o par na conexão da máquina, e quem joga recebe `TunelRecusado { motivo: CanaisEsgotados }` (o caminho `RepasseCortado::aviso` que existe). O log diz `quem=`, `mod_id=`, `tunel=` e o teto.

O teto é por pessoa, somando todos os túneis, contando canais **vivos** (sai do total quando a cópia termina, por qualquer motivo). Ele protege o `MAX_CANAIS = 64` da conexão da máquina: sem ele, uma pessoa sozinha esgota os canais de todos (R12).

- [ ] **Step 1: Escrever os testes que falham**

```rust
#[tokio::test]
async fn uma_pessoa_nao_esgota_os_canais_da_maquina() {
    // quem joga abre 17 canais `t` pela estrela no mesmo túnel; o 17º é cortado, chega CanaisEsgotados,
    // canais_acima_do_teto() == 1; outra pessoa ainda abre um canal e ele chega à máquina;
    // fechado um dos 16, o próximo da primeira pessoa passa.
}
```

Isso também fecha a pendência T5 da parte 2 («não há prova de ponta a ponta de `CanaisEsgotados`»).

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server --test tunel_no_servidor canais`
Expected: FAIL

- [ ] **Step 3: Implementar**

Um `HashMap<PersonId, usize>` no `Server` (ou dentro de `Tuneis`), incrementado por um guarda RAII que decrementa no `Drop`, para nenhum caminho de saída de `repassar_t`/`repassar_j` esquecer.

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-server`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

Tire o decremento do `Drop`; a última metade do teste (o próximo passa depois de fechar um) tem de falhar. Volte. Tire o corte; o teste tem de falhar no 17º. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server
git commit -m "Cap the channels one person can have relayed through the host at once"
```

### Task 5: A metade de servidor chama a máquina, ouve o aceite e conversa com ela

**Files:**
- Modify: `crates/seele-server/src/mods/mod.rs` (`Portas` :449 ganha `maquinas: bool`; `montar` :641 põe `mundo.maquinas` quando `portas.maquinas`; `carregar_do_disco` :1112; `pedir` :928 ganha o contexto da máquina), `crates/seele-server/src/mods/pedidos.rs` (`Portas` a partir do manifesto :313; depois da chamada, o que `mundo.maquinas` declarou vai ao livro, como `avisador.avisar` :413; um `executar_da_maquina`), `crates/seele-server/src/mods/despacho.rs` (`momento_de` :547 ganha os três momentos; o mesmo despejo do declarado depois de um momento), `crates/seele-server/src/mods/aviso.rs` (o teto e a taxa do aviso valem para `AvisoParaMaquina`), `crates/seele-server/src/session.rs` (o braço de `PedidoDaMaquina`), `api/v8.json` (`world`, `moments`, `reads`, `sentido`, `limites`, `recusas`), `api/congeladas.sha256`, `api/README.md`
- Test: o módulo de testes de `mods/mod.rs`; `crates/seele-server/tests/maquina_no_servidor.rs` (do Task 3); `cargo xtask check-api`

**Interfaces:**
- Consumes: `Maquinas` (Task 3), `OfertaDeMaquina` e a validação (Task 1).
- Produces, no JS da metade de servidor, só para um MOD cujo manifesto tem `maquina`:
  - `mundo.maquinas.pedir({ quem, servidor, oferta })` — `quem` é o id numérico da pessoa; `oferta` é o objeto com os nomes de campo de `OfertaDeMaquina` (hash como `{ algoritmo: "sha256", hex }`; ausentes viram vazio/`null`/0). Validada na hora: inválida **lança** um erro com o nome da regra (`servidor-invalido`, `oferta-invalida:<campo>`), e o MOD vê no `console`. Declarativa: o pedido sai depois que a chamada termina, como `mundo.avisar`.
  - `mundo.maquinas.avisar({ servidor, carga })` — só à máquina aceita daquele servidor de jogo; sem máquina aceita, nada sai, e o retorno de `aoPedir`/`aoAcontecer` não muda, mas o log diz `sem-maquina` e o contador do MOD sobe (`Server::avisos_sem_maquina(mod_id)`).
  - Momentos, entregues **só** ao MOD do `mod_id`: `MaquinaAceitou { servidor, pessoa }`, `MaquinaRecusou { servidor, pessoa, motivo }`, `MaquinaParou { servidor, pessoa, motivo }` (os nomes kebab-case do Task 1/3).
  - `aoPedir(contexto, pedido)` para um `PedidoDaMaquina` aceito tem `contexto.maquina = { servidor }` e `contexto.person` = a pessoa da máquina. Um `PedidoDaMaquina` de quem não é a máquina aceita daquele servidor de jogo **não chama** o MOD: o servidor responde `MaquinaDispensada { motivo: SemAceite }`, e o log diz por quê.
  - Tetos: `MAX_PEDIDOS_DE_MAQUINA_POR_CHAMADA = 4` e `MAX_AVISOS_A_MAQUINA_POR_CHAMADA = 16`; acima, lança com o nome.

O `hash` do `PedidoDeMaquina` é o do pacote ligado agora (`persistence::mods::enabled`). `quem` que não está conectado com 12, ou que não existe, é recusado na hora do despejo e vira `MaquinaRecusou { motivo: "nao-esta-aqui" }` para o próprio MOD — a metade de servidor sempre fica sabendo.

- [ ] **Step 1: Escrever os testes que falham**

Em `mods/mod.rs`: `mundo_maquinas_so_existe_com_a_metade_de_maquina` (`typeof mundo.maquinas` é `undefined` sem `portas.maquinas`), `uma_oferta_invalida_lanca_com_o_nome`. Em `maquina_no_servidor.rs`, agora com o MOD de teste cujo `aoPedir` faz `mundo.maquinas.pedir({ quem: contexto.person, servidor: "mc-1", oferta })` e cujo `aoAcontecer` grava os momentos em `dados`:

```rust
#[tokio::test]
async fn a_metade_de_servidor_chama_e_ouve_o_aceite() {
    // ModRequest -> chega PedidoDeMaquina com o hash do pacote; AceiteDeMaquina com o resumo certo;
    // o MOD vê MaquinaAceitou { servidor: "mc-1", pessoa }; PedidoDaMaquina -> ModReply, e o MOD viu contexto.maquina.servidor == "mc-1".
}

#[tokio::test]
async fn quem_nao_e_a_maquina_nao_fala_como_maquina() { /* PedidoDaMaquina de outra pessoa -> MaquinaDispensada { SemAceite }, e o MOD não foi chamado */ }

#[tokio::test]
async fn o_aviso_vai_so_a_maquina_aceita() { /* mundo.maquinas.avisar chega a A como AvisoParaMaquina e não a B */ }

#[tokio::test]
async fn pedir_a_quem_nao_esta_aqui_volta_como_recusa() { /* quem = uma pessoa desconectada -> MaquinaRecusou { motivo: "nao-esta-aqui" } */ }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server maquina && cargo xtask check-api`
Expected: FAIL

- [ ] **Step 3: Implementar e escrever na `v8.json`**

`world`: `"mundo.maquinas.pedir"` e `"mundo.maquinas.avisar"` apontando para os símbolos Rust; `moments`: os três; `reads`: `contexto.maquina`; `limites`: os dois tetos; `sentido`: uma frase por entrada, com o nome da variante (o `check-api` reprova prosa sem símbolo). Refaça a linha da v8. Em `api/README.md`, a seção «A API 8» ganha um parágrafo da metade de máquina.

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p seele-server && cargo xtask check-api && cargo test -p seele-conformance --test a_api_dos_mods_nao_diverge --test a_api_publicada_nao_se_edita`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

No braço de `PedidoDaMaquina`, chame o MOD sem conferir `maquina_de`; `quem_nao_e_a_maquina_nao_fala_como_maquina` tem de falhar. Volte. Entregue os momentos a todos os MODs; um teste com dois MODs (o segundo grava todo momento que vê) tem de falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server api
git commit -m "Let the server half ask a member's machine to run a game server and talk to its machine half"
```

### Task 6: O núcleo responde ao pedido, guarda o aceite, e a FFI o expõe

**Files:**
- Create: `crates/seele-core/src/aceites_de_maquina.rs` (e `pub mod aceites_de_maquina;`)
- Modify: `crates/seele-core/src/enlace.rs` (`Aviso` :235 ganha `Aviso::PedidoDeMaquina`, `Aviso::AvisoParaMaquina`, `Aviso::MaquinaDispensada`; o despacho de `ServerMessage`; `Enlace` ganha os métodos), `crates/seele-ffi/src/lib.rs` (a `Connection` ganha os métodos e os eventos; `servir_tunel`/`parar_tunel`/`quedas_do_tunel_servido` chegam à FFI), `crates/seele-ffi/src/types.rs` (`OfertaDeMaquinaFfi` serializável, `Event::MachineRequest`, `Event::MachineNotice`, `Event::MachineDismissed`), `crates/seele-ffi/src/mods.rs` (wrappers do repositório, no molde de `aceite_de` :192), `crates/seele-web/src/ligacao.rs` (recusa `SoNoApp`), `crates/seele-sessao/src/state.rs` (os três braços novos: «dito e decidido»)
- Test: os módulos de teste de `aceites_de_maquina.rs`, `enlace.rs`, `seele-web/src/ligacao.rs`

**Interfaces:**
- Produces, em `seele_core::aceites_de_maquina`:
  - `pub struct AceiteDeMaquina { pub alvo: String, pub mod_id: String, pub hash: String, pub servidor: String, pub resumo: String, pub oferta: OfertaDeMaquina, pub pedido_por: String /* apelido */, pub quando: u64 /* unix */ }` — **o registro que a parte 4 lê**: `oferta.origens` e as URLs de `oferta.artefatos` são o que `programa.baixar` aceita; `oferta.rodar` é o que `programa.rodar` aceita; `oferta.disco_maximo` é o teto da pasta.
  - `pub struct AceitesDeMaquina` com `em(dir) -> Self`, `listar() -> io::Result<Vec<AceiteDeMaquina>>`, `de(alvo, mod_id, servidor) -> io::Result<Option<AceiteDeMaquina>>`, `guardar(&AceiteDeMaquina) -> io::Result<()>` (substitui o de mesma chave), `revogar(alvo, mod_id, servidor) -> io::Result<bool>`.
  - `pub fn pasta_do_servidor_de_jogo(dir_de_config, alvo, mod_id, servidor) -> PathBuf` (Desvio 10; não cria nada).
  - Arquivo `<config>/maquinas`, uma linha JSON por aceite; escrita atômica (arquivo temporário + `rename`); linha ilegível é pulada **e contada** (`listar` devolve as boas, e `ilegiveis()` o número, que o app põe no log).
- Produces, no `Enlace`:
  - `Enlace::aceitar_pedidos_de_maquina(&self, sim: bool)` — padrão `false`: sem isso, todo `PedidoDeMaquina` é respondido na hora com `RecusaDeMaquina { SemTela }` e logado (Desvio 5). Com `true`, vira `Aviso::PedidoDeMaquina { mod_id, hash, servidor, pedido_por, oferta, resumo }` (o resumo já calculado aqui, com `resumo_da_oferta`).
  - `Enlace::aceitar_maquina(&self, mod_id, servidor, resumo) -> Result<(), Fechado>`, `recusar_maquina(.., motivo)`, `parar_maquina(.., motivo)`.
  - `Enlace::pedido_da_maquina(&self, mod_id, servidor, payload) -> Result<String, FalhaDoPedidoDaMaquina>` — espera o `ModReply` (o mesmo contador de `request` de `mod_request`) ou `MaquinaDispensada { SemAceite }` → `FalhaDoPedidoDaMaquina::SemAceite`; prazo de 15 s como o da janela → `SemResposta`.
  - `Aviso::AvisoParaMaquina { mod_id, servidor, carga }` e `Aviso::MaquinaDispensada { mod_id, servidor, motivo }`.
- Produces, na FFI: `Connection::{aceitar_pedidos_de_maquina, aceitar_maquina, recusar_maquina, parar_maquina, pedido_da_maquina, servir_tunel, parar_tunel, quedas_do_tunel_servido}` e os três eventos; `seele_ffi::mods::{aceites_de_maquina, aceite_de_maquina, guardar_aceite_de_maquina, revogar_aceite_de_maquina}` com a chave `chave_do_servidor(alvo)`.
- No navegador (`seele-web`): `PedidoDeMaquina` é respondido com `RecusaDeMaquina { SoNoApp }`, sem chegar à casca, e logado.

- [ ] **Step 1: Escrever os testes que falham**

Em `aceites_de_maquina.rs`: `guardar_e_ler_de_volta`, `guardar_de_novo_substitui`, `revogar_apaga_so_aquele`, `uma_linha_ilegivel_e_pulada_e_contada`, `a_pasta_do_jogo_nunca_e_a_de_configuracao` (não é igual a `dir`, e `identity.key` não está nela). Em `enlace.rs` (com o servidor de mentira dos testes de lá): `sem_tela_o_pedido_e_recusado_sozinho` e `com_tela_o_pedido_vira_aviso_com_o_resumo`. Em `ligacao.rs`: `o_navegador_recusa_ser_maquina`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-core maquina && cargo test -p seele-web maquina`
Expected: FAIL

- [ ] **Step 3: Implementar**

O resumo que o `Aviso` leva é o que o app compara e o que vai no `AceiteDeMaquina`; ninguém mais o recalcula. `seele-sessao/state.rs` trata as três mensagens como «dito e decidido» (o estado da sessão não muda com elas).

- [ ] **Step 4: Rodar e ver passar; os crates inteiros**

Run: `cargo test -p seele-core -p seele-ffi -p seele-web -p seele-sessao`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

Troque o padrão de `aceitar_pedidos_de_maquina` para `true`; `sem_tela_o_pedido_e_recusado_sozinho` tem de falhar. Volte. Tire o `rename` (escreva direto no arquivo final) — não há teste de queda no meio; anote no relatório que a atomicidade não é provada por reversão, e por quê.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-core crates/seele-ffi crates/seele-web crates/seele-sessao
git commit -m "Answer machine requests in the core, refuse them without a screen, and keep the owner's acceptance on disk"
```

### Task 7: A prova de ponta a ponta, e o fim do interruptor

**Files:**
- Create: `crates/seele-conformance/tests/aceite_da_maquina.rs`
- Modify: `crates/seele-conformance/tests/tunel_de_jogo.rs` e `tunel_pela_ffi.rs` (passam pelo aceite; o MOD de teste ganha `server` e `maquina`), `crates/seele-server/src/lib.rs` (sai `ServerConfig::servir_tuneis` :317-369), `crates/seele-server/src/server.rs:1595`, `crates/seele-server/src/tuneis.rs:726`, `crates/seele-server/src/session.rs:7048`, `crates/seele-server/src/mods/pedidos.rs:722`, `crates/seele-server/tests/tunel_no_servidor.rs` (:74-112, :440 — o teste «servir é provisório» vira «servir sem aceite é recusado»)
- Test: os próprios

**Interfaces:**
- Consumes: Tasks 1–6. Um ajudante de teste, em `crates/seele-conformance/tests/` (módulo comum que os três arquivos usam): `async fn aceitar_como_maquina(server, maquina: &Enlace, mod_id, servidor) -> Resumo` — liga `aceitar_pedidos_de_maquina(true)`, faz o `mod_request` que leva a metade de servidor a pedir, espera o `Aviso::PedidoDeMaquina` e responde `aceitar_maquina` com o resumo dele.

- [ ] **Step 1: Escrever os testes**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn sem_aceite_a_maquina_nao_serve() { /* servir_tunel antes do aceite -> Err(ForaDoAceite); nenhuma porta do eco foi tocada */ }

#[tokio::test(flavor = "multi_thread")]
async fn com_aceite_o_eco_passa() { /* aceitar_como_maquina; servir_tunel "eco-tcp"; quem joga entra; 64 KiB voltam iguais */ }

#[tokio::test(flavor = "multi_thread")]
async fn um_enlace_sem_tela_recusa_e_a_metade_de_servidor_sabe() { /* máquina sem aceitar_pedidos_de_maquina; o MOD grava MaquinaRecusou { motivo: "sem-tela" } em dados, lido por um mod_request */ }

#[tokio::test(flavor = "multi_thread")]
async fn parar_a_maquina_fecha_a_porta_de_quem_joga_e_avisa_o_mod() { /* parar_maquina(PeloDono): quem joga vê a queda MaquinaParou; o MOD grava MaquinaParou { motivo: "pelo-dono" } */ }

#[tokio::test(flavor = "multi_thread")]
async fn a_maquina_que_cai_e_volta_nao_serve_sem_aceite_novo() { /* a sessão da máquina cai; outra sessão da mesma pessoa tenta servir -> ForaDoAceite; o MOD vê MaquinaParou { motivo: "saiu" } */ }
```

- [ ] **Step 2: Rodar**

Run: `cargo test -p seele-conformance --test aceite_da_maquina`
Expected: PASS. Um que falhe é defeito dos Tasks 3–6: volte lá, com teste de unidade.

- [ ] **Step 3: Tirar o interruptor**

Remova `ServerConfig::servir_tuneis`, `Server::servir_tuneis`, o portão em `atender_servir` e todo `servir_tuneis: true` (procure com `grep -rn servir_tuneis crates apps`, que tem de voltar vazio). Os testes de `tunel_de_jogo.rs` e `tunel_pela_ffi.rs` passam a chamar `aceitar_como_maquina` antes de servir; os de quadros crus de `tunel_no_servidor.rs` põem o aceite pelo livro (`Maquinas::pedir` + `aceitar`).

- [ ] **Step 4: Rodar tudo do túnel**

Run: `cargo test -p seele-server && for i in 1 2; do cargo test -p seele-conformance --test aceite_da_maquina --test tunel_de_jogo --test tunel_pela_ffi || break; done`
Expected: PASS nas duas.

- [ ] **Step 5: Provar o guarda**

Em `atender_servir`, devolva `Ok` sem consultar `tunel_permitido`; `sem_aceite_a_maquina_nao_serve` e `a_maquina_que_cai_e_volta…` têm de falhar. Volte. No `seele-server`, chame `Maquinas::saiu` sem parar os túneis; `parar_a_maquina…` não pega isso (é `parar`) — então tire a parada de túneis de `parar` e veja `parar_a_maquina…` falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-conformance crates/seele-server
git commit -m "Prove the machine acceptance end to end and replace the servir_tuneis switch with it"
```

### Task 8: A metade de máquina no app

**Files:**
- Create: `apps/seele-app/src/maquina.rs` (o ciclo de vida das metades de máquina; `mod maquina;` em `main.rs`)
- Modify: `crates/seele-motor/src/preludio.rs` (com a capacidade `maquina`: o global `maquina`, e **nenhum** `SeeleUI`/`SeeleMods`), `crates/seele-motor/src/lib.rs` (se o prelúdio precisar de um ponto de entrada para `aoAviso`/`aoParar`), `apps/seele-app/src/main.rs` (comandos; o `ExitRequested` :14653 para as metades de máquina com `AppFechou`; ligar `aceitar_pedidos_de_maquina(true)` em toda conexão do app; o despacho dos eventos `MachineRequest`/`MachineNotice`/`MachineDismissed`), `apps/seele-app/src/executor.rs` (se o executor precisar de um modo sem janela)
- Test: os testes de `apps/seele-app/src/maquina.rs` e de `executor.rs`; um teste do prelúdio em `seele-motor`

**Interfaces:**
- Consumes: `capacidades_da_maquina` (Task 2), `AceitesDeMaquina` e os métodos da FFI (Task 6), `ler_codigo_do_mod` (`main.rs:~9812`, que passa a saber ler o arquivo `manifesto.maquina`).
- Produces, no JS da metade de máquina (só existe depois do aceite; uma instância por `(alvo, mod_id, servidor)`):
  - `maquina.servidor` (string) e `maquina.oferta` (o objeto aceito, congelado).
  - `maquina.pedir(valor) -> Promise<string>` — `PedidoDaMaquina`; rejeita com `sem-aceite` ou `sem-resposta`.
  - `maquina.aoAviso(fn)` — `fn(carga)` para cada `AvisoParaMaquina` daquele servidor de jogo.
  - `maquina.aoParar(fn)` — chamada uma vez antes de a instância acabar, com o motivo (`pelo-dono`, `revogada`, `app-fechou`, `mod-desligado`, `outro-pedido`, `sem-aceite`, `servidor-saiu`); o executor espera até 2 s.
  - Só com `maquina.porta`: `maquina.porta.servir({ nome, protocolo, porta }) -> Promise<void>`, `maquina.porta.parar(nome) -> Promise<void>`, `maquina.porta.quedas(nome) -> Promise<Array<[string, number]>>` (Desvio 8). `servir` de um túnel que não está em `maquina.oferta.tuneis` com a mesma porta e protocolo rejeita com `fora-do-aceite` **no app**, sem ir ao fio; as recusas do servidor chegam com os nomes kebab-case de `RecusaDeTunel`.
- Produces, comandos Tauri:
  - `maquina_aceitar({ alvo, mod_id, servidor, resumo })` — guarda o aceite (com a oferta e o hash do pedido que o app guardou ao receber o evento), responde `AceiteDeMaquina`, lê o código da metade de máquina do pacote **agora** e liga a instância. Um `resumo` que não é o do pedido pendente é recusado com `pedido-velho`.
  - `maquina_recusar({ alvo, mod_id, servidor })` → `RecusaDeMaquina { PeloDono }`.
  - `maquina_desligar({ alvo, mod_id, servidor })` → `aoParar("pelo-dono")`, para os túneis que essa instância serve, encerra a instância, `MaquinaParada { PeloDono }`. O aceite guardado fica.
  - `maquina_revogar({ alvo, mod_id, servidor })` → o mesmo com `Revogada`, e apaga o aceite.
  - `maquinas_aceitas() -> [{ alvo, mod_id, servidor, titulo, pedido_por, quando, ligada: bool, tuneis: [{ nome, protocolo, porta }] }]`.
  - Eventos para a casca: `maquina-pedido` `{ alvo, mod_id, mod_nome, servidor, pedido_por, oferta, resumo, pasta, mudou: string[] | null }` (`mudou` = nomes dos campos da oferta que diferem do aceite guardado, ou `null` se não há aceite) e `maquina-estado` `{ alvo, mod_id, servidor, ligada, motivo? }`.
- Regras no app:
  - `pedido_por` chega como `PersonId`; o app o troca pelo apelido da lista de pessoas da sessão (sem apelido: «alguém deste servidor»), e é o apelido que vai no evento e no aceite guardado.
  - Ao receber `MachineRequest`: com aceite guardado de mesmo `resumo` **e** mesmo `hash`, responde `aceitar_maquina` sozinho, liga a instância e emite `maquina-estado { ligada: true }` (Desvio 3). Senão, guarda o pedido pendente e emite `maquina-pedido`.
  - `MachineDismissed` e a saída do servidor encerram a instância com o motivo, sem apagar o aceite.
  - Um erro não tratado na metade de máquina (o executor morre, ou a promessa de topo rejeita) encerra a instância com `MetadeFalhou`, manda `MaquinaParada { MetadeFalhou }` e emite `maquina-estado { ligada: false, motivo: "metade-falhou" }` com o texto do erro no log.

- [ ] **Step 1: Escrever os testes que falham**

Em `seele-motor`: `a_metade_de_maquina_nao_tem_tela` (com `["maquina"]`, `typeof SeeleUI === 'undefined'`, `typeof SeeleMods === 'undefined'`, `typeof maquina.pedir === 'function'`, `typeof maquina.porta === 'undefined'`; com `["maquina", "maquina.porta"]`, `typeof maquina.porta.servir === 'function'`). Em `apps/seele-app/src/maquina.rs` (lógica pura, sem Tauri):

```rust
#[test]
fn um_resumo_diferente_nao_se_aceita_sozinho() { /* aceite guardado com resumo A; pedido com resumo B -> Decisao::Mostrar { mudou: ["versao_do_jogo"] } */ }

#[test]
fn o_mesmo_resumo_com_outro_pacote_tambem_pede_tela() { /* mesmo resumo guardado mas hash diferente -> Mostrar */ }

#[test]
fn servir_fora_da_oferta_e_recusado_antes_do_fio() { /* conferir_servir(&oferta, "mc", Tcp, 25566) -> Err("fora-do-aceite") */ }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-motor maquina && cargo test -p seele-app maquina`
Expected: FAIL

- [ ] **Step 3: Implementar**

`maquina.rs` separa a decisão (pura, testável: `decidir(pedido, guardado) -> Decisao`, `conferir_servir`, `campos_que_mudaram(&OfertaDeMaquina, &OfertaDeMaquina) -> Vec<&'static str>`) do efeito (executor, FFI, eventos). O executor da metade de máquina é um `ExecutorQuickJs` com `capacidades_da_maquina`, cujas mensagens de saída vão a uma bomba Rust (não à janela) que trata `maquina-pedir`, `porta-servir`, `porta-parar`, `porta-quedas` e recusa todo outro tipo com `tipo-desconhecido` no log.

- [ ] **Step 4: Rodar e ver passar; o app inteiro**

Run: `cargo test -p seele-motor && cargo test -p seele-app`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

Faça `decidir` olhar só o `servidor` (e não o resumo); `um_resumo_diferente…` tem de falhar. Volte. Tire `conferir_servir` do caminho de `porta-servir`; um teste da bomba (`a_bomba_recusa_servir_fora_da_oferta`) tem de falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-motor apps/seele-app/src
git commit -m "Run a MOD's machine half on the app only after the owner accepts, within what was shown"
```

### Task 9: A tela do aceite, a faixa com DESLIGAR, e a bancada

**Files:**
- Create: `apps/seele-app/ui/camada-maquina.js` + `camada-maquina.css`, `apps/seele-app/bancada/aceite-da-maquina.cjs` (no molde de `bancada/tunel-de-jogador.cjs`)
- Modify: `apps/seele-app/ui/frases.js` (um bloco `FRASES_DA_MAQUINA` depois de `FRASES_DO_TUNEL` :1691), `apps/seele-app/ui/index.html`, `apps/seele-app/ui/ponte-web.js` (os comandos `maquina_*` recusam com `so-no-app`), a lista de aceites nas configurações (onde a casca lista hoje os aceites de MODs; siga `esquecer_aceite_de_mods`), `.github/workflows/ci.yml` (ao lado de :603), `empacotar/publicar.sh` (ao lado de :1451)
- Test: `apps/seele-app/tests/frontend.rs`, a bancada nova, o guarda `toda_bancada_de_navegador_e_a_do_celular_rodam_no_ci_e_no_publicar`

**Interfaces:**
- Consumes: os comandos e eventos do Task 8.
- Produces: a tela do produto (fora de toda superfície de MOD; nada dela passa por `mods-regiao.js`), a faixa de máquinas ligadas, e a lista REVOGAR.

**O texto** (em `FRASES_DA_MAQUINA`, no tom de `FRASES_DO_TUNEL`; maiúsculas só no título e nos botões, como a casca faz):

```js
const FRASES_DA_MAQUINA = Object.freeze({
  titulo: (quem) => `${quem} PEDE PARA RODAR UM SERVIDOR DE JOGO NESTE COMPUTADOR`,
  mod: (nome, id) => `MOD: ${nome} (${id})`,
  jogo: (titulo, versao) => versao ? `jogo: ${titulo}, versão ${versao}` : `jogo: ${titulo}`,
  receita: (nome, versao) => `receita: ${nome} ${versao}`,
  pedidoPor: (quem, servidor) => `pedido por ${quem}, no servidor ${servidor}`,
  porta: (porta, protocolo) => `o jogo escuta na porta ${porta} (${protocolo}); quem está no servidor alcança essa porta pelo SEELE`,
  baixar: (n, total, origens) => `vai baixar ${n === 1 ? "1 arquivo" : `${n} arquivos`}, ${total} no total, de ${origens}`,
  nadaABaixar: "nada para baixar neste pedido",
  rodar: (linha) => `vai rodar: ${linha}`,
  nadaARodar: "nenhum programa: este pedido só abre a porta do jogo",
  disco: (teto, pasta) => `ocupa até ${teto} de disco, em ${pasta}`,
  oJogoBaixaSozinho: (dominios) => `o jogo baixa por conta própria de ${dominios}, sem o SEELE conferir`,
  suaConta: "o servidor do jogo roda com a sua conta deste computador, e pode ler o que ela lê.",
  termo: (titulo) => `li e aceito: ${titulo}`,
  quemAceitaEVoce: "quem aceita os termos do jogo é você, e não o SEELE nem o MOD",
  podeDesligar: "você desliga quando quiser, pela faixa no alto da tela",
  mudou: (campos) => `isto mudou desde o seu aceite: ${campos}. Por isso o SEELE pergunta de novo.`,
  aceitar: "ACEITAR E LIGAR",
  recusar: "RECUSAR",
  faixa: (titulo, mod) => `este computador serve ${titulo} (${mod})`,
  desligar: "DESLIGAR",
  revogar: "REVOGAR",
  paradas: Object.freeze({ "pelo-dono": "desligado por você", "revogada": "aceite revogado por você", "app-fechou": "o app fechou",
    "metade-falhou": "a metade de máquina do MOD falhou; veja o registro", "mod-desligado": "o MOD foi desligado no servidor",
    "outro-pedido": "o servidor passou este jogo para outra máquina", "sem-aceite": "o servidor não reconhece mais o seu aceite",
    "servidor-saiu": "você saiu do servidor" }),
  campos: Object.freeze({ titulo: "o nome do jogo", receita: "a receita", versao_da_receita: "a versão da receita",
    versao_do_jogo: "a versão do jogo", tuneis: "as portas", artefatos: "o que se baixa", pack: "o pack",
    origens: "de onde se baixa", dominios_do_jogo: "de onde o jogo baixa sozinho", rodar: "o programa e os argumentos",
    disco_maximo: "o teto de disco", termos: "os termos", hash: "a versão do MOD" }),
});
```

Regras: ACEITAR fica desligado até todo termo ter a caixa marcada; o aviso `suaConta` é sempre visível, sem rolar, em destaque; a tela não fecha por fora (Esc é RECUSAR, e a pessoa vê). Mais de um pedido ao mesmo tempo vira fila, um por vez. A faixa tem uma linha por máquina ligada, com DESLIGAR, e a última parada com o motivo em frase (`paradas`).

- [ ] **Step 1: Escrever os testes que falham**

Em `frontend.rs`: `FRASES_DA_MAQUINA.suaConta` tem o texto exato da Global Constraint; `camada-maquina.js` não importa nada de `mods-regiao.js`; `ponte-web.js` recusa `maquina_aceitar`. A bancada, com a casca no app e uma ponte de mentira:

1. `maquina-pedido` sem termos → a tela mostra título, MOD, jogo, porta, «nada para baixar», «nenhum programa», disco com a pasta, e `suaConta`; ACEITAR chama `maquina_aceitar` com o `resumo` do evento;
2. com dois termos → ACEITAR desligado até as duas caixas;
3. RECUSAR (e Esc) → `maquina_recusar`, e nada de `maquina_aceitar`;
4. `maquina-estado { ligada: true }` → a faixa mostra a linha; DESLIGAR → `maquina_desligar` com as três chaves; `maquina-estado { ligada: false, motivo: "pelo-dono" }` → a linha diz «desligado por você»;
5. `maquina-pedido` com `mudou: ["versao_do_jogo", "rodar"]` → a tela diz «isto mudou desde o seu aceite: a versão do jogo, o programa e os argumentos»;
6. dois pedidos seguidos → um por vez, o segundo depois de responder o primeiro;
7. na lista de aceites, REVOGAR → `maquina_revogar`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-app --test frontend maquina && node apps/seele-app/bancada/aceite-da-maquina.cjs`
Expected: FAIL

- [ ] **Step 3: Implementar e ligar a bancada no CI e no `publicar.sh`**

Em `ci.yml`, ao lado da do túnel:

```yaml
      # O aceite da máquina (API 8, parte 3): a tela do produto, o aviso da conta,
      # os termos que a pessoa marca, e o DESLIGAR.
      - run: node apps/seele-app/bancada/aceite-da-maquina.cjs
        if: ${{ !cancelled() }}
        timeout-minutes: 5
```

Em `publicar.sh`, dentro de `todas_as_bancadas_de_navegador()`: `uma_bancada node apps/seele-app/bancada/aceite-da-maquina.cjs`.

- [ ] **Step 4: Rodar e ver passar**

Run: o comando do Step 2, e `cargo test -p seele-app --test frontend`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

Deixe ACEITAR sempre ligado; o caso 2 tem de falhar. Volte. Tire `suaConta` da tela; o caso 1 tem de falhar. Volte. Tire a linha do `publicar.sh`; o guarda das bancadas tem de falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add apps/seele-app .github/workflows/ci.yml empacotar/publicar.sh
git commit -m "Show the machine owner what will run before accepting, and let them turn it off at any time"
```

### Task 10: O guia do autor

**Files:**
- Modify: `docs/como-se-faz-um-mod.md` (uma seção nova «## A metade de máquina» depois de «## A metade de servidor» :854, antes de «## No celular e no navegador» :1047; `mod.json` :35 ganha `plataformas`, `poderes`, `maquina`; «As capacidades»/«O que existe lá dentro» :133 ganha `SeeleUI.tunel`; «`mundo`» :964 ganha `mundo.maquinas`; «`aoAcontecer`» :907 ganha os três momentos; «No celular e no navegador» :1047 diz o que `plataformas: ["app"]` faz)
- Test: `cargo xtask check-api` (se ele confere o guia), e uma leitura de cada exemplo contra a `api/v8.json`

**O que o guia diz** (o porquê antes do como, no tom do guia):
- `plataformas`, `poderes` (`porta`, e `programa` reservado à próxima parte), `maquina`, e as regras de recusa, com os nomes que a tela mostra.
- O fluxo inteiro, com código: a metade de servidor chama `mundo.maquinas.pedir` com a oferta; o dono vê a tela do produto (descrever o que ela mostra e que o MOD não a desenha); `MaquinaAceitou`; a metade de máquina sobe e chama `maquina.porta.servir`; quem joga clica e a metade de cliente chama `SeeleUI.tunel.entrar(nome)` dentro do `botao`; DESLIGAR e `MaquinaParou`.
- «O que o aceite prende»: a tabela de campos de `OfertaDeMaquina` e o que muda o resumo; o que não muda (a configuração do jogo, que vira arquivo). Que uma versão nova do MOD pede aceite de novo (Desvio 2).
- Os tetos e as recusas, por nome; o que a metade de máquina **não** tem (`SeeleUI`, `SeeleMods`, `dados`, `mundo`).
- «Avisar e isolar depois»: o jogo roda com a conta da pessoa, e o aceite diz isso.

- [ ] **Step 1: Escrever**

- [ ] **Step 2: Conferir cada nome contra o código**

Run: `for n in mundo.maquinas.pedir mundo.maquinas.avisar maquina.porta.servir maquina.porta.parar maquina.porta.quedas maquina.pedir maquina.aoAviso maquina.aoParar SeeleUI.tunel.entrar MaquinaAceitou MaquinaRecusou MaquinaParou fora-do-aceite poderes-sem-maquina maquina-sem-servidor; do grep -q "$n" docs/como-se-faz-um-mod.md || echo "falta no guia: $n"; grep -rq "${n##*.}" api/v8.json crates apps/seele-app/ui || echo "falta no código: $n"; done`
Expected: nenhuma linha.

- [ ] **Step 3: Commit**

```bash
git add docs/como-se-faz-um-mod.md
git commit -m "Document platforms, powers, the machine half and the player's tunnel in the MOD author guide"
```

---

## Depois dos dez

- **O que a parte 4 consome:** `AceiteDeMaquina` em disco (Task 6), `pasta_do_servidor_de_jogo`, `OfertaDeMaquina::{artefatos, pack, origens, rodar, disco_maximo}` e `HashDeArtefato` (Task 1), a capacidade que ela vai somar a `capacidades_da_maquina` para `Poder::Programa`, e o `aoParar` da metade de máquina (Task 8), que é onde o processo do jogo vai ser encerrado.
- **O indexador** (`SEELE-MODS-INDEXER`) precisa aceitar `maquina` e as regras novas antes de um MOD da 8 com elas ser publicado. Fora deste plano.
- **Não medido aqui, e dito como não medido:** a tela entre duas casas reais. A bancada prova o texto e os botões; a conformância prova o mecanismo numa máquina só.
