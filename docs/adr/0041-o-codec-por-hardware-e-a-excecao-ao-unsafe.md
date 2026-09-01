# ADR 0041 — O codec por hardware, e a exceção nomeada ao `unsafe`

**Estado:** aceito
**Data:** 2026-09-01

`seele-video` deixa de herdar `unsafe_code = "forbid"` do workspace e declara o
próprio bloco com `deny`. Um módulo nomeado — o codificador por VideoToolbox —
recebe `allow`. Nenhum outro lugar do crate ganha nada.

## Contexto

Quem transmite a tela é, quase sempre, quem está jogando. O custo de codificar
sai do mesmo processador que desenha o jogo, e hoje ele sai inteiro: o
`Codificador` é o OpenH264, em software.

Do outro lado a conta já está resolvida e nunca foi decidida aqui — quem assiste
entrega os bytes ao decodificador do sistema pela janela, que é acelerado por
GPU. O desequilíbrio é só de quem transmite, e é o que faz «várias pessoas
transmitindo ao mesmo tempo» ser uma pergunta sem resposta boa neste produto.

O caminho para equilibrar é o codificador do sistema: VideoToolbox no macOS,
Media Foundation no Windows. Os dois são C, e chamá-los é `unsafe`. Não há
invólucro seguro — procurei, e o que existe em Rust para VideoToolbox são
bindings crus da família `objc2`.

## A decisão que este ADR reverte

O `Cargo.toml` do `seele-video` registra, sobre a captura do Windows:

> a alternativa é WGC crua pelo `windows` 0.61, com zero crates novos e uma
> exceção nomeada ao `forbid unsafe_code` — que a spec recusou para a v1.

Aquela recusa foi certa e continua certa **para aquele caso**: existia uma porta
segura, `windows-capture`, e o preço dela era 31 crates. Trocar segurança por
dependências, havendo escolha, é uma troca ruim.

Aqui não há escolha. Não existe VideoToolbox seguro, e a alternativa a `unsafe`
não é «outro crate»: é **não ter codec por hardware**.

## O que exatamente muda

Nada no workspace. O mecanismo já estava escrito no próprio `Cargo.toml` que
declara a regra:

> Crates needing an exception (seele-ffi, audio bindings) declare their own
> `[lints]` block instead of inheriting this one — `forbid` cannot be relaxed by
> `allow`, which is precisely the property we want.

`seele-ffi` e os bindings de áudio já fazem isso. `seele-video` passa a fazer
também, e é toda a mudança de política: um crate a mais na lista de quem declara
o próprio bloco.

Dentro dele, `deny` e não `forbid`, e `allow` **só** nos módulos de plataforma do codificador —
`codec/macos.rs` hoje, `codec/windows.rs` amanhã. A garantia deixa de ser «não existe `unsafe` neste crate» e passa a
ser «existe em um módulo nomeado, e o compilador recusa em qualquer outro» — que
é uma garantia mais fraca, e é a mais forte que este trabalho admite.

## Por que não um crate separado

Foi a primeira ideia: um crate só para o codificador de hardware, com a exceção
inteiramente contida nele. Ela esbarra na costura: o trait `CodificaVideo` e o
`QuadroI420` moram em `seele-video`, então o crate novo dependeria dele, e o
`armar` — que escolhe entre hardware e OpenH264 — dependeria do crate novo.
Ciclo.

Desfazer o ciclo custaria mudar a costura de lugar por uma razão que não é da
costura. Um módulo com `allow` nomeado dá a mesma garantia prática por um preço
que não distorce o desenho.

## Consequências

**A queda é obrigatória.** Uma máquina sem suporte, um driver que recusa
1080p60, uma sessão que o sistema não concede — nada disso pode custar o
compartilhamento. `armar` tenta o hardware e cai para o OpenH264, e quem
transmite não fica sabendo por um erro, e sim por não sentir a CPU.

**O `unsafe` fica cercado por testes, não por confiança.** O módulo é o único
lugar do produto onde um ponteiro do sistema é lido, e é onde a prova tem de ser
mais dura que a leitura.

**O Windows vem depois, e por medida.** O macOS primeiro, porque é onde dá para
medir antes de escrever a segunda metade.
