# 0017 — Identidade e pins gravados em disco, sem senha

Status: aceito

Contexto: o ADR 0004 escolhe Ed25519 como identidade mas não diz onde a chave mora. Até M2 não importava: sem contas, uma chave nova a cada execução era simplesmente um piloto novo a cada execução. **M3 mudou isso.** O CASPER passou a vincular o apelido à identidade que o reivindicou primeiro, e o MELCHIOR passa a recusar qualquer outra:

```
session closed error=handshake refused: could not establish an account:
nickname belongs to a different identity
```

Isso é exatamente a proteção que o servidor deve oferecer, e exatamente a resposta errada para alguém reabrindo o próprio cliente. Foi encontrado rodando o `plug` duas vezes seguidas — a segunda execução não entra. O mesmo raciocínio vale para os pins: o ADR 0003 é confiança no **primeiro** uso, e um `MemoryPinStore` faz de toda conexão um primeiro uso, o que significa que o aviso que interessa — a chave mudou — nunca dispara.

Decisão: gravar a identidade e os pins em disco, em `$MAGI_HOME` (ou `$XDG_CONFIG_HOME/magi`, ou `~/.config/magi`).

- `identity.key` — os 32 bytes crus da chave, modo `0600`, com a permissão aplicada **na criação do arquivo** e não depois. Uma chave privada legível por todos pela largura de uma syscall foi legível por todos.
- `pins` — uma linha `host impressão` por servidor, texto puro. O formato é legível de propósito: quem foi avisado de que a chave do servidor mudou precisa conseguir abrir o arquivo e comparar a olho. Um formato binário transformaria isso numa conversa de suporte.

`$MAGI_HOME` vem primeiro na ordem justamente para que dois clientes na mesma máquina possam ser dois pilotos — que é o que testar as duas pontas de uma conversa exige.

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

Custo de reverter: **baixo** para o mecanismo — `magi-core::identity` é um módulo com duas funções e um `PinStore`. **Alto** para o formato, assim que existir um usuário: mudar o layout do arquivo depois de distribuído exige migração, e a coisa que migra é a única prova de quem a pessoa é.
