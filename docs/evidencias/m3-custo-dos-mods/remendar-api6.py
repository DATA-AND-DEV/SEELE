#!/usr/bin/env python3
"""M3 da 0.16 — os remendos da cópia da medição, e só dela.

    python3 -I remendar-api6.py <fonte>

`<fonte>` é a cópia por `git archive` que `preparar-api6.sh` monta; o
repositório não muda. Cada troca confere que o texto de antes está lá **uma
vez**, e para com o nome dele se não estiver: um remendo que não pegou é uma
medida de outra coisa.

1. **A API 6 aceita só aqui** (decisão da orquestradora, 10/10):
   `MOD_API_VERSION = 6` e `APIS_ACEITAS = [6, 5, 4, 3]` no `seele-proto`, e as
   duas constantes da casca que dizem isso na gestão (`camada-mods.js`). No
   produto, a 6 só entra com o `v6.json` (tarefa 12).
2. **O jitter no `seele.log`, a cada 2 s** (tarefa 11, Step 2): com o diálogo
   aberto e com a página imersiva, a barra de telemetria fica inerte, e um nó
   inerte sai da árvore de acessibilidade — a base não leu o jitter desses
   cenários. A linha leva o mesmo número que a barra mostra
   (`snapshot.telemetry.jitter_ms`), com o `modId` `m3`, que tem balde próprio
   no registro. Vale em **todos** os cenários desta campanha, para o custo dela
   ser o mesmo em todos.
3. **O servidor da medição na 0.16**: `EnabledMod` ganhou `api` (migração 14),
   e o `m3-servidor.rs` da base é o da v0.15.1.
4. **O falante com o número da voz da 0.16** (achado da F4 e da orquestradora,
   10/10). Desde o `f3aaf9ce`, o primeiro byte do datagrama de voz é o
   `MEDIA_HEADER_VERSION` (1), e não mais o `PROTOCOL_VERSION`, e o `decode`
   recusa outro número. O `m3-falante.rs` da base grava o `PROTOCOL_VERSION`,
   que é o certo na v0.15.1 (lá o número da voz era ele, e a constante nova não
   existe), e na 0.16 quem ouve jogava fora todo datagrama do falante: nos sete
   primeiros cenários com voz da campanha, nada foi decodificado, e o jitter
   saía 0 exato. Remendo, e não troca no arquivo, para a base continuar
   compilando com o mesmo `m3-falante.rs`.
"""

import pathlib
import sys

JITTER = """// M3 (só na cópia da medição): o jitter ao seele.log a cada 2 s, para
  // os cenários em que a barra fica inerte e a acessibilidade não a lê.
  if (snapshot?.telemetry && Date.now() - (globalThis.__m3Jitter ?? 0) >= 2000) {
    globalThis.__m3Jitter = Date.now();
    registrarNoAnfitriao("m3", `jitter_ms=${snapshot.telemetry.jitter_ms}`, "aviso", "m3");
  }
  """


def trocar(fonte, relativo, antes, depois):
    arquivo = fonte / relativo
    texto = arquivo.read_text()
    vezes = texto.count(antes)
    if vezes != 1:
        sys.exit(f"remendo não pegou em {relativo}: «{antes[:60]}» aparece {vezes} vez(es)")
    arquivo.write_text(texto.replace(antes, depois))
    print(f"remendado: {relativo}")


def main():
    if len(sys.argv) != 2:
        sys.exit("uso: remendar-api6.py <fonte>")
    fonte = pathlib.Path(sys.argv[1])
    trocar(fonte, "crates/seele-proto/src/mods.rs",
           "pub const MOD_API_VERSION: u32 = 5;", "pub const MOD_API_VERSION: u32 = 6;")
    trocar(fonte, "crates/seele-proto/src/mods.rs",
           "pub const APIS_ACEITAS: &[u32] = &[5, 4, 3];", "pub const APIS_ACEITAS: &[u32] = &[6, 5, 4, 3];")
    trocar(fonte, "apps/seele-app/ui/camada-mods.js",
           "const API_DESTE_APLICATIVO = 5;", "const API_DESTE_APLICATIVO = 6;")
    trocar(fonte, "apps/seele-app/ui/camada-mods.js",
           "const APIS_QUE_ESTE_APLICATIVO_ACEITA = [5,4,3];", "const APIS_QUE_ESTE_APLICATIVO_ACEITA = [6,5,4,3];")
    trocar(fonte, "apps/seele-app/ui/tela-sessao.js",
           "function desenharTelemetria(snapshot) {\n  ", "function desenharTelemetria(snapshot) {\n  " + JITTER)
    trocar(fonte, "crates/seele-server/examples/m3-servidor.rs",
           "server_half: manifesto.server.is_some(),",
           "server_half: manifesto.server.is_some(),\n                    api: manifesto.api,")
    trocar(fonte, "crates/seele-core/examples/m3-falante.rs",
           "version: seele_proto::PROTOCOL_VERSION,",
           "version: seele_proto::MEDIA_HEADER_VERSION,")


if __name__ == "__main__":
    main()
