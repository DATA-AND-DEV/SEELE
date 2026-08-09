# 04 — Servidor (seeled)

## Identidade

O daemon se chama `seeled`. Uma instância é um **Dogma Central**. Internamente, três subsistemas nomeados — não é decoração: são fronteiras reais de módulo, e o estado de cada um aparece nas telas de diagnóstico do cliente.

| Subsistema | Responsabilidade |
|---|---|
| **MELCHIOR** | Identidade, autenticação, sessões, papéis e permissões |
| **BALTHASAR** | Roteamento de mídia: assinaturas de Cage, encaminhamento de datagrams, controle de banda |
| **CASPER** | Estado persistente: Cages, Linhas, histórico, configuração, migrações |

Quando um subsistema está degradado, o cliente mostra isso explicitamente. "Os três concordam" é o estado nominal.

## Modelo de domínio

```
Dogma (a instância)
 ├─ Cage       — canal de voz     (id, nome, limite, senha?, papel mínimo)
 ├─ Linha      — canal de texto   (id, nome, papel mínimo de leitura/escrita)
 ├─ Piloto     — conta de usuário (id, apelido, chave pública, papéis)
 └─ Papel      — conjunto de permissões
```

Cages e Linhas são independentes; um Cage pode ter uma Linha associada, mas não é obrigatório.

## Permissões

Modelo simples e enumerado, sem sistema de expressão. Cada Papel carrega um conjunto:

`ver_cage`, `inserir_plug`, `falar`, `ler_linha`, `escrever_linha`, `remover_mensagem`, `mover_piloto`, `expulsar`, `banir`, `gerenciar_cages`, `gerenciar_papeis`, `administrar_dogma`.

Papéis padrão: **Comandante** (tudo), **Operador** (moderação), **Piloto** (uso normal), **Observador** (só ouvir e ler).

Regra: permissões negadas vencem concedidas. Sem herança em árvore — a complexidade não se paga na escala alvo.

## Concorrência

- Uma task `tokio` por conexão, tratando o stream de controle.
- Uma task por **Cage**, dona do estado daquele Cage. Entrada e saída por `mpsc`. Isso elimina lock global e torna o roteamento de mídia trivialmente paralelo.
- Datagrams de mídia entram na task do Cage, que replica para os assinantes. Zero cópia sempre que possível (`Bytes`).

## Encaminhamento de mídia (BALTHASAR)

1. Recebe datagram de um `ssrc` conhecido.
2. Valida que o remetente está no Cage e tem permissão de falar. **Validar sempre** — não confiar no cliente.
3. Encaminha o payload íntegro a todos os outros assinantes do Cage.
4. Nunca decodifica o Opus.

Controle de fluxo: limite por remetente de quadros por segundo (um cliente honesto envia 50/s). Acima disso, descarta e registra. Protege contra cliente malicioso saturando o Cage.

**[EM ABERTO]** Política acima de 20 falantes simultâneos: encaminhar apenas os N mais ativos, medidos por energia reportada? Requer que o cliente reporte energia, o que é confiável apenas parcialmente.

## Persistência (CASPER)

SQLite, arquivo único, WAL ligado. Tabelas: `pilotos`, `papeis`, `piloto_papeis`, `cages`, `linhas`, `mensagens`, `banimentos`, `config`, `schema_version`.

- Migrações embutidas no binário, aplicadas no boot, versionadas e irreversíveis.
- Histórico de mensagens com retenção configurável (padrão: ilimitado).
- Índice em `(linha_id, criado_em)` para paginação por cursor.
- Escritas de mensagem em lote com `flush` por tempo (~200 ms) para não fazer fsync por mensagem.

## Configuração

Arquivo TOML único, mais variáveis de ambiente para segredos. Exemplo de forma esperada:

```toml
[dogma]
nome = "Terceira Tóquio"
descricao = "..."
max_pilotos = 50

[rede]
escuta = "0.0.0.0:8383"
certificado = "auto"          # auto | caminho para PEM

[audio]
bitrate_maximo = 48000
quadros_por_segundo_max = 60

[persistencia]
caminho = "/var/lib/seeled/dogma.db"
retencao_dias = 0             # 0 = ilimitado
```

Recarga a quente de configuração: **[EM ABERTO]**. Provavelmente não em v1 — reiniciar é aceitável.

## Operação

- Log estruturado com `tracing`, saída JSON opcional.
- Endpoint de saúde: **[EM ABERTO]** — HTTP separado ou comando no protocolo de controle? Um HTTP mínimo facilita monitoramento externo.
- Métricas em formato Prometheus: pilotos conectados, cages ativos, datagrams/s, taxa de descarte, uso de banda.
- Desligamento gracioso: avisar clientes com motivo `ManutencaoProgramada`, dar 3 s, encerrar.

## Critérios de aceite

- Suporta 50 sessões e 5 Cages ativos em 1 vCPU / 512 MB.
- Reinício não perde mensagem confirmada ao cliente.
- Cliente malicioso não consegue: falar em Cage sem permissão, ler Linha sem permissão, saturar CPU com datagrams, ou forjar `ssrc` de outro piloto.
