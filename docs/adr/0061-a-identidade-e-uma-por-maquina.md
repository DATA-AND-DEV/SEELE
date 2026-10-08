# 0061 — A identidade é uma por máquina, e não por linha nem por versão

Status: **aceito**. Decidido por quem responde pelo projeto em 2026-10-08,
em dois tempos:
- a direção: a identidade é uma por máquina, e os dados são por linha;
- a linha legada: a opção (a), e a regra dos 32 bytes.

A regra é **condição**: a 0.15.2 não sai sem ela.
Data: 2026-10-08
Sobre o spec-mestre da 0.16 (`docs/superpowers/specs/2026-10-08-0.16-design.md`,
§1, «O que a atualização para a 0.16 precisa fazer na primeira abertura»), e a
spec da sala pessoal («Uma chave para todas as versões»).
**Emenda o [ADR 0017](0017-identidade-e-pins-em-disco.md) e o
[ADR 0046](0046-toda-versao-continua-de-pe.md).**

## Contexto

O ADR 0017 pôs a identidade em `$SEELE_HOME`, ou em `$XDG_CONFIG_HOME/seele`, ou
em `~/.config/seele`. A identidade é uma por pasta de dados, e o `$SEELE_HOME`
vem primeiro de propósito: é ele que deixa duas pessoas numa máquina, e os
testes das duas pontas de uma conversa dependem disso.

O ADR 0046, com a emenda das duas numerações, deu **uma pasta de dados por
linha** de servidor (`dados/servidor-0.15`). O launcher abre cada versão com o
`$SEELE_HOME` apontando para a pasta da linha dela. A primeira abertura da 0.16
**copia** a identidade junto com os dados da 0.15, para que a pessoa seja a
mesma nas duas linhas.

Isso não basta para a sala pessoal, por dois motivos:
- **Com cópias, a pessoa pode virar outra.** Uma linha aberta antes de a cópia
  existir ganha uma chave nova, e a pessoa aparece como outra nos servidores
  dela (spec-mestre, §1).
- **O código é a chave** (ADR 0060). Uma identidade por pasta faria o código
  mudar com a versão aberta. Um número de telefone que muda quando se troca de
  versão não é um número de telefone.

## Decisão

1. **A identidade é lida de uma pasta da identidade.** A ordem é:
   `$SEELE_IDENTIDADE`, e então a do ADR 0017 (`$SEELE_HOME`,
   `$XDG_CONFIG_HOME/seele`, `~/.config/seele`).
2. **O launcher sempre põe `$SEELE_IDENTIDADE`** na pasta principal desta
   máquina, ao abrir qualquer versão. O `$SEELE_HOME` continua apontando para a
   pasta de dados da linha.
3. **Sem o launcher, nada muda.** Um `$SEELE_HOME` sozinho continua dando duas
   pessoas numa máquina.
4. **Só a chave é da máquina.** Os pins continuam com os dados de cada linha: um
   pino diz «este servidor é o mesmo de ontem», e os servidores de uma linha só
   aparecem nela. Os pinos `@CÓDIGO` só existem na linha 1.
5. **A linha legada também lê `$SEELE_IDENTIDADE`, e as cópias acabam.** É a
   opção (a) do dono: «uma por máquina» é literal, e há um arquivo só. A 0.15.2
   aprende a variável, e a primeira abertura da 0.16 deixa de copiar a
   `identity.key` para `dados/servidor-0.15`. O resto da cópia dos dados fica
   como está.

### O contrato da variável

É o que a 0.15.2, a 0.16 e o launcher seguem, e ele não muda depois de
publicado.

- **O nome:** `SEELE_IDENTIDADE`.
- **O valor:** o caminho absoluto de uma **pasta**, e não do arquivo. A chave é
  `<pasta>/identity.key`, no formato do ADR 0017: 32 bytes crus, em modo `0600`,
  criada nesse modo, e reportada, nunca substituída, se estiver corrompida.
- **Um valor vazio vale como ausente.** Sem a variável, a pasta é a de hoje:
  `$SEELE_HOME`, `$XDG_CONFIG_HOME/seele`, `~/.config/seele` e, por último, a
  pasta de configuração da plataforma.
- **Só a chave sai daí.** O banco, os pins, os conhecidos, a agenda e as
  preferências continuam em `$SEELE_HOME`.
- **O que o launcher passa** é a pasta da identidade **do próprio processo**:
  - a `$SEELE_IDENTIDADE` que ele recebeu, se recebeu uma;
  - senão, a pasta que o `config_dir` dele resolve: no Mac, `~/.config/seele`;
    no Windows, `%APPDATA%\tech.datadev.seele`.

  Assim, uma versão aberta por outra versão continua apontando para o mesmo
  arquivo.
- **Onde se lê, hoje.** Na `linha/0.15` e na 0.16, a `identity.key` é lida em
  produção em dois lugares, os dois em `crates/seele-ffi/src/lib.rs`:
  `impressao_desta_maquina` e o `connect`. Os dois passam a pedir a pasta da
  identidade.

### A condição: a 0.15.2 só assina desafios de exatamente 32 bytes

A linha legada assina com a mesma chave que a linha 1, e isso abria dois
ataques:

- **O que o cliente da 0.15 assina.** O nonce cru que o servidor mandar, de até
  256 bytes (`linha/0.15:crates/seele-core/src/client.rs`, `signing_key.sign(&nonce)`;
  `MAX_PROOF_LEN = 256`). É o oráculo que o S2 fechou na linha 1.
- **Ataque 1: entrar como a pessoa num servidor 1.0.** Um servidor 0.15
  malicioso, visitado pela linha legada, manda como nonce a mensagem inteira do
  S2: `rótulo ‖ impressão de X ‖ nonce de X`, 89 bytes. Ele repassa a assinatura
  a um servidor 1.0 X, e entra em X como a pessoa. Se X for o servidor da
  própria pessoa, entra como Comandante.
- **Ataque 2: atender as ligações da pessoa.** O mesmo servidor manda como nonce
  o conteúdo de um CertificateVerify do TLS 1.3: 130 bytes, que cabem nos 256.
  Com isso ele se apresenta como a sala da pessoa, e o `ConfereCodigo` aceita a
  chave e a assinatura.

**A medida, decidida:** todo servidor 0.15 manda exatamente 32 bytes
(`NONCE_LEN = 32`, `linha/0.15:crates/seele-server/src/session.rs`). A 0.15.2
**só assina desafios de exatamente 32 bytes**, e continua falando com todo
servidor 0.15 de verdade. Nenhum domínio que a chave assina na linha 1 cabe em
32 bytes:
- a prova do S2 tem 89;
- o CertificateVerify tem 130 ou 146;
- um `TBSCertificate` tem centenas;
- o `MORO2` leva o rótulo de 17 bytes e a chave de 32, e já passa de 32 antes dos outros campos.

Quem escreve é a frente do launcher, dona da `linha/0.15`, com o teste e a
prova por reversão. **A 0.15.2 não sai sem ela.**

## Alternativas

1. **A opção (b): a 0.15.2 fica como está, e o launcher planta a identidade da
   máquina em `dados/servidor-0.15`, sem nunca sobrescrever uma diferente.**
   Recusada pelo dono em favor da (a). Duas cópias da mesma chave são duas
   coisas que podem divergir, e a tela teria de explicar uma divergência que um
   arquivo só não tem como produzir.
2. **Uma chave por linha, sem cópia.** A pessoa seria outra em cada linha: o
   apelido dela nos servidores 0.15 ficaria preso à chave antiga (ADR 0017), e o
   código da sala mudaria com a versão. Recusada pela decisão de 08/10.
3. **A cópia como está (a F6, sem este ADR).** Funciona para quem tinha a 0.15
   antes de atualizar, e erra em quem abre uma linha antes da cópia. Ela também
   não resolve a próxima linha: quando houver um Servidor 2.0, a linha 1 vai
   para o depósito com uma pasta própria, e o código da pessoa mudaria.
4. **Mudar o `$SEELE_HOME` em vez de criar uma variável nova.** Recusada: o
   `$SEELE_HOME` é o que separa as pastas de dados, e é o que os testes usam
   para ter duas pessoas.

## Consequências

- O código da pessoa é o mesmo em toda versão que o launcher abrir.
- O risco residual que a spec de 23/09 aceitava («um build até a 0.15 rodando
  com a mesma `identity.key` continua assinando nonce cru, e isso só acontece
  reinstalando uma versão antiga por cima») **deixa de ser raro**: a linha
  legada é exatamente isso, e vive para sempre (ADR 0046). É por isso que a
  regra dos 32 bytes é condição, e não melhoria.
- **Quem faz o quê:**
  - a frente do launcher (F6): o `Lancamento` de toda versão passa
    `SEELE_IDENTIDADE`, a primeira abertura da 0.16 deixa de copiar a chave, e
    a 0.15.2 lê a variável e só assina desafios de 32 bytes;
  - a frente da sala (F7, tarefa 4b): a 0.16 lê a pasta da identidade, e a casca
    passa à FFI a pasta da identidade separada da pasta de dados.

## Custo de reverter

**Médio.** Voltar a uma chave por pasta faz a pessoa ser outra em alguma linha
e muda o código dela. A regra dos 32 bytes da 0.15.2 não precisa reverter:
ela não quebra servidor 0.15 nenhum.
