# 0013 — Manifestos e CI em inglês

Status: aceito por default
Contexto: `specs/10-convencoes.md` define "código, identificadores, tipos e comentários: inglês" e "documentação, specs e ADRs: português". É silenciosa sobre `Cargo.toml`, `deny.toml`, workflows de CI e `.gitignore`.
Decisão: manifestos, configuração de build e CI seguem a regra de **código**: comentários em inglês.
Alternativas: português, alinhando com a documentação. Descartado porque a justificativa que `10` dá para o inglês — "mantém o projeto aberto a contribuição externa" — se aplica igualmente a um `Cargo.toml`, que é a primeira coisa que um contribuidor abre.
Consequências: fronteira simples de lembrar — se o arquivo é lido por ferramenta, é inglês; se é lido por pessoa como documento, é português. `docs/`, `specs/` e ADRs permanecem em português.
