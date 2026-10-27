# 0017 — Identidade e pins gravados em disco, sem senha

Status: aceito

> **Vocabulário.** Esta página é anterior ao [ADR
> 0035](0035-o-codigo-deixa-de-falar-evangelion.md) e diz `Dogma` onde o
> produto hoje diz **servidor**, `piloto` onde diz **pessoa**, `CASPER` onde
> diz **PERSISTENCE** e `MELCHIOR` onde diz **PERMISSIONS**. O texto fica como
> foi escrito: o 0035 preserva de propósito o registro de ontem, e o
> `docs/glossario.md` é a autoridade sobre a palavra de hoje.

> **Emenda de 08/10/2026 ([ADR 0061](0061-a-identidade-e-uma-por-maquina.md)).**
> A identidade passa a ter uma pasta própria, `$SEELE_IDENTIDADE`, que o
> launcher aponta para a pasta principal da máquina, e os pins continuam com os
> dados. Sem a variável, a ordem deste ADR continua valendo, e o `$SEELE_HOME`
> continua dando duas pessoas numa máquina.

Contexto: o ADR 0004 escolhe Ed25519 como identidade mas não diz onde a chave mora. Até M2 não importava: sem contas, uma chave nova a cada execução era simplesmente um piloto novo a cada execução. **M3 mudou isso.** O CASPER passou a vincular o apelido à identidade que o reivindicou primeiro, e o MELCHIOR passa a recusar qualquer outra:

```
session closed error=handshake refused: could not establish an account:
nickname belongs to a different identity
```

Isso é exatamente a proteção que o servidor deve oferecer, e exatamente a resposta errada para alguém reabrindo o próprio cliente. Foi encontrado rodando o `plug` duas vezes seguidas — a segunda execução não entra. O mesmo raciocínio vale para os pins: o ADR 0003 é confiança no **primeiro** uso, e um `MemoryPinStore` faz de toda conexão um primeiro uso, o que significa que o aviso que interessa — a chave mudou — nunca dispara.

Decisão: gravar a identidade e os pins em disco, em `$SEELE_HOME` (ou `$XDG_CONFIG_HOME/seele`, ou `~/.config/seele`).

- `identity.key` — os 32 bytes crus da chave, modo `0600`, com a permissão aplicada **na criação do arquivo** e não depois. Uma chave privada legível por todos pela largura de uma syscall foi legível por todos.
- `pins` — uma linha `host impressão` por servidor, texto puro. O formato é legível de propósito: quem foi avisado de que a chave do servidor mudou precisa conseguir abrir o arquivo e comparar a olho. Um formato binário transformaria isso numa conversa de suporte.

`$SEELE_HOME` vem primeiro na ordem justamente para que dois clientes na mesma máquina possam ser dois pilotos — que é o que testar as duas pontas de uma conversa exige.

Alternativas:

1. **Chave protegida por senha.** É o que se quer no fim, e não é isto. Pede um fluxo de desbloqueio, uma decisão sobre cache em memória e um caminho de recuperação — trabalho de contas, não de interface. Empurrar para M5 é deliberado.
2. **Chaveiro do sistema operacional** (Keychain, Secret Service, DPAPI). Três implementações, três dependências nativas e três modos de falha em máquinas sem sessão gráfica — que é onde um servidor auto-hospedado costuma estar. Não em v1.
3. **Derivar a chave de uma frase secreta.** Sem senha não há segredo; com senha é a alternativa 1.

Consequências:

- **Uma chave sem senha no diretório do usuário: quem consegue ler esse arquivo consegue ser este piloto.** É a mesma fronteira de confiança de uma chave SSH privada sem senha, e está escrito aqui e no módulo para que seja uma escolha registrada e não um descuido.
- Perder o arquivo é perder o apelido naquele Dogma até um operador liberá-lo. Não há recuperação em M4. Isso precisa aparecer na documentação do usuário antes de qualquer release para fora.
- Um `identity.key` corrompido é **reportado, nunca substituído**. Sobrescrever destruiria a identidade de alguém cujo disco encheu no meio de uma escrita; recusar iniciar é recuperável, o contrário não.
- Um arquivo de pins corrompido custa um novo primeiro contato, não uma falha de arranque. Recusar abrir um cliente de conversa por causa de um cache ilegível seria a troca pior.
- No Windows a proteção é a ACL herdada do diretório do perfil, que é mais fraca que o modo Unix. Registrado, não resolvido.

Custo de reverter: **baixo** para o mecanismo — `seele-core::identity` é um módulo com duas funções e um `PinStore`. **Alto** para o formato, assim que existir um usuário: mudar o layout do arquivo depois de distribuído exige migração, e a coisa que migra é a única prova de quem a pessoa é.

### Adendo de 27/10/2026 — bases iguais e tags do servidor

A base do nome pode ser repetida por contas diferentes. A identidade continua
sendo a chave pública e o PersonId; nomes e tags não concedem autoridade.
O servidor atribui uma tag aleatória de quatro caracteres base36, única naquele
servidor e persistida com a conta. Renomear, reconectar ou restaurar a cópia do
banco não troca a tag. Ela não identifica a pessoa entre servidores distintos.

A migração 19 preserva a base integral de até 32 bytes e todos os IDs. A primeira
abertura pelo app conserva o portão de confirmação e cópia verificada antes de
migrar. O protocolo 11 acrescenta uma mensagem de apresentação por PersonId;
a faixa mínima continua 10 e os quadros publicados não mudam. Clientes 11
apresentam somente a base integral; a tag fica interna, inclusive nos nomes
acessíveis e cartões de MOD. Clientes 10 recebem um alias único de até 32
bytes: prefixo UTF-8 de até 27 bytes e a tag. Apenas essa apresentação legada
pode ser abreviada; o nome no banco não é truncado. O alias também continua nos
campos internos de mídia, impedindo ambiguidade de volume/SSRC. O retrato público
dos MODs apresenta a base e mantém o PersonId como alvo das ações. Detalhes de
identidade e moderação permanecem acessíveis. O cliente não escolhe a tag de
outra conta. Sem metadados, o nome legado permanece inteiro: um `#` real não é
removido por heurística.
