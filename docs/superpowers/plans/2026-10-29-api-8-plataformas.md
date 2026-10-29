# API 8, parte 1: a abertura da API e `plataformas` — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Abrir a API 8 na linha 2 e deixar um MOD declarar em que plataformas ele roda, de modo que quem entra pelo navegador não baixe nem carregue um MOD `["app"]`, e a tela diga por quê.

**Architecture:** O manifesto ganha `plataformas` (só na API 8), lido em `seele-proto`. O servidor guarda a marca numa coluna nova de `mods` (migração 20, o padrão das colunas `api` e `tema`) e a manda no JSON do catálogo que a janela já pede (`pedidos.rs`), sem tocar no fio. A casca (`base.js`), que é a mesma no app e no navegador, pula o MOD que não roda na plataforma dela e o marca com um estado novo.

**Tech Stack:** Rust (`seele-proto`, `seele-server`, `xtask`), SQLite (`rusqlite`), JavaScript da casca (`apps/seele-app/ui`), testes de texto em `apps/seele-app/tests/frontend.rs` e bancadas `.cjs`.

**Spec:** `docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md` (seção «A API 8», item 1, e «A ordem de entrega», passo 1). Este é o primeiro de oito planos; os próximos (túnel, metade de máquina e aceite, `programa`, o MOD, `.mrpack`, reserva, segunda receita) saem um por vez depois deste.

## Global Constraints

- A API 8 é a 7 inteira, mais o que segue; ela entra na linha 2: `APIS_POR_LINHA` passa a `("2", &[8, 7, 6])`.
- Nenhum campo novo em mensagem que já existe no fio (ADR 0058). `PROTOCOL_VERSION` não muda neste plano.
- Valores de `plataformas`: `"app"` e `"navegador"`. O celular é a casca web, então conta como `"navegador"`.
- Ausente quer dizer as duas. Valor desconhecido é recusado (serde, `Malformed`).
- A metade de cliente de um MOD fora da plataforma **não é baixada**, e a tela diz «só funciona no app» (ou «só funciona no navegador»).
- Recusa é variante de `Refused`, com os números dentro; nenhuma string livre chega à casca (`specs/02-protocolo.md`).
- Os comentários seguem o idioma do repositório: português, o porquê e não o quê.
- Provar cada guarda revertendo o conserto e vendo o teste falhar (CLAUDE.md, «Existir não é funcionar»).

## Review Focus

1. **Host antigo (0.18) sem o campo no catálogo:** um navegador novo que não recebe `plataformas` trata o MOD como das duas plataformas, e o carrega como hoje. Teste no Task 4.
2. **`"plataformas": []`:** recusado como `PlataformasVazias`, e não instalado como um MOD que não roda em lugar nenhum. Teste no Task 1.
3. **`["app", "app"]`:** recusado como `PlataformaRepetida`. Teste no Task 1.
4. **Re-habilitar o MOD numa versão que muda a marca:** a coluna é regravada no mesmo ato que grava o hash, e o catálogo passa a dizer a marca nova. Teste no Task 3.
5. **MOD `["app"]` no navegador:** não conta como ausente, não dispara «sem-pacote» nem a busca do que falta, e o aceite do conjunto continua sendo feito normalmente. Teste no Task 4.

---

### Task 1: A API 8 e `plataformas` no manifesto

**Files:**
- Modify: `crates/seele-proto/src/mods.rs` (`MOD_API_VERSION` :60, `APIS_POR_LINHA` :128, `capacidades_da_api` :162, `Manifest` :436, `Refused` :518, `read_manifest` :706, testes no fim do arquivo)
- Test: `crates/seele-proto/src/mods.rs` (módulo de testes existente, ao lado de `a_api_6_roda_sem_permissoes_e_a_7_as_le`)

**Interfaces:**
- Produces:
  - `pub enum Plataforma { App, Navegador }` com `#[serde(rename_all = "lowercase")]`, `Deserialize`, `Serialize`, `Clone, Copy, Debug, PartialEq, Eq`.
  - `impl Plataforma { pub const fn nome(self) -> &'static str }` → `"app"` / `"navegador"`.
  - `Manifest::plataformas: Vec<Plataforma>` (`#[serde(default)]`; vazio sem a chave).
  - `pub fn plataformas_efetivas(manifest: &Manifest) -> Vec<Plataforma>` → `[App, Navegador]` quando a lista está vazia, senão a lista.
  - `pub const PRIMEIRA_API_COM_PLATAFORMAS: u32 = 8;`
  - `Refused::PlataformasPedemApiNova { api: u32, exige: u32 }`, `Refused::PlataformasVazias`, `Refused::PlataformaRepetida { posicao: usize }`.
  - `MOD_API_VERSION = 8`; `APIS_POR_LINHA = [("2", &[8, 7, 6]), ("1", &[6]), ("0.15", &[5, 4, 3])]`; `capacidades_da_api(8)` = a lista da 7, igual, na mesma ordem.

`plataformas` vazio por falta da chave e `"plataformas": []` escrito são casos diferentes: o segundo é recusado. Use `Option<Vec<Plataforma>>` por dentro, ou um `deserialize_with`, para distinguir; o campo público pode continuar `Vec` normalizado depois da conferência.

- [ ] **Step 1: Escrever os testes que falham**

```rust
#[test]
fn a_api_8_le_plataformas_e_a_7_continua_rodando() {
    let oito = read_manifest(&manifesto_com(8, r#" "plataformas": ["app"],"#)).expect("8");
    assert_eq!(oito.plataformas, vec![Plataforma::App]);
    assert_eq!(plataformas_efetivas(&oito), vec![Plataforma::App]);
    let sem = read_manifest(&manifesto_com(8, "")).expect("8 sem a chave");
    assert_eq!(plataformas_efetivas(&sem), vec![Plataforma::App, Plataforma::Navegador]);
    assert!(read_manifest(&manifesto_com(7, "")).is_ok());
    assert_eq!(APIS_ACEITAS, &[8, 7, 6]);
    assert_eq!(capacidades_da_api(8), capacidades_da_api(7));
}

#[test]
fn plataformas_sem_a_api_8_sao_recusadas() {
    assert_eq!(
        read_manifest(&manifesto_com(7, r#" "plataformas": ["app"],"#)),
        Err(Refused::PlataformasPedemApiNova { api: 7, exige: 8 })
    );
}

#[test]
fn plataformas_vazias_repetidas_ou_desconhecidas_sao_recusadas() {
    assert_eq!(read_manifest(&manifesto_com(8, r#" "plataformas": [],"#)), Err(Refused::PlataformasVazias));
    assert_eq!(
        read_manifest(&manifesto_com(8, r#" "plataformas": ["app", "app"],"#)),
        Err(Refused::PlataformaRepetida { posicao: 1 })
    );
    assert!(matches!(
        read_manifest(&manifesto_com(8, r#" "plataformas": ["celular"],"#)),
        Err(Refused::Malformed { .. })
    ));
}
```

`manifesto_com(api, trecho)` é o ajudante que os testes de `permissoes` já usam para montar um manifesto (veja em volta de `:1392`). Se ele tiver outro nome, use o que existe.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-proto --lib mods::`
Expected: FAIL (não compila: `Plataforma`, `plataformas_efetivas` e as variantes não existem)

- [ ] **Step 3: Implementar em `crates/seele-proto/src/mods.rs`**

Em `read_manifest`, depois da conferência de `permissoes`: a ordem é API (`PlataformasPedemApiNova`), depois vazia, depois repetida. Atualize os comentários de `APIS_ACEITAS`/`APIS_POR_LINHA` com o parágrafo da 8 («desde a 0.19… a linha 2 roda a 8, a 7 e a 6»), no estilo dos que estão lá.

- [ ] **Step 4: Rodar e ver passar; rodar o crate inteiro**

Run: `cargo test -p seele-proto`
Expected: PASS. Testes que afirmavam `APIS_ACEITAS == [7, 6]` ou `MOD_API_VERSION == 7` falham: atualize-os para 8, um por um, lendo o que cada um protege. `manifesto_da_0_16.rs` não muda (a 0.16 continua recusando).

- [ ] **Step 5: Provar o guarda**

Comente a conferência de `PlataformasVazias` em `read_manifest`; rode o teste de vazias; ele tem de falhar. Volte o código.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-proto/src/mods.rs
git commit -m "Open MOD API 8 with platform declaration in the manifest"
```

### Task 2: O contrato da v8 e os espelhos

**Files:**
- Create: `api/v8.json`
- Modify: `api/README.md` (seção nova «A API 8», depois de «A API 7 (0.17)»), `apps/seele-app/ui/base.js:3390` (`CAPACIDADES_POR_API`), `apps/seele-app/testes/api-do-indexador.json`
- Test: `cargo xtask check-api`, `crates/seele-conformance/tests/a_api_dos_mods_nao_diverge.rs`, `apps/seele-app/tests/frontend.rs`

**Interfaces:**
- Consumes: `MOD_API_VERSION = 8`, `capacidades_da_api(8)`, `Manifest::plataformas` (Task 1).

- [ ] **Step 1: Rodar os guardas e ver falhar**

Run: `cargo xtask check-api && cargo test -p seele-conformance --test a_api_dos_mods_nao_diverge && cargo test -p seele-app --test frontend`
Expected: FAIL. Anote a mensagem de cada um: elas dizem o que falta (o arquivo da v8, a entrada 8 do espelho, a API 8 no indexador).

- [ ] **Step 2: Escrever `api/v8.json`**

Copie `api/v7.json` e escreva a v8 por inteiro, sem `extends` (a regra da v6 e da v7). Mude: `version` 8; `compatibilidade.aceitas` `[8, 7, 6]`; `capacidades-por-versao` com a chave `"8"` igual à `"7"`; `manifesto` com `"plataformas": "Plataforma"` ao lado de `"permissoes"`; no bloco `sentido`, a frase de `plataformas` (valores, ausente = as duas, exige a 8, o celular é navegador). Todo apontador novo é símbolo do código (`Plataforma`, `plataformas_efetivas`), porque o `check-api` reprova prosa.

- [ ] **Step 3: Espelhos**

`CAPACIDADES_POR_API[8]` em `base.js` igual ao `[7]`. `api-do-indexador.json` com a 8, no formato das entradas que já estão lá. Em `api/README.md`, a seção «A API 8»: o que entra (`plataformas`), por que na linha 2 (ADR 0058, acréscimo), e o que a 0.18 faz com um manifesto da 8 (`ApiTooNew` sem a chave, `Malformed` com ela). Não mexa em `congeladas.sha256` para a v8: ela só congela quando sair numa release.

- [ ] **Step 4: Rodar os guardas e ver passar**

Run: o mesmo comando do Step 1
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add api/v8.json api/README.md apps/seele-app/ui/base.js apps/seele-app/testes/api-do-indexador.json
git commit -m "Write the API 8 contract and mirror it in the shell and indexer fixture"
```

O indexador de verdade mora no repositório `SEELE-MODS-INDEXER` e precisa aceitar a 8 antes de um MOD da 8 ser publicado. Isso fica fora deste plano; anote no fim do trabalho.

### Task 3: O servidor guarda a marca e a manda no catálogo

**Files:**
- Modify: `crates/seele-server/src/persistence/schema.rs` (migração 20, depois da 19 em :789), `crates/seele-server/src/persistence/mods.rs` (`EnabledMod` :29, `enable` :75, `definir_conjunto` :122, `enabled` :177, `por_preencher` :217, `preencher_do_pacote` :237), `crates/seele-server/src/mods/mod.rs:1231-1245` (a subida que preenche), quem monta `EnabledMod` ao habilitar (siga as chamadas de `enable`), `crates/seele-server/src/mods/pedidos.rs:212`
- Test: `crates/seele-server/src/persistence/mods.rs` (testes ao lado de `preencher_do_pacote`, :535), `crates/seele-server/tests/convite_e_mods.rs`

**Interfaces:**
- Consumes: `seele_proto::mods::{Plataforma, plataformas_efetivas}` (Task 1).
- Produces:
  - Migração 20: `ALTER TABLE mods ADD COLUMN plataformas TEXT;` — NULL é «não preenchida», como a `tema` da 15/16.
  - `EnabledMod::plataformas: Option<Vec<Plataforma>>` (`None` = não preenchida).
  - `preencher_do_pacote(persistence, id, hash, api, tema, plataformas: &[Plataforma]) -> Result<bool>`: preenche só a coluna nula, e `por_preencher` também devolve as linhas com `plataformas IS NULL`.
  - No catálogo (`id` vazio), cada MOD leva `"plataformas": ["app", "navegador"]` — sempre a lista efetiva, nunca ausente; `None` vira as duas.

- [ ] **Step 1: Escrever os testes que falham**

Em `persistence/mods.rs`:

```rust
#[test]
fn a_marca_de_plataformas_e_gravada_com_o_hash_e_preenchida_na_subida() {
    // habilitar com Some([App]) e ler de volta Some([App]);
    // uma linha com a coluna nula aparece em por_preencher;
    // preencher_do_pacote(.., &[Plataforma::App]) a preenche e devolve true;
    // a segunda chamada não sobrescreve e devolve false;
    // re-habilitar com outro hash e Some([App, Navegador]) regrava a coluna.
}
```

Em `tests/convite_e_mods.rs`, um teste que habilita um MOD `["app"]` e um sem a chave, pede o catálogo (`id` vazio) e afirma:

```rust
assert_eq!(catalogo["mods"][0]["plataformas"], serde_json::json!(["app"]));
assert_eq!(catalogo["mods"][1]["plataformas"], serde_json::json!(["app", "navegador"]));
```

Siga os ajudantes que esse arquivo já usa para montar pacote e servidor.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-server plataformas`
Expected: FAIL (não compila)

- [ ] **Step 3: Implementar**

A migração 20 com comentário no estilo da 14 e da 16 (coluna e não leitura do disco no aperto de mão; NULL quer dizer não preenchida). Serialize a coluna como o JSON da lista (`["app"]`). O teste de migrações em `schema.rs:808-829` cobra descrição e SQL não vazios.

- [ ] **Step 4: Rodar e ver passar; rodar o crate inteiro**

Run: `cargo test -p seele-server`
Expected: PASS. Ajuste os testes que constroem `EnabledMod` à mão com `plataformas: None`.

- [ ] **Step 5: Provar o guarda**

Troque, no catálogo, a lista efetiva pela lista crua (que fica ausente para o MOD sem a chave); o teste do catálogo tem de falhar. Volte.

- [ ] **Step 6: Commit**

```bash
git add crates/seele-server
git commit -m "Store each MOD's platforms and send them in the catalogue"
```

### Task 4: A casca pula o MOD que não roda aqui, e diz por quê

**Files:**
- Modify: `apps/seele-app/ui/base.js` (`carregarMods` :4426, o laço de `ausentes` :4491, o laço de carga :4544), `apps/seele-app/ui/camada-mods.js` (o desenho do estado do MOD, perto de :689 e :729)
- Create: `apps/seele-app/bancada/plataformas-dos-mods.cjs` (no molde de `bancada/cargos-no-navegador.cjs`)
- Test: `apps/seele-app/tests/frontend.rs`, a bancada nova

**Interfaces:**
- Consumes: `catalogo.mods[i].plataformas: string[] | undefined` (Task 3); `invoke("plataforma")` responde `"web"` no navegador (`ponte-web.js:2116`) e `"macos"`/`"windows"`/`"outro"` no app.
- Produces:
  - `function plataformaDaCasca(respostaDoInvoke)` → `"navegador"` para `"web"`, `"app"` para o resto.
  - `function rodaAqui(modDoCatalogo, aqui)` → `true` se `plataformas` falta (host antigo) ou inclui `aqui`.
  - Estado novo do MOD, `"so-noutra-plataforma"`, com o detalhe `"só funciona no app"` ou `"só funciona no navegador"`, gravado por `anotarEstadoDoMod`.

- [ ] **Step 1: Escrever os testes que falham**

Em `frontend.rs`, no estilo dos testes de `carregarMods` (:17011):
- `carregarMods` consulta `rodaAqui` **antes** de montar `ausentes` (a busca do que falta não pode ver o MOD de outra plataforma);
- o laço de carga pula o MOD cujo id foi marcado `"so-noutra-plataforma"`;
- `camada-mods.js` desenha o detalhe desse estado, e não o de «sem-pacote».

Na bancada nova, com a casca no navegador e um catálogo falso:
- MOD `["app"]`: nenhum pedido de download, estado `"so-noutra-plataforma"`, texto «só funciona no app» na tela de MODs;
- MOD sem `plataformas` (host 0.18): carregado como hoje;
- MOD `["app", "navegador"]`: carregado;
- o `AceitarMods` do conjunto acontece igual nos três casos.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p seele-app --test frontend plataforma` e `node apps/seele-app/bancada/plataformas-dos-mods.cjs`
Expected: FAIL

- [ ] **Step 3: Implementar**

Pergunte a plataforma uma vez por conferência, junto do `snapshot`. Filtre `catalogo.mods` em dois grupos antes de calcular `ausentes`; o grupo de fora só ganha o estado e o `modsExigidos.set` (ele continua exigido pelo servidor, e a gestão tem de dizer isso). Um MOD já carregado cuja marca o host trocou para fora desta plataforma é descarregado pelo mesmo caminho do «saiu da exigência».

- [ ] **Step 4: Rodar e ver passar**

Run: os dois comandos do Step 2, e depois `cargo test -p seele-app`
Expected: PASS

- [ ] **Step 5: Provar o guarda**

Faça `rodaAqui` devolver sempre `true`; a bancada tem de falhar no caso `["app"]`. Volte.

- [ ] **Step 6: Commit**

```bash
git add apps/seele-app/ui/base.js apps/seele-app/ui/camada-mods.js apps/seele-app/bancada/plataformas-dos-mods.cjs apps/seele-app/tests/frontend.rs
git commit -m "Skip MODs that do not run on this shell and say which platform they need"
```

### Task 5: O guia do autor e a medida dos MODs oficiais no navegador

**Files:**
- Modify: `docs/como-se-faz-um-mod.md` (o cabeçalho diz «Este guia é da API 6»: atualize para a 8; a tabela do `mod.json` ganha `plataformas`; a seção «No celular e no navegador» ganha um parágrafo)
- Create: `docs/evidencias/api-8/mods-oficiais-no-navegador.md`

- [ ] **Step 1: Medir**

Rode os três MODs oficiais (aparência, perfis, mesa) na casca do navegador, com `apps/seele-app/bancada/mods-oficiais-na-ui.cjs` contra o alvo web, se ele o aceitar; senão, à mão, num `seeled` local com o navegador. Para cada um: carregou ou não, e o que não funcionou, com a saída.

- [ ] **Step 2: Registrar**

No arquivo de evidências: data, commit, comando, resultado por MOD. O que não foi medido é dito como não medido. Nenhum MOD oficial ganha `plataformas` neste plano; a evidência é o que decide isso depois.

- [ ] **Step 3: Atualizar o guia**

`plataformas` na tabela, com os valores, o «ausente = as duas», «exige a API 8» e «o celular é navegador». A recusa de vazia e de repetida, com os nomes das variantes.

- [ ] **Step 4: Commit**

```bash
git add docs/como-se-faz-um-mod.md docs/evidencias/api-8
git commit -m "Document platforms for MOD authors and record official MODs in the browser"
```
