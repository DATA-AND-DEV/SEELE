# A pasta de dados de quem hospedou na 0.15.1

A prova da tarefa 18 da F3 (o B9 do ESTILO; rascunho da API 6, §9): quem
hospedou na 0.15.1 com um MOD e sobe para a 0.16 continua com o quintal dele, e
a versão nova do MOD — outro pacote, outro hash — lê o que a velha gravou. O
teste é `versoes::o_quintal_sobe_da_0_15`, no `apps/seele-app/src/versoes.rs`.

## O que tem aqui

- `pasta/` — **a gravação**, como a 0.15.1 a deixou numa máquina sem servidor
  guardado: o banco em `seele.sql` (o `.dump` do `sqlite3`, porque texto se lê
  numa revisão e um binário não) e o pacote no cache por conteúdo,
  `mod-packages/<hash>/`. Não se edita à mão: regrava-se com o `gerar.sh`;
- `estilo-3.1.0/` — o `seele/estilo` 3.1.0 de mentira que a gravação usou. A
  metade de servidor guarda o tema como a de verdade guarda (`dados.theme`, o
  JSON de `{ revision, theme, enabled }`), sem a conferência das cores;
- `gerar.rs` e `gerar.sh` — o gerador.

## Como a gravação foi feita

**Pelo código da v0.15.1** (`54f542f94`, o commit que o release `v0.15.1`
grava no corpo), e não por uma imitação dele: o `gerar.sh` copia aquele commit
por `git archive`, põe o `gerar.rs` como exemplo do `seele-server` de lá e o
roda. O banco é aberto e migrado pela `Persistence` da 0.15.1 (esquema 13), a
conta de quem hospeda é a do `register_or_find` dela, o MOD é ligado pelo
`mods::enable` dela, e o tema entra no quintal pelo caminho de pedido dela
(`mods::pedidos::executar`), com o `aoPedir` do MOD rodando no QuickJS dela.

Sem subir servidor: nenhuma porta, e nenhuma identidade TLS no banco. O que o
banco leva é a conta de teste (a chave pública `5e5e…`, que não assina nada), os
papéis, o MOD ligado e uma linha de `mod_data`.

```sh
sh apps/seele-app/testes/pasta-da-0.15.1/gerar.sh <pasta-de-trabalho>
```

## O que o teste faz com ela

Os passos do produto, na ordem do `setup` e do HOSPEDAR: a primeira abertura
(a cópia para a linha legada), a migração dos pacotes, o servidor sobre o banco
de sempre (que o migra para o esquema deste build), o 4.0.0 instalado e
aplicado como a gestão aplica, e o pedido de quem hospeda. O servidor é o
`Daemon` com a configuração do HOSPEDAR em `127.0.0.1:0`, sem a escada do ADR
0022, que pediria um mapeamento de porta ao roteador de quem roda o teste.
