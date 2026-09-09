### Task 10: A costura, num servidor de verdade com dois clientes

**Files:**
- Create: `crates/seele-server/tests/tela_por_um_par.rs`

**Interfaces:**
- Consumes: tudo das Tasks 1–9.

- [ ] **Step 1: Write the failing test**

Copie o andaime de `crates/seele-server/tests/subida_no_arranque.rs` — `AceitaQualquer`, `servidor_com`, `abrir`, `Par` — e escreva:

```rust
/// O quadro chega ao segundo cliente **pelo primeiro**, e o servidor não o subiu.
///
/// # A prova que importa é a negativa
///
/// Que o quadro chegou, um teste ingênuo prova sem querer: o servidor sabe
/// servir tela desde agosto, e um caminho novo que não funcione é indistinguível
/// de um que funcione se ninguém olhar de onde os bytes vieram. Então a
/// asserção é dupla: o quadro bate byte a byte **e** o contador do servidor diz
/// que aquela cópia não saiu dele. É a mesma forma de prova que
/// `subida_no_arranque.rs` usa.
#[tokio::test(flavor = "multi_thread")]
async fn o_quadro_chega_pelo_par_e_o_servidor_nao_o_subiu() -> Result<()> {
    // 1. servidor, três clientes: quem compartilha, quem empresta, quem assiste
    // 2. quem empresta manda `EmprestarSubida { emprestando: true, ... }`
    // 3. quem compartilha abre a tela; quem assiste pede para ver
    // 4. o servidor manda `SirvaTelaPara` a um e `AssistaTelaPor` ao outro
    // 5. o quadro que chega a quem assiste bate byte a byte com o que saiu
    // 6. e o contador de cópias do servidor para aquela transmissão é **menor**
    //    do que seria sem o par
    todo!("escrever segundo o roteiro acima, com o andaime de subida_no_arranque.rs")
}

/// O par morre no meio, e quem estava atrás dele continua vendo.
#[tokio::test(flavor = "multi_thread")]
async fn quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem() -> Result<()> {
    // 1. o mesmo cenário acima, até o quadro chegar pelo par
    // 2. a conexão de quem empresta é fechada à força
    // 3. quem assiste manda `ParFalhou { motivo: CaiuNoMeio }`
    // 4. e o quadro **seguinte** chega — pelo servidor
    todo!("escrever segundo o roteiro acima")
}
```

> **Nota para quem implementa:** os dois `todo!()` acima são as **únicas** ocorrências
> permitidas neste plano, e são deliberadas: o corpo depende das assinaturas que
> as Tasks 7, 8 e 9 fixarem, e escrevê-lo aqui de antemão seria inventar nomes que
> ainda não existem. O roteiro numerado é o contrato; escreva o corpo contra as
> assinaturas reais quando chegar aqui, **antes** de qualquer implementação nesta
> tarefa.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-server --test tela_por_um_par`
Expected: FAIL — `todo!()` entra em pânico, e depois falha nas asserções reais.

- [ ] **Step 3: Write the real test bodies, then whatever glue they demand**

Escreva os corpos. O que faltar de ligação nas Tasks 7, 8 e 9 aparece aqui — é para isso que este teste vem depois delas e não antes.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p seele-server --test tela_por_um_par`
Expected: PASS, 2 testes.

- [ ] **Step 5: Prove the guards**

1. Faça `Pares::escolher` devolver sempre `None`. Esperado: `o_quadro_chega_pelo_par...` FALHA (o servidor sobe a cópia).
2. Faça `por_onde` nunca cair para o servidor. Esperado: `quando_o_par_morre...` FALHA.

Desfaça as duas.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
git add crates/seele-server/tests/tela_por_um_par.rs
git commit -m "test(par): o quadro chega pelo par, e o servidor assume quando ele morre"
```

---

