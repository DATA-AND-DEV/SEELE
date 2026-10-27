# 04 — Servidor (seeled)

## Identidade

O daemon se chama `seeled`. Uma instância é um **Server Central**. Internamente, três subsistemas nomeados — não é decoração: são fronteiras reais de módulo, e o estado de cada um aparece nas telas de diagnóstico do cliente.

| Subsistema | Responsabilidade |
|---|---|
| **PERMISSIONS** | Identidade, autenticação, sessões, cargos e permissões |
| **MEDIA** | Roteamento de mídia: assinaturas de VoiceRoom, encaminhamento de datagrams, controle de banda |
| **PERSISTENCE** | Estado persistente: VoiceRooms, Linhas, histórico, configuração, migrações |

Quando um subsistema está degradado, o cliente mostra isso explicitamente. "Os três concordam" é o estado nominal.

## Modelo de domínio

```
Server (a instância)
 ├─ VoiceRoom       — canal de voz     (id, nome, limite, senha?, papel mínimo)
 ├─ Linha      — canal de texto   (id, nome, papel mínimo de leitura/escrita)
 ├─ Pessoa     — conta de usuário (id, apelido, chave pública, cargos)
 └─ Cargo      — posição, o que concede e o que restringe
```

VoiceRooms e Linhas são independentes; uma sala de voz pode ter uma Linha associada, mas não é obrigatório.

## Cargos e permissões (desde a 0.17, ADR 0064)

O modelo mental é o do Discord, com as decisões SEELE que o ADR 0064 escreve ao lado de cada uma.

**Permissões**, enumeradas, sem sistema de expressão:
- do produto: ver salas de voz, entrar em sala de voz, falar, transmitir tela, ler canais, escrever em
  canais, enviar arquivos, remover mensagens de outros, mover pessoas, expulsar, banir, gerenciar salas e
  canais, gerenciar cargos, gerenciar servidor e **Administrador** (implica todas);
- de MOD: uma ação só é autorizável se o MOD a declarou no `mod.json` (API 7). A chave é `{mod_id, acao}`,
  e o servidor autoriza no despacho. Uma permissão desconhecida nunca concede.

**A regra:**
- **o proprietário** pode tudo;
- **o banido** não pode nada;
- **para o resto**, o efetivo é a soma do que o `@everyone` e os cargos da pessoa concedem, menos o que
  qualquer um deles restringe. A **restrição vence** qualquer concessão, inclusive a do Administrador.

**A hierarquia**, por posição (o `@everyone` é 0):
- gerenciar cargos só alcança cargos e pessoas abaixo do seu cargo mais alto, e só concede o que se tem;
- toda mudança passa pelo **guarda do ganho efetivo**: ninguém dá a ninguém, nem a si, o que não tem
  efetivamente;
- moderar só alcança quem está abaixo, e nunca o proprietário;
- os verbos de cargo levam a **revisão** do conjunto, e uma revisão velha é recusada.

**O proprietário** é a chave **declarada pela máquina que hospeda**:
- o app, ao hospedar;
- a sala pessoal, pela chave dela;
- o `seeled`, por `--proprietario <impressão>` na linha de comando (ele não lê arquivo de configuração;
  o `ServerConfig::proprietario` é o mesmo campo para quem sobe o servidor em processo).

Chegar primeiro, reconectar ou repetir um apelido nunca dá autoridade. Sem declaração, o servidor sobe sem
dono e registra o aviso acionável.

**Cargos semeados** (a migração 18 preserva o efeito exato dos quatro papéis de antes):
- **Comandante** (tudo, com o Administrador);
- **Operador** (moderação);
- **Pessoa** (uso normal, e o cargo de chegada);
- **Observador** (ouvir e ler, restringindo falar, escrever, enviar arquivos e transmitir tela);
- **@everyone**, vazio.

**Sem sobrescritas por canal na 0.17.** Fica o «cargo exigido» de leitura e escrita da Linha: a pessoa
tem de ter aquele cargo, e quem administra passa.

## Concorrência

- Uma task `tokio` por conexão, tratando o stream de controle.
- Uma task por **VoiceRoom**, dona do estado daquele VoiceRoom. Entrada e saída por `mpsc`. Isso elimina lock global e torna o roteamento de mídia trivialmente paralelo.
- Datagrams de mídia entram na task da sala de voz, que replica para os assinantes. Zero cópia sempre que possível (`Bytes`).

## Encaminhamento de mídia (MEDIA)

1. Recebe datagram de um `ssrc` conhecido.
2. Valida que o remetente está na sala de voz e tem permissão de falar. **Validar sempre** — não confiar no cliente.
3. Encaminha o payload íntegro a todos os outros assinantes da sala de voz.
4. Nunca decodifica o Opus.

Controle de fluxo: limite por remetente de quadros por segundo (um cliente honesto envia 50/s). Acima disso, descarta e registra. Protege contra cliente malicioso saturando a sala de voz.

**[EM ABERTO]** Política acima de 20 falantes simultâneos: encaminhar apenas os N mais ativos, medidos por energia reportada? Requer que o cliente reporte energia, o que é confiável apenas parcialmente.

## Persistência (PERSISTENCE)

SQLite, arquivo único, WAL ligado. Tabelas: `pessoas`, `papeis` (os cargos, com `position`), `pessoas_papeis`, `voice_rooms`, `linhas`, `mensagens`, `banimentos`, `config` (o `proprietario` declarado), `mod_permissoes`, `revisao_dos_cargos`, `schema_version`.

- Migrações embutidas no binário, aplicadas no boot, versionadas e irreversíveis.
- Histórico de mensagens com retenção configurável (padrão: ilimitado).
- Índice em `(linha_id, criado_em)` para paginação por cursor.
- Escritas de mensagem em lote com `flush` por tempo (~200 ms) para não fazer fsync por mensagem.

## Configuração

Arquivo TOML único, mais variáveis de ambiente para segredos. Exemplo de forma esperada:

```toml
[server]
nome = "Casa"
descricao = "..."
max_pessoas = 50

[rede]
escuta = "0.0.0.0:8383"
certificado = "auto"          # auto | caminho para PEM

[audio]
bitrate_maximo = 48000
quadros_por_segundo_max = 60

[persistencia]
caminho = "/var/lib/seeled/seele.db"
retencao_dias = 0             # 0 = ilimitado
```

Recarga a quente de configuração: **[EM ABERTO]**. Provavelmente não em v1 — reiniciar é aceitável.

## Operação

- Log estruturado com `tracing`, saída JSON opcional.
- Endpoint de saúde: **[EM ABERTO]** — HTTP separado ou comando no protocolo de controle? Um HTTP mínimo facilita monitoramento externo.
- Métricas em formato Prometheus: pessoas conectados, voice_rooms ativos, datagrams/s, taxa de descarte, uso de banda.
- Desligamento gracioso: avisar clientes com motivo `ManutencaoProgramada`, dar 3 s, encerrar.

## Critérios de aceite

- Suporta 50 sessões e 5 VoiceRooms ativos em 1 vCPU / 512 MB.
- Reinício não perde mensagem confirmada ao cliente.
- Cliente malicioso não consegue: falar em VoiceRoom sem permissão, ler Linha sem permissão, saturar CPU com datagrams, ou forjar `ssrc` de outro pessoa.
