# 10 — Convenções

## Idioma

- **Código, identificadores, tipos e comentários: inglês.** É o padrão do ecossistema Rust e mantém o projeto aberto a contribuição externa.
- **Documentação, specs e ADRs: português.**
- **Strings visíveis ao usuário: i18n desde o início**, com pt-BR e en como locales iniciais. Nunca literal de string direto na interface — o custo de retrofit é alto e o vocabulário temático (`07`) tem que ser traduzido com cuidado.

O `docs/glossario.md` é normativo em ambos os idiomas, e é a autoridade sobre a palavra que a pessoa lê desde o ADR 0033: `sala de voz` é `VoiceRoom`, `pessoa` é `Person`, `sinal` é `Signal`.

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

**A janela vale só para o lado que ouve, e isso precisa ser lido junto com a regra.** Todo quadro sai carimbado com a `PROTOCOL_VERSION` global (`control::encode`), nunca com a versão negociada, e a build mais velha recusa o carimbo antes de ler o corpo. Então alargar a janela não faz um par publicado voltar a conectar — foi tentado em 2026-09-14, com a subida para a v5, e desfeito pela medida no mesmo dia. Quem não atualizou perde o servidor quando a versão sobe, e o que resolve isso é o seletor de versão do ADR 0046 ou carimbar a versão negociada (pendência #42), não o número desta janela.

## Observabilidade

- `tracing` em todo lugar, com spans nas fronteiras: conexão, VoiceRoom, sessão.
- Níveis: `error` para ação necessária do operador; `warn` para degradação; `info` para eventos de ciclo de vida; `debug` e `trace` para desenvolvimento.
- **Nunca logar:** segredos, conteúdo de mensagem, payload de mídia.

## Performance

Antes de otimizar, medir. `criterion` para benchmark do caminho crítico de áudio e serialização. Regressão de performance no caminho de áudio é bug, não questão de gosto.

## Documentação

- `cargo doc` em toda API pública, com exemplo compilável onde fizer sentido.
- `README.md` na raiz: o que é, como rodar em cinco minutos, como hospedar.
- Estas specs são atualizadas quando a realidade diverge delas. Spec desatualizada é pior que spec ausente.
