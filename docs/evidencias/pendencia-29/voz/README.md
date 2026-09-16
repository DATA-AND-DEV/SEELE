# A voz da desistência: qual canal o `libtest` engole

A fila da vaga tem um modo de falha próprio (causa 2 da §29: sob acesso
restrito o CoreAudio não recusa, ele espera). Quando quem tem a vez trava, a
fila é abandonada e a rodada volta a ser paralela — com a §29 de volta. Isso é
aceitável **se aparecer no registro**, e só se aparecer.

A primeira versão anunciava a desistência com `eprintln!`. Uma revisão
independente apontou que isso é silêncio disfarçado: o `libtest` captura a
saída de cada teste e imprime apenas a dos que **reprovam**, e quem desiste de
esperar é um teste que depois passa. Medimos:

`dois_canais.rs` é um teste que passa e escreve a mesma frase por dois
caminhos — a macro `eprintln!` e `writeln!` direto em `std::io::stderr()`.

| execução | `CANAL_MACRO_EPRINTLN` | `CANAL_STDERR_DIRETO` |
| --- | --- | --- |
| `cargo test` (o que a CI roda) — `capturado.log` | **não aparece** | aparece |
| `cargo test -- --nocapture` — `nocapture.log` | aparece | aparece |

A captura do `libtest` vive dentro das macros de impressão, que consultam um
destino por thread antes de escrever; `std::io::stderr()` escreve no descritor
e não consulta nada. Por isso `vaga::em_voz_alta` usa `writeln!` em
`std::io::stderr()`.

A linha da tabela com `--nocapture` é a prova por reversão deste conserto: é
exatamente o que se veria se o aviso voltasse a sair por `eprintln!` — visível
só quando alguém pede, e a CI não pede.
