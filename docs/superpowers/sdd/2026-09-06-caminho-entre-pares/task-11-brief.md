### Task 11: O roteiro de duas máquinas mede o que nenhum teste daqui mede

**Files:**
- Modify: `docs/teste-duas-maquinas.md`

**Interfaces:** nenhuma. É documento.

- [ ] **Step 1: Write the section**

Acrescente ao fim de `docs/teste-duas-maquinas.md`:

```markdown
## O caminho entre pares (subprojeto A da malha)

**Isto é o que nenhum teste automático deste repositório consegue produzir.** Um
furo de NAT entre dois roteadores domésticos não acontece em `127.0.0.1`, e os
dois números abaixo são a razão de o subprojeto A existir antes do B.

Precisa de **três** máquinas, ou de duas mais um celular em 4G — o ponto é que
quem empresta e quem assiste **não** estejam na mesma rede.

1. Numa máquina, suba o servidor e compartilhe a tela.
2. Noutra rede, entre duas pessoas. Numa delas, ligue o empréstimo de subida.
3. Na terceira, peça para assistir.

Anote, do `tracing` de quem assistiu:

| o que | onde ler | anote |
|---|---|---|
| como a ligação chegou | `um par ligou`, campo `como` | `Local` ou `Furo` |
| ida e volta com o par | `um par ligou`, campo `ida_e_volta` | ms |
| quando não ligou | `o par não veio`, campo `erro` | o motivo enumerado |

Repita **umas dez vezes**, em redes diferentes se der. O que se quer é a
**fração** de `Furo` sobre tentativas, e ela é o número que decide o desenho da
árvore: se o furo falhar em boa parte dos pares, o subprojeto B não pode supor
que qualquer par se alcança, e vira «árvore entre quem se alcança, estrela para
o resto».

Escreva os dois números em `docs/m1-medicoes.md`, ao lado dos outros. **Enquanto
eles não existirem, a aritmética da malha na spec continua sendo estimativa**, e
está marcada como tal.
```

- [ ] **Step 2: Verify**

Run: `cargo test --workspace`
Expected: PASS — nenhum guarda cobre documento, e é por isso que este passo existe: confirmar que nada quebrou junto.

- [ ] **Step 3: Commit**

```bash
git add docs/teste-duas-maquinas.md
git commit -m "docs(teste): o roteiro mede o furo entre pares e o custo de um salto"
```

---

## Depois deste plano

O subprojeto A termina com **o caminho existindo e dois números medidos**. Nada
de árvore, nada de escolha boa, nada de interface.

O **subprojeto B** só deve ser desenhado depois de a Task 11 ter sido executada
por uma pessoa, em máquinas de verdade. Se a fração de furo vier baixa, o
desenho da árvore muda — e essa é a razão inteira de a ordem ser esta.
