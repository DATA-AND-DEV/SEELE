### Task 9: Quem empresta repassa os bytes, por pedaço e a partir de um quadro-chave

**Files:**
- Modify: `crates/seele-core/src/par.rs`

**Interfaces:**
- Consumes: `ParLigado` (Task 4); `seele_proto::stream::StreamType::Screen`; `seele_proto::screen::ScreenHeader`
- Produces: `pub async fn repassar(ligado: &ParLigado, abertura: &[u8], pedacos: tokio::sync::mpsc::Receiver<Vec<u8>>) -> Result<(), ErroDePar>`

**Por que esta tarefa existe, e por que ela quase não existiu.** As sete
anteriores fazem dois clientes se conectarem. **Nenhuma faz um quadro
atravessar.** É o trabalho todo, e é fácil de perder de vista justamente porque
o §3.4 da spec diz que o lado que recebe já é agnóstico ao transporte — o que é
verdade e não é o suficiente: alguém ainda tem de escrever os bytes do outro
lado.

- [ ] **Step 1: Write the failing test**

Em `par.rs`, em `mod testes`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn o_par_recebe_a_abertura_e_os_pedacos_na_ordem() {
    // **Por pedaço, e nunca remontando quadro.** É a mesma regra que o
    // encaminhamento do servidor segue — *«o encaminhamento é por pedaço, sem
    // remontar quadro»* —, e ela não é estilo: um fluxo QUIC é uma sequência
    // ordenada de bytes, e esperar o quadro inteiro para repassar acrescenta
    // um tempo de quadro de atraso a cada salto. Numa árvore de profundidade
    // três isso seria três quadros, que é o orçamento inteiro.
    let (a, b, ligado) = duas_pontas_ligadas().await;

    let abertura = vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN];
    let (manda, recebe) = tokio::sync::mpsc::channel(8);
    let repassando = tokio::spawn(async move { repassar(&ligado, &abertura_clone, recebe).await });

    manda.send(vec![1, 2, 3]).await.unwrap();
    manda.send(vec![4, 5]).await.unwrap();
    drop(manda);

    // Do outro lado, o que chega é: o byte de tipo, a abertura, e os pedaços
    // na ordem em que foram mandados.
    let mut fluxo = a.accept_uni().await.unwrap();
    let mut tipo = [0_u8; 1];
    fluxo.read_exact(&mut tipo).await.unwrap();
    assert_eq!(tipo[0], seele_proto::stream::StreamType::Screen.byte());
    let mut veio = Vec::new();
    fluxo.read_to_end(&mut veio).await.unwrap();
    assert_eq!(
        veio,
        [vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN], vec![1, 2, 3], vec![4, 5]].concat(),
        "o que chegou ao par não é a abertura seguida dos pedaços na ordem"
    );
    repassando.await.unwrap().unwrap();
    drop(b);
}
```

> **Nota para quem implementa:** `duas_pontas_ligadas()` é um auxiliar a extrair
> do teste `dois_pares_se_ligam_e_o_teste_sabe_como` da Task 4 — ele já monta
> exatamente isso. Extraia-o **antes** de escrever este teste, num commit à
> parte se preferir; duplicar aquele bloco seria dois lugares para consertar
> quando a assinatura de `ligar` mudar. E `abertura_clone` é a cópia que o
> `move` do `spawn` obriga.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p seele-core --lib par::testes::o_par_recebe_a_abertura`
Expected: FAIL de compilação — `repassar` não existe.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Escreve uma transmissão de tela para um par, por pedaço.
///
/// # Por pedaço, e nunca remontando quadro
///
/// A mesma regra do encaminhamento do servidor, pela mesma razão: um fluxo QUIC
/// é uma sequência ordenada de bytes, e esperar o quadro inteiro para repassar
/// acrescenta um tempo de quadro de atraso **a cada salto**.
///
/// # A abertura vai primeiro, e o pedaço nenhum antes dela
///
/// Quem recebe usa `TelaRecebida::do_fluxo`, que lê o byte de tipo e o
/// `ScreenHeader` antes de qualquer coisa. Um pedaço que chegasse antes deslocaria
/// o enquadramento **para sempre** — é o mesmo defeito que o §5.2 do desenho de
/// compartilhamento de tela nomeia, e ele não se corrige depois.
///
/// # Errors
///
/// [`ErroDePar::Escuta`] quando o par para de aceitar bytes.
pub async fn repassar(
    ligado: &ParLigado,
    abertura: &[u8],
    mut pedacos: tokio::sync::mpsc::Receiver<Vec<u8>>,
) -> Result<(), ErroDePar> {
    use tokio::io::AsyncWriteExt as _;

    let mut fluxo = ligado
        .conexao
        .open_uni()
        .await
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    fluxo
        .write_all(abertura)
        .await
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    while let Some(pedaco) = pedacos.recv().await {
        fluxo
            .write_all(&pedaco)
            .await
            .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    }
    fluxo
        .finish()
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p seele-core --lib par::`
Expected: PASS, 8 testes.

- [ ] **Step 5: Write the second failing test — o quadro-chave**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn quem_entra_por_um_par_entra_num_quadro_chave() {
    // **Ligar no meio de um quadro é ligar em lixo.** O servidor já sabe disto:
    // ele segura quem chega numa lista de espera e só abre a cópia no começo do
    // próximo quadro-chave. Quem repassa tem de fazer o mesmo, ou a pessoa que
    // entrou por um par vê blocos coloridos até o quadro-chave seguinte — e não
    // tem como saber por quê.
    //
    // O que se prende aqui é que `repassar` **descarta** o que vem antes do
    // primeiro pedaço marcado como chave, e nada depois dele.
    todo!(
        "montar dois pedaços — um comum e um chave — e afirmar que só o segundo \
         em diante atravessou. A marca de chave é o `TipoDeQuadro::Chave` do \
         `crate::tela`; confira como `EmCurso::esperando` faz do lado do servidor \
         e siga a mesma regra."
    )
}
```

> **Nota:** este `todo!()` é deliberado e é o terceiro e último do plano. A forma
> exata depende de como `crate::tela` expõe a marca de quadro-chave, e inventá-la
> aqui produziria um teste que não compila contra o código real. O **contrato**
> está escrito: descarta antes da primeira chave, nada depois dela.

- [ ] **Step 6: Run, implement, verify**

Escreva o teste de verdade, veja-o falhar, e acrescente a `repassar` o parâmetro
que diz se um pedaço é chave — descartando tudo até a primeira. Rode até verde.

- [ ] **Step 7: Prove the guards**

1. Escreva a abertura **depois** do primeiro pedaço. Esperado: `o_par_recebe_a_abertura_e_os_pedacos_na_ordem` FALHA.
2. Tire o descarte até a primeira chave. Esperado: `quem_entra_por_um_par_entra_num_quadro_chave` FALHA.

Desfaça as duas.

- [ ] **Step 8: Commit**

```bash
cargo fmt -p seele-core && cargo clippy -p seele-core --all-targets
git add crates/seele-core/src/par.rs
git commit -m "feat(par): quem empresta repassa por pedaço, a partir de um quadro-chave"
```

