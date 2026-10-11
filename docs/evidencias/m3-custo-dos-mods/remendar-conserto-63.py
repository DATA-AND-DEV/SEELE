#!/usr/bin/env python3
"""M3 da 0.16 — o conserto da pendência 63 na cópia da campanha, e só nela.

    python3 -I remendar-conserto-63.py <fonte>

`<fonte>` é a cópia da campanha da API 6 (`preparar-api6.sh`, do `6e845e0d`),
que `preparar-conserto-63.sh` chama; o repositório não muda. Como em
`remendar-api6.py`, cada troca confere que o texto de antes está lá **uma vez**,
e para com o nome dele se não estiver: um remendo que não pegou é uma medida de
outra coisa.

1. **No máximo um `atualizar()` por quadro** (`6717dc2c`, decisão da
   orquestradora, 10/10). A `carga-cartoes` travou a página: cada pessoa manda
   um `PersonState` por segundo a todas as conexões, e a janela respondia a cada
   um com o retrato inteiro e a lista inteira. O texto que entra é o do commit,
   letra por letra: a cópia mede o produto consertado, e não uma versão dele.
2. **O falante com o número da voz**, como o item 4 de `remendar-api6.py`, no
   `m3-falante.rs` que `preparar-conserto-63.sh` acabou de copiar (o do «diz e
   segue», `94fee0c0`). Com o campo `version` no `MediaHeader` da cópia, o
   número da voz no lugar do do protocolo; sem ele (a 0.16 depois do
   `5c26377a`, da F1), a linha sai, e o `encode` grava o número da voz.
"""

import pathlib
import sys

SESSAO = "apps/seele-app/ui/tela-sessao.js"

ESQUECER_ANTES = """  falouPorUltimo.clear();
  capasAgendadas = false;
"""
ESQUECER_DEPOIS = """  falouPorUltimo.clear();
  capasAgendadas = false;
  // O `atualizar()` que um evento deixou para o próximo quadro: ele pediria o
  // retrato depois da saída. O quadro pendente também é recurso.
  if (quadroDaAtualizacao) cancelAnimationFrame(quadroDaAtualizacao);
  quadroDaAtualizacao = 0;
"""

OUVINTE_ANTES = """// O core diz o que mudou; isto só redesenha. Um snapshot por evento é barato e
// evita que a tela e o estado discordem.
listen("seele://event", (evento) => {"""
OUVINTE_DEPOIS = """// O core diz o que mudou; isto só redesenha, pelo snapshot, para a tela e o
// estado não discordarem — e no máximo um por quadro (`atualizarNoProximoQuadro`):
// um por evento era barato com dez pessoas e travava a página com 251.
listen("seele://event", (evento) => {"""

FIM_ANTES = """      .catch((falha) => console.warn("busca:", falha));
    return;
  }

  atualizar();
});
"""
FIM_DEPOIS = """      .catch((falha) => console.warn("busca:", falha));
    return;
  }

  atualizarNoProximoQuadro();
});

/**
 * **Um `atualizar()` por quadro, por mais eventos que cheguem nele**
 * (pendência 63).
 *
 * Cada pessoa da sessão manda um `PersonState` por segundo a todas as
 * conexões, e cada um chega aqui como um `RosterChanged`. Com um `atualizar()`
 * por evento, N pessoas eram N retratos pela ponte e N listas inteiras
 * redesenhadas por segundo, e o custo crescia com o quadrado: na M3 de 10/10,
 * 251 pessoas com um cartão de MOD em cada travaram a página, com a fila
 * crescendo enquanto houvesse sessão (`tools/lista-grande.cjs`: 4,9 s de
 * atraso em 5 s). Um quadro não mostra mais que um desenho, e o retrato pedido
 * no quadro é o mais novo, não um velho: os eventos que chegam no meio só
 * dizem que há o que pedir.
 *
 * Com a janela escondida não há quadro, e o pedido espera por um: o tique de
 * 500 ms continua puxando o retrato, como sempre puxou.
 */
let quadroDaAtualizacao = 0;
function atualizarNoProximoQuadro() {
  if (quadroDaAtualizacao) return;
  quadroDaAtualizacao = requestAnimationFrame(() => {
    quadroDaAtualizacao = 0;
    atualizar();
  });
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
        sys.exit("uso: remendar-conserto-63.py <fonte>")
    fonte = pathlib.Path(sys.argv[1])
    # Uma preparação que parou no meio (a compilação falhou) roda de novo: o
    # conserto que já entrou fica, e o falante, que é copiado de novo, é
    # remendado de novo. Quem impede remendar uma cópia terminada é o
    # `COMMIT_DA_MEDICAO` marcado, em `preparar-conserto-63.sh`.
    if "atualizarNoProximoQuadro" in (fonte / SESSAO).read_text():
        print(f"o conserto da pendência 63 já está em {SESSAO}; segue")
    else:
        trocar(fonte, SESSAO, ESQUECER_ANTES, ESQUECER_DEPOIS)
        trocar(fonte, SESSAO, OUVINTE_ANTES, OUVINTE_DEPOIS)
        trocar(fonte, SESSAO, FIM_ANTES, FIM_DEPOIS)
    if "pub version: u8" in (fonte / "crates/seele-proto/src/media.rs").read_text():
        trocar(fonte, "crates/seele-core/examples/m3-falante.rs",
               "version: seele_proto::PROTOCOL_VERSION,",
               "version: seele_proto::MEDIA_HEADER_VERSION,")
    else:
        trocar(fonte, "crates/seele-core/examples/m3-falante.rs",
               "            version: seele_proto::PROTOCOL_VERSION,\n", "")


if __name__ == "__main__":
    main()
