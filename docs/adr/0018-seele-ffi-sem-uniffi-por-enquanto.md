# 0018 — `seele-ffi` com a forma que o `uniffi` exige, sem a dependência ainda

Status: aceito

> **Vocabulário.** Esta página é anterior ao [ADR
> 0035](0035-o-codigo-deixa-de-falar-evangelion.md) e diz `PilotId` onde o
> produto hoje diz **`PersonId`** e `Plug` onde diz **`Connection`**. O texto
> fica como foi escrito: o 0035 preserva de propósito o registro de ontem, e o
> `docs/glossario.md` é a autoridade sobre a palavra de hoje.

Contexto: `specs/06-clientes-gui.md` descreve a superfície da `seele-ffi` e diz que ela deve ser "gerada com `uniffi` sempre que possível". Ao chegar em M5 a pergunta ficou concreta: o `uniffi` entra agora, com o desktop, ou em M6, com o mobile?

O `uniffi` gera *bindings* — Kotlin, Swift, Python. O Tauri não usa nenhum deles: o lado Rust do Tauri chama Rust. Trazer o `uniffi` em M5 significa carregar um proc-macro e um passo de build por um milestone inteiro sem um único consumidor de binding.

O que o `uniffi` realmente impõe, e que **não** é opcional adiar, é a **forma** da API: objetos atrás de `Arc`, sem genéricos na superfície, erros como enum, callbacks como trait `Send + Sync`, tipos de valor sem referências emprestadas. Descobrir em M6 que a superfície não tem essa forma, com o desktop já dependendo dela, seria caro.

Decisão: escrever a `seele-ffi` **com a forma que o `uniffi` exige**, sem depender do `uniffi`. A dependência e as anotações entram em M6, junto com o primeiro consumidor de binding.

A forma, explicitamente, e é isto que M6 vai anotar sem reescrever:

- Um objeto opaco por handle, sempre `Arc<Plug>`, nunca campos públicos.
- Nada de genéricos, nada de lifetimes, nada de referências emprestadas atravessando a fronteira.
- Erros como enum fechado. Nenhuma variante carrega texto livre.
- Eventos por trait `EventListener: Send + Sync`, entregues em thread própria.
- Tipos de valor (`Snapshot` e o que ele contém) usam `u64`, `u32`, `String`, `bool`, `Vec` e enums — **nunca** os newtypes de `seele-proto`. Um `PilotId` atravessando a fronteira é conhecimento de protocolo vazando para a casca, que é o que `specs/06` proíbe em uma frase: "Se o frontend precisa saber o que é um `ssrc`, algo está errado."

Alternativas:

1. **`uniffi` agora.** Honesto com a letra da spec e custa um passo de build sem consumidor. Se M6 começasse logo em seguida seria a escolha certa; não é o caso.
2. **FFI escrita à mão em `extern "C"`.** É o que o `uniffi` existe para evitar. Cada tipo vira código de marshalling manual e cada erro vira um segfault em vez de um `Result`.

Consequências:

- M6 anota, não reescreve — **se** a forma acima for respeitada. Isso é uma promessa que só um binding de verdade prova, e por isso está escrita aqui como lista verificável e não como intenção.
- O desktop não paga por geração de código que não consome.
- Risco assumido: alguma restrição do `uniffi` que eu não conheço aparece em M6. A mitigação é a lista acima ser conservadora — ela é mais restrita que o necessário para o Tauri.
- `specs/06-clientes-gui.md` deveria dizer *quando* o `uniffi` entra, não só que entra.

Custo de reverter: **baixo agora, alto depois de M6**. Enquanto o único consumidor for o Tauri, mudar a superfície é mudar um crate e quem o chama. Depois que houver Kotlin e Swift gerados em cima dela, cada mudança é três.
