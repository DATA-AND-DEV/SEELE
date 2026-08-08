# 0011 — Toolchain fixado, MSRV igual ao toolchain

Status: aceito por default
Contexto: `specs/01-arquitetura.md` pede "edição 2021, MSRV fixado" sem dizer qual. MSRV declarada e não verificada em CI é ficção.
Decisão: `rust-toolchain.toml` fixa a versão exata (`1.97.1`); `rust-version` no workspace declara o mesmo `1.97`. Edição 2021, conforme a spec.
Alternativas: declarar uma MSRV baixa (ex. 1.85) para compatibilidade ampla. Descartado porque MAGI é **aplicação**, não biblioteca: nenhum consumidor externo compila estes crates, e `specs/00-visao-geral.md` registra cliente de terceiros como não-objetivo. MSRV baixa custaria trabalho real sem beneficiar ninguém.
Consequências: mais fácil — build idêntico nas três plataformas de CI e na máquina de quem desenvolve; sem classe de bug "compila aqui, não compila lá". Mais difícil — atualizar o toolchain vira commit deliberado, não efeito colateral de `rustup update`. Isso é o objetivo.

Revisar se `magi-proto` algum dia for publicado para consumo de terceiros.
