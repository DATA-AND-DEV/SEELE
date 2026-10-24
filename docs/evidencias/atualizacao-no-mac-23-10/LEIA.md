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

## Os passos com tela (23/10, pela orquestradora, com o Computer use)

Feitos na mesma pasta, com a 0.16 aberta de novo.

- **O servidor 1.0.** HOSPEDAR AQUI subiu o servidor que já existia pelo Servidor 1.0, depois de pedir o apelido. O banco original foi a esquema 17, com a pessoa e o canal; a cópia para o Servidor 0.15 ficou no esquema 13.
- **Hospedar pelo Servidor 0.15.** Em HOSPEDAR COM OUTRA VERSÃO DO SEELE → 0.15.2 HOSPEDAR COM ESTA, a v0.15.2 guardada abriu com `--hospedar` na 8383. O registro da cópia diz «entrou pessoa=1 … cliente=0.15.2», e no banco a pessoa 1 é a Commander: só a mesma chave chega aí (L56, pela tela).
- **O link `v=0.15.1` pelo CONECTAR.** Colado `seele://192.168.50.142:8383/?fp=e26fada0…747c&v=0.15.1` (a impressão é a SHA-256 do `tls_cert` da cópia), veio a pergunta «Este servidor roda o Servidor 0.15. Ele abre noutra janela do SEELE…», e o ABRIR abriu outra v0.15.2, que entrou no servidor («entrou pessoa=1 sessao=3»). Ficaram três janelas abertas ao mesmo tempo (L20).
- **O computador volta a dormir (L259).** O `pmset -g assertions`, no servidor 1.0:
  - fora da sala, só a do `powerd` («Prevent sleep while display is on»), que não é do SEELE;
  - dentro da SALA 1, mais as duas do `coreaudiod` (`BuiltInSpeakerDevice` e `BuiltInMicrophoneDevice`, `preventuseridlesleep`);
  - depois de sair da sala, as duas somem.
- **A sala pessoal** se registrou no ponto de encontro de produção pelo ENC/2 («a sala está registrada no ponto (ENC/2)»).
- **A rede desta casa:** sem IPv6 próprio, e o roteador sem endereço na internet (192.168.0.2, CGNAT ou roteador duplo). A escada subiu no degrau 4, o furo pelo ponto de encontro.

## Um defeito achado, fora do roteiro

A 0.16 aberta com `--entrar <link de um servidor 0.15>` mostra a mesma pergunta, mas o ABRIR **não lança** a v0.15.2: nenhum processo nasce, e nada vai ao `seele.log`. A mesma v0.15.2, lançada à mão com o ambiente do launcher, abre e entra («sessao=2»). Hoje esse caminho não acontece na prática: só um launcher que já escolheu a 0.16 a abre com `--entrar`. O defeito foi para a F6.

## Uma nota sobre as somas

O `seele.db` da cópia deu `d83570f1…3d23`, igual ao original, na primeira conferência. Depois, a soma mudou para `79b866e7…`. A causa foi o `sqlite3` desta conferência: ao abrir a cópia (em WAL), ele levou o diário ao arquivo principal. O conteúdo é o mesmo, e o esquema continua 13. Daí em diante, as leituras foram feitas com `mode=ro`.

## O que não foi medido

- A nota da primeira abertura na tela: a primeira abertura foi feita pelo terminal.
- O Windows inteiro: ele sai sem a medida na tela, pela D-dono-2.
