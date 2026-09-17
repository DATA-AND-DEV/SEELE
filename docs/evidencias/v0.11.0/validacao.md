# Validação da v0.11.0 — 2026-09-17T18:29:09Z

commit: 5cbe7bac0e0d1b1532fe553b246be2a459174cdb
máquina: Darwin 27.0.0 arm64
rustc: rustc 1.97.1 (8bab26f4f 2026-07-14)

fmt: saída 0
## cargo fmt --all -- --check
```
saída 0, sem diferença
```

## cargo clippy --workspace --all-targets --all-features -- -D warnings
```
   Compiling seele-app v0.0.0 (/Users/dev-alexandre/SEELE/apps/seele-app)
    Checking seele-lancador v0.0.0 (/Users/dev-alexandre/SEELE/crates/seele-lancador)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.71s
saída: 0
```

## as três conferências de regra
```
check-api: toda a superfície de MOD ainda aponta para algo (2 versão(ões)).
check-deps: dependency rule holds across 12 workspace crates.
check-versao: a versão alcança as 3 entregas, e nenhum dos 4 lugares que a anunciam lê o Cargo.toml.
```

## cargo test --workspace

Saída **0**. Registro inteiro em `suite-completa.log`.

```
real 298.20
user 83.37
sys 49.46
conjuntos: 82
testes passados: 2086
reprovações: 0
seções Doc-tests: 9
```

## empacotamento local

```
$ ./empacotar/macos.sh 0.11.0    → saída 0
SEELE_0.11.0_aarch64.dmg
seele-cli-0.11.0-macos.tar.gz

$ seeled --versao
seeled 0.11.0

# a versão chegou também ao app, que era o defeito desta rodada:
$ strings SEELE.app/Contents/MacOS/seele-app | grep -c "0\.11\.0"   → 1
$ strings SEELE.app/Contents/MacOS/seele-app | grep -c connection/local → 0
```

**Não provado nesta máquina**, e dito em voz alta: o `.dmg` é só
`aarch64` — um Mac Intel não o abre; Windows e Linux não foram
empacotados; nada foi assinado nem notarizado; e o job `windows-2022`
continua sem nunca ter rodado (pendência 38).

## o que não foi feito, de propósito

Nenhum `push`, nenhuma tag, nenhum binário enviado. A criação da tag e a
publicação são passo manual, descrito em `docs/notas-da-v0.11.0.md`.
