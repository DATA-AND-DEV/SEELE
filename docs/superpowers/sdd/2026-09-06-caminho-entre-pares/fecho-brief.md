# Dispensa de fechamento — três itens, e nada mais

Worktree `wt2`, branch `malha/caminho-entre-pares`, HEAD `004322a`.

Uma onda de doze consertos acabou de passar por re-revisão. Os doze estão bons.
A re-revisão achou **uma regressão que a própria onda criou** e **dois testes que
não prendem o que dizem prender**. Você conserta esses três. Nada mais — se achar
um quarto problema, escreva no relatório e **não conserte**.

Regras da casa: TDD (ver o teste falhar primeiro, pelo motivo certo), prova por
reversão em todo guarda, um commit por item, português nos comentários,
vocabulário servidor / sala de voz / pessoa, motivo de erro sempre enumerado.
Leia `CLAUDE.md` na raiz. **Não despache subagentes.**

## 1 · R1 (Important) — o fim limpo pode desfazer um `UnwatchScreen`

Depois do conserto do C3, **todo** fim limpo de fluxo de par vira `ParFalhou`. O
braço de `ParFalhou` em `crates/seele-server/src/session.rs:2440-2460` manda
`TelaAssistir` para quem relatou **sem conferir se essa pessoa ainda quer a
tela**.

Do lado do cliente, `Comando::Assistir { quero: false }`
(`crates/seele-core/src/enlace.rs:2610`) faz uma coisa só: manda
`unwatch_screen`. **Nada derruba o caminho do par** — a tarefa de
`escoar_tela_alheia` e a conexão com o par seguem vivas.

Então: alguém dá `UnwatchScreen`; algum tempo depois o fluxo do par termina
limpo (contrapressão, quem empresta reconectando, quem empresta saindo da sala)
com a transmissão ainda no ar; o `ParFalhou` sai; e **o servidor volta a subir a
cópia para quem tinha pedido para parar**.

A classe já existia no caminho de erro (`CaiuNoMeio`); o C3 a estendeu ao
caminho de rotina, que é o comum.

Conserta **os dois lados**, porque cada um sozinho deixa metade:

- **servidor**: o braço de `ParFalhou` confere que quem relata ainda assiste
  aquela tela antes de mandar `TelaAssistir`. A estrutura que sabe isso já
  existe — procure em `voice_room.rs` quem guarda os espectadores de um curso.
- **cliente**: `assistir(tela, false)` derruba o caminho do par daquela tela,
  em vez de deixar a tarefa viva mandando `ParFalhou` depois.

Teste: `UnwatchScreen`, depois o fluxo do par termina limpo, e o servidor **não**
volta a mandar a tela. Prova por reversão obrigatória, nos dois lados.

## 2 · O guarda que não tem teste (preocupação 1, invertida)

O `desapontou` no braço de `UnwatchScreen` foi mantido na onda com a explicação
de que o conserto do C3 já cobre aquele caminho. **Não cobre** — é o item 1
acima: nada no cliente encerra o fluxo do par no `UnwatchScreen`. Aquela linha é
o **único** mecanismo que solta a vaga do par ali, e hoje removê-la deixa a
suíte inteira verde (medido pelo revisor: 6/6 na conformance, 374/374 no
`seele-server`).

Escreve o teste que falta: depois de um `UnwatchScreen` que encerra um repasse,
o par volta a poder ser escolhido. Confirma por reversão que ele falha sem a
linha.

(Os outros dois `desapontou` — `StopScreenShare` e `VoiceRoomDeleted` — são
defesa em profundidade legítima e ficam como estão. Não mexa neles.)

## 3 · O teste que afirma e não prende (preocupação 4, confirmada)

`quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`, em
`crates/seele-conformance/tests/tela_por_um_par.rs`.

O revisor removeu o `send(ParFalhou{CaiuNoMeio})` do braço `Err` de
`escoar_tela_alheia` — o mecanismo que este teste diz prender — e o teste passou
**três vezes em três**, em 0,24 s cada. Com o conserto no lugar, o rastro
mostra por quê:

```
o quadro 0 chegou pelo par; agora quem empresta morre
o quadro 6 chegou pelo servidor depois de o par ter morrido
```

`pelo_par` é o **seq 0** — o primeiro quadro da fila FIFO, entregue pelo
servidor antes de `assistir()` ser chamado, quando as duas cópias ainda subiam.
`alvo = pelo_par + 1 = 1`, e o quadro «que prova» é o seq 6, que já estava
enfileirado.

Conserta com o que o teste irmão do mesmo arquivo já faz: drenar a fila e
guardar `maior_seq_ja_enfileirado` antes do corte, exigir estritamente maior
depois, e somar sustentação (`QUADROS_PARA_PROVAR`). Use os helpers que já estão
no arquivo — não escreva outros.

Prova por reversão obrigatória: com o `ParFalhou{CaiuNoMeio}` removido, este
teste tem de falhar, **três execuções em três**. Reporte as três.

## Relatório

`.superpowers/sdd/2026-09-06-caminho-entre-pares/fecho-report.md`: item a item, o
commit, a mensagem de falha de cada reversão, e as três execuções do item 3.
Depois `cargo test --workspace`, `clippy --workspace --all-targets`,
`fmt --check` e `xtask check-deps`, com os números.

No chat devolva só status, commits, uma linha de testes e preocupações.
