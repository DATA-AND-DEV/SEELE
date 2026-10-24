# A atualização da 0.15.1 para a 0.16 no Mac, medida pelo terminal (23/10/2026)

A parte do roteiro do dono (`docs/superpowers/sdd/2026-10-08-0.16/F6/roteiro-do-dono-23-10.md`,
passos 1 e 2) que se confere sem clicar, feita pela orquestradora.

- **A origem:** o SEELE 0.15.1 instalado em `/Applications`, o de quem atualiza.
- **O destino:** a 0.16 montada pelo `./empacotar/macos.sh 0.16.0` no `3c8c21b6`, sem publicar.
- **Os dados:** os dois rodaram com o `SEELE_HOME` numa pasta temporária, e nada tocou os dados reais (o SEELE de verdade estava fechado e a porta 8383, livre).

## O que foi feito

1. A 0.15.1 com `--hospedar`. Ela subiu o servidor na 8383 e criou `identity.key` e `seele.db`, e foi fechada pelo `quit` do app, como o Cmd-Q.
2. A 0.16, na mesma pasta: a primeira abertura, o que o atualizador faria. Ela foi fechada da mesma forma.

## O que foi medido

| | antes (0.15.1) | depois da primeira abertura da 0.16 |
|---|---|---|
| `identity.key` | `7c098be9…d880` | `7c098be9…d880`, **igual** |
| `seele.db` | `d83570f1…3d23`, esquema 13 | o original segue em `d83570f1…3d23`, esquema 13 (a 0.16 só migra ao hospedar um Servidor 1.0) |
| a cópia para o Servidor 0.15 | — | `lancador/dados/servidor-0.15/`, com o `PRONTA`; o `seele.db` **byte a byte** o de antes (`d83570f1…3d23`), e **sem** `identity.key` (D-id-1) |
| a v0.15.2 guardada | — | baixada e instalada sozinha em **9 s**: `lancador/versoes/0.15.2/INSTALADA` com as 4 linhas (versão, executável, linha `0.15`, soma) |

O registro da 0.16 diz «a linha legada na primeira abertura resultado=guardada: 0.15.2».

**A correção ao roteiro:** a cópia mora em `lancador/dados/servidor-0.15/`, e não em `dados/servidor-0.15/`, como o roteiro dizia.

## O que não foi medido

O que pede clique ficou de fora:
- a nota da primeira abertura na tela;
- hospedar pelo Servidor 0.15, como Comandante;
- o link `v=0.15.1` abrindo a v0.15.2 noutra janela;
- o `pmset` dentro e fora de uma sala.

O Windows também ficou de fora.
