# Final fix wave: whole-branch review of «API 8, parte 3»

Branch `desenho/api-8-partes`, base HEAD `958a30dc`. Nothing pushed or merged.

## Per item

### 1. Clippy red: `PedidoDeMaquina` too large
- **Change:** `ServerMessage::PedidoDeMaquina.oferta` is now `Box<OfertaDeMaquina>`, and `#[allow(clippy::large_enum_variant)]` is gone. Construction and destructuring sites updated in proto (control/vocabulario tests, `fio_2_0`, `sementes_do_fuzz`), core `enlace.rs` (the `Aviso` was already a Box, so it now passes the box through), server `session.rs`, the server test `maquina_no_servidor`, and the web `ligacao.rs` test.
- **Wire unchanged:** `cargo test -p seele-proto --test fio_2_0` gives 5 passed with no regeneration (postcard encodes `Box<T>` the same as `T`).
- **CI clippy (`cargo clippy --workspace --all-targets --all-features -- -D warnings`):** clean. Getting there needed more fixes:
  - **Introduced on this branch, after base:** `a_api_dos_mods_nao_diverge.rs` indexing (×4) in `fbc451c5`. Now uses `get` along a path, so a missing key fails with its name.
  - **Older than base 92d678ba, fixed because they blocked the CI command:**
    - `apps/seele-app/tests/frontend.rs:17140` `unwrap` (commit 100b9f3f8).
    - `apps/seele-app/src/main.rs:9044, 9053` `[0]` (commit 69df6873a).
    - `apps/seele-app/src/main.rs:15703` `v["poderes"]` (commit ca38bc098).
- The per-crate clippy (`-p seele-proto -p seele-server -p seele-core -p seele-ffi -p seele-web -p seele-app --all-targets -- -D warnings`) is clean too.

### 2. An invalid `AceiteDeMaquina` got no answer
- **Server (`session.rs`):** any `Err` from `Maquinas::aceitar` now replies `MaquinaDispensada { SemAceite }` to the accepting connection (12 only, via `entende_a_mensagem`), logged in WARN.
  - One exception: an accept that repeats what already holds on this connection (same person, connection and digest; new `Maquinas::ja_aceita_igual`). Dismissing it would kill a half the ledger says is valid. It is logged as "repetido".
- **App (`maquina.rs`):** the new `Maquinas::dispensar` handles both server replies:
  - `sem-aceite` stops only running instances (`ligadas`) and never touches `pendentes`. This applies to the reply to an accept and to the existing reply to `PedidoDaMaquina`.
  - The JS side only closes a dialog when `resumo` is present, so it is untouched.
- **`ligar` reordered:**
  1. `Viva::subir`.
  2. Register in `ligadas` (`registrar_se_vale`).
  3. Send the `AceiteDeMaquina`.
  4. `iniciar`, only if still `de_pe`.
  5. Emit `ligada:true` while holding the state lock, and only if the instance is still registered.

  A fast `sem-aceite` now finds the instance, and its "desligada" always comes after the "ligada".
  - If `subir` fails: accept then `MaquinaParada{metade-falhou}`, so the MOD still hears "accepted and failed", as before.
  - If the accept send fails: `Viva::descartar` (new; no `aoParar`, no `MaquinaParada`).
- **Tests:**
  - `cargo test -p seele-server --test maquina_no_servidor um_aceite_com_o_resumo_errado_volta_como_sem_aceite`: ok. A wrong digest gives `MaquinaDispensada{SemAceite}`, the ledger stays empty, the right accept still works, and a repeated accept does not dismiss.
  - App: `maquina::testes::sem_aceite_para_a_metade_e_nao_fecha_o_pedido`: ok.
- **Revert proofs:**
  - `if true || repetido` (old silence): fails with `left: None, right: Some(SemAceite)`.
  - `if false && repetido`: fails with "o aceite repetido dispensou a metade aceita".
  - `leva_os_pedidos = conhecido.is_some()` (SemAceite drops the pending): `sem_aceite_para_a_metade_e_nao_fecha_o_pedido` and `a_dispensa_que_chega_depois_do_pedido_vale_contra_ele` fail.
- **By reasoning (no test drives `ligar`, which needs an `AppHandle`):** register-before-send, and emit-under-lock.
- **Residual risk:** a `SemAceite` answering an *old* instance's `PedidoDaMaquina` can arrive after a new instance of the same key registered, and stop it. That fails closed; the strip says "sem-aceite".

### 3. Machine requests handled out of order
- **Change:** `Estado` gets a per-key arrival order assigned on the enlace thread, under the state lock, in `ao_evento`. The functions are `numerar`, `cortar`, `cortado`, `e_o_ultimo` and `esquecer_sessao`, and the number travels in `ChegadoCedo` and `PedidoRecebido`.
  - A known dismissal other than `sem-aceite`, for (key, session), cuts every request that already arrived.
  - The worker drops a request that is no longer the last one or was cut. It checks at the start, and again where the request turns into a dialog: `guardar_pendente_se_vale` does check, insert and emit `maquina-pedido` under one lock.
  - It also checks where the request turns into a half: `registrar_se_vale` checks only the cut, so a newer identical resend does not abort an accept in flight. A dismissed in-flight accept returns `sem-pedido`, which is in the UI's "request gone" set.
- **Tests:**
  - `o_pedido_mais_velho_nao_escreve_por_cima_do_mais_novo` (R1, R2, with R2's thread first): R1 is not shown.
  - `a_dispensa_que_chega_depois_do_pedido_vale_contra_ele` (R then D): no dialog and no half. A dismissal from another session does not cut. A request after the dismissal is valid. `sem-aceite` cuts nothing.
- **Revert proof:** `e_o_ultimo` returning true and `cortado` returning false make both tests fail.
- **Not done:** threads are still one per request (stale ones exit at once). No single worker queue.

### 4. A half stays `ligada` after an enlace reconnect
- **Change:** `ao_evento` now takes the event's generation (`main.rs` passes `carimbo.geracao`). On every event it calls `Maquinas::tirar_de_geracao_velha(sessao, geracao)`, which removes this session's halves with `geracao < event generation`, and they stop with `ServidorSaiu` via `encerrar_e_dizer`, which emits `maquina-estado`.
  - A half already turned on in the new generation stays.
  - The repeated identical request then finds the key unregistered, reads the stored acceptance and turns it on again without a screen (`decidir` gives `AceitarSozinho`).
  - The half's generation is now captured at registration, just before the accept is sent.
- **Test:** `a_reconexao_para_as_metades_de_antes`. Revert proof: appending `&& false` to the filter makes it fail.
- **By reasoning:** the Bridge wiring. If a reconnect lands between registration and the accept send, the new-generation half can be stopped wrongly (fails closed).

### 5. `resumo_da_oferta` failed open
- **Change:** `unwrap_or_default` became `expect` with the reason. Encoding into a `Vec` can only fail when memory runs out.
- **Proof:** by reasoning; the failure cannot be triggered, so there is no test.

### 6. `ArtefatoOferecido.nome` accepted paths
- **Change:** a new `nome_de_arquivo` check in `Validate`: ASCII alphanumerics plus `.`, `-`, `_`; not empty, `.` or `..`; at most `MAX_TEXTO`.
- **Samples:** the `fio_2_0` sample (`server.jar`) is valid, so nothing was regenerated.
- **Test:** `cargo test -p seele-proto --lib o_nome_do_artefato_nao_sai_da_pasta`: ok.
- **Revert proof:** going back to `texto(...)` fails with «o nome «.» passou».
- **Guide:** the rule is documented.

### 7. Unknown dismissal motive was ignored
- **Change:** a new `Fim::DispensaDesconhecida` ("dispensa-desconhecida"; no `MaquinaParada`, `aoParar` is called). The running half stops, logged in WARN; pendings are untouched. A phrase was added to `ui/frases.js` `paradas`, and the frontend test that matches each `Fim` name to a phrase passes.
- **Test:** `uma_dispensa_de_motivo_desconhecido_para_a_metade`. Revert proof: not stopping for an unknown motive makes it fail.

### 8. `mod_desligado` wiped new-package entries
- **Change:** `mod_desligado(mod_id, ligado: Option<&str>)` keeps pendings and acceptances whose hash is the enabled one.
  - `Desligado` gains `a_parar` (each removed machine's tunnels that no remaining acceptance covers) and `restam_aceitas`.
  - `fechar_as_que_nao_valem_mais` passes the enabled hash. When new-package machines remain, it stops only the removed machines' tunnels, not the MOD's blanket `tuneis().mod_desligado`.
- **Test:** `cargo test -p seele-server --lib trocar_de_pacote_nao_leva_o_que_ja_e_do_pacote_novo`: ok.
- **Revert proof:** ignoring the hash fails with `left: [(PersonId(2),"mc-2"),(PersonId(1),"mc-1")]`.

### 9. Guide (`docs/como-se-faz-um-mod.md`)
- **«O que o aceite prende»** now says what the screen shows versus what the digest binds:
  - The screen omits `pack` and each artefact's name, URL, size and hash, all of which the digest binds.
  - So the digest covers more than the screen and never less, and a change to either re-asks, with the screen naming the field.
- **New section «Quando o servidor SEELE reinicia»:** the ledger is in memory, there is no `MaquinaParou` after a restart, and the MOD must ask again.
  - The 5 s hold-back is described as I measured it in the code: a repeat from a *moment* (`aoAcontecer`) after an automatic refusal (`sem-tela`/`so-no-app`) is **dropped**, not delayed. It must be asked again after 5 s. `aoPedir` always goes out.

### 10. `mod_de_referencia` window-half guard counted machine-half API
- **Change:** a generic `sem_o_bloco` (cut by indentation, fails loudly if not found). `sem_o_console` uses it, and a new `sem_a_metade_de_maquina` cuts the `if (daMaquina) { … }` block. The guard now reads `sem_a_metade_de_maquina(&sem_o_console(preludio))`.
- **Effect:** a stronger guard, not a weaker one. Before, `SeeleUI.quedas(...)` passed as "exposto".
- **Proof:** with `SeeleUI.quedas("mc");` appended to the vector, the test fails («o vetor chama `SeeleUI.quedas`, e o prelúdio do executor não o expõe»). The vector was then restored.
- **Before the fix:** the test failed with the review's message.
- The reference MOD has no machine half; the machine API is exercised by the `maquina.rs` executor tests.

### 11. `nenhum_manifesto_de_teste_crava_a_versao_da_api`
- **Change:** `crates/seele-ffi/src/mods.rs` reads `MOD_API_VERSION` through `format!`.
- **Test:** `cargo test -p seele-conformance --test a_api_dos_mods_nao_diverge`: 4 passed.

### 12. `este_processo_tem_tela(true)`
- Verified only: `main.rs:14536` has the call, and the `frontend.rs` guard (around line 26062) covers it. No change.

### Extra: frontend guard updated
- `maquina_a_saida_do_pedido_e_dita_e_a_faixa_e_de_toda_tela` matched the old dismissal text. It now requires `estado.maquinas.dispensar(&chave, sessao, motivo)` together with `dizer_pedidos_desfeitos(app, &dispensa.desfeitos, dispensa.fim)`.
- **Revert proof:** removing that call makes it fail («a dispensa tira o pedido pendente e não diz à tela»).

## Verification (sequential, per crate)

| Command | Result |
|---|---|
| `cargo test -p seele-proto` | all ok |
| `cargo test -p seele-server --no-fail-fast` | only baseline failures: `fora_do_ar` (2), `pedido_recusado` (1) |
| `cargo test -p seele-core`, `-p seele-ffi`, `-p seele-web` (`--no-fail-fast`) | no failures |
| `cargo test -p seele-conformance --test aceite_da_maquina --test tunel_de_jogo --test tunel_pela_ffi --test mod_de_referencia --test a_api_dos_mods_nao_diverge` | 4 + 5 + 5 + 6 + 4 passed |
| `cargo test -p seele-app --bin seele-app` | 504 passed |
| `cargo test -p seele-app --test frontend` | 360 passed; 4 failed, all baseline: `atender_e_ligar…`, `no_two_screens…`, `every_element…`, `the_page_never_composes…` |
| `cargo xtask check-api` | ok |
| clippy (CI command, and the per-crate one) | clean |
| `PLAYWRIGHT=… node apps/seele-app/bancada/aceite-da-maquina.cjs` | chromium 15/0, webkit 15/0 |
