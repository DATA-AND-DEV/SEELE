# 10 — Convenções

## Idioma

- **Código, identificadores, tipos e comentários: inglês.** É o padrão do ecossistema Rust e mantém o projeto aberto a contribuição externa.
- **Documentação, specs e ADRs: português.**
- **Strings visíveis ao usuário: i18n desde o início**, com pt-BR e en como locales iniciais. Nunca literal de string direto na interface — o custo de retrofit é alto e o vocabulário temático (`07`) tem que ser traduzido com cuidado.

O glossário de `07` é normativo em ambos os idiomas: `Cage` permanece `Cage`, `Pessoa` vira `Person`, `Taxa de Sincronização` vira `Sync Ratio`.

## Estilo

- `rustfmt` padrão, sem configuração customizada. Discussão de formatação é tempo perdido.
- `clippy` com `-D warnings` no CI. Supressão permitida apenas com `#[allow(...)]` acompanhado de comentário explicando o porquê.
- `#![forbid(unsafe_code)]` em todos os crates, exceto `seele-ffi` e bindings de áudio.
- Erros com `thiserror` nas bibliotecas; `anyhow` apenas nos binários.
- **Nada de `unwrap()` ou `expect()` fora de testes e de invariantes provadas** — e quando houver, com comentário justificando a invariante.

## Testes

| Camada | Abordagem |
|---|---|
| `seele-proto` | Round-trip de serialização, testes de propriedade, fuzzing |
| Jitter buffer | Determinístico, entrada sintética, sem áudio real |
| Protocolo | Testes de integração com servidor em processo |
| Permissões | Um teste por permissão negada, obrigatório |
| TUI | Snapshot de buffer do `ratatui` |
| Áudio ponta a ponta | Manual, com checklist documentado por plataforma |

Regra: se um bug chegou ao usuário, o commit que o corrige inclui o teste que o pegaria.

## Commits e branches

- Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `perf:`.
- Escopo pelo crate: `feat(audio): jitter buffer adaptativo`.
- Branch por tarefa, PR pequeno. `main` sempre verde e sempre executável.

## ADRs

Toda decisão marcada **[EM ABERTO]** nas specs vira um ADR quando resolvida, em `docs/adr/NNNN-titulo.md`:

```
# NNNN — Título
Status: aceito | substituído por NNNN
Contexto: qual problema forçou a escolha
Decisão: o que foi escolhido
Alternativas: o que foi considerado e por que não
Consequências: o que fica mais fácil, o que fica mais difícil
```

Curto. Cinco a quinze linhas. O valor está em existir, não em ser longo.

## Versionamento

SemVer. O protocolo tem versão própria, independente da versão do produto (`02`). Compatibilidade de protocolo: janela de N−1.

## Observabilidade

- `tracing` em todo lugar, com spans nas fronteiras: conexão, Cage, sessão.
- Níveis: `error` para ação necessária do operador; `warn` para degradação; `info` para eventos de ciclo de vida; `debug` e `trace` para desenvolvimento.
- **Nunca logar:** segredos, conteúdo de mensagem, payload de mídia.

## Performance

Antes de otimizar, medir. `criterion` para benchmark do caminho crítico de áudio e serialização. Regressão de performance no caminho de áudio é bug, não questão de gosto.

## Documentação

- `cargo doc` em toda API pública, com exemplo compilável onde fizer sentido.
- `README.md` na raiz: o que é, como rodar em cinco minutos, como hospedar.
- Estas specs são atualizadas quando a realidade diverge delas. Spec desatualizada é pior que spec ausente.
